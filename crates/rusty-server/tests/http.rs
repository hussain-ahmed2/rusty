//! HTTP, routing, middleware, authentication, and CSRF integration tests.

#![forbid(unsafe_code)]

use std::sync::{Arc, Mutex};

use rusty_server::{
    authenticate, csrf, require_auth, AuthIdentity, BearerAuthenticator, CsrfToken, Extensions,
    Headers, HttpRouter, Method, Middleware, Next, Request, RequestContext, Response, ServerError,
    StatusCode, CSRF_COOKIE, CSRF_HEADER,
};

#[test]
fn parses_and_validates_http_types() {
    assert_eq!("PATCH".parse::<Method>().expect("method"), Method::Patch);
    assert!(matches!(
        "TRACE".parse::<Method>(),
        Err(ServerError::InvalidMethod(_))
    ));
    assert_eq!(StatusCode::CREATED.as_u16(), 201);
    assert_eq!(StatusCode::OK.canonical_reason(), Some("OK"));
    assert!(StatusCode::new(99).is_err());
    assert!(StatusCode::new(600).is_err());

    let mut headers = Headers::new();
    headers
        .insert("Content-Type", "text/plain")
        .expect("header");
    headers.append("Set-Cookie", "a=1").expect("first cookie");
    headers.append("set-cookie", "b=2").expect("second cookie");
    assert_eq!(headers.get("content-type"), Some("text/plain"));
    assert_eq!(headers.get_all("SET-COOKIE"), ["a=1", "b=2"]);
    assert!(headers.contains("Content-Type"));
    assert!(headers.insert("bad name", "value").is_err());
    assert!(headers.insert("x-test", "bad\nvalue").is_err());

    let request = Request::new(Method::Get, "/search?q=rust#results")
        .with_header("accept", "text/html")
        .expect("header");
    assert_eq!(request.path(), "/search");
    assert_eq!(request.query(), Some("q=rust"));
    assert_eq!(request.query_param("q"), Some("rust"));
    assert_eq!(request.query_param("missing"), None);
    assert_eq!(request.headers().get("accept"), Some("text/html"));

    let response = Response::text(StatusCode::OK, "hello");
    assert_eq!(response.body(), b"hello");
    assert_eq!(
        response.headers().get("content-type"),
        Some("text/plain; charset=utf-8")
    );
}

#[test]
fn collects_streaming_bodies_for_transport_neutral_callers() {
    let response =
        Response::stream_html(StatusCode::OK, ["hello ".to_owned(), "stream".to_owned()]);
    assert!(response.is_streaming());
    assert_eq!(response.body(), b"hello stream");
    assert_eq!(response.into_parts().2, b"hello stream");
}

#[test]
fn routes_most_specific_matches_and_reports_missing_methods() {
    let app = HttpRouter::new()
        .get("/users/[id]", |context: &mut RequestContext| {
            Ok(Response::text(
                StatusCode::OK,
                format!("user {}", context.param("id").unwrap_or_default()),
            ))
        })
        .expect("dynamic route")
        .get("/users/me", |_context: &mut RequestContext| {
            Ok(Response::text(StatusCode::OK, "current user"))
        })
        .expect("static route")
        .get("/files/[...path]", |context: &mut RequestContext| {
            Ok(Response::text(
                StatusCode::OK,
                context.param("path").unwrap_or_default(),
            ))
        })
        .expect("catch-all route")
        .post("/users", |_context: &mut RequestContext| {
            Ok(Response::text(StatusCode::CREATED, "created"))
        })
        .expect("post route");

    let current = app
        .handle(Request::new(Method::Get, "/users/me"))
        .expect("handle");
    assert_eq!(current.body(), b"current user");

    let user = app
        .handle(Request::new(Method::Get, "/users/42?expand=true"))
        .expect("handle");
    assert_eq!(user.body(), b"user 42");

    let files = app
        .handle(Request::new(Method::Get, "/files/guides/rust"))
        .expect("handle");
    assert_eq!(files.body(), b"guides/rust");

    let wrong_method = app
        .handle(Request::new(Method::Get, "/users"))
        .expect("handle");
    assert_eq!(wrong_method.status(), StatusCode::METHOD_NOT_ALLOWED);

    let missing = app
        .handle(Request::new(Method::Get, "/missing"))
        .expect("handle");
    assert_eq!(missing.status(), StatusCode::NOT_FOUND);

    assert!(matches!(
        HttpRouter::new()
            .get("/users/[id]", |_| Ok(Response::new(StatusCode::OK)))
            .expect("first")
            .get("/users/[name]", |_| Ok(Response::new(StatusCode::OK))),
        Err(ServerError::Route(_))
    ));
}

#[test]
fn custom_fallback_receives_the_request_context() {
    let app = HttpRouter::new().fallback(|context: &mut RequestContext| {
        Ok(Response::text(
            StatusCode::NOT_FOUND,
            format!("missing {}", context.request().path()),
        ))
    });

    let response = app
        .handle(Request::new(Method::Get, "/nope"))
        .expect("handle");
    assert_eq!(response.body(), b"missing /nope");
}

#[test]
fn middleware_runs_in_order_and_can_short_circuit() {
    let trace = Arc::new(Mutex::new(Vec::<String>::new()));
    let outer_trace = Arc::clone(&trace);
    let outer: Middleware = Arc::new(move |context: &mut RequestContext, next: Next<'_>| {
        outer_trace
            .lock()
            .expect("trace")
            .push("outer before".to_owned());
        let response = next.run(context)?;
        outer_trace
            .lock()
            .expect("trace")
            .push("outer after".to_owned());
        Ok(response)
    });
    let inner_trace = Arc::clone(&trace);
    let inner: Middleware = Arc::new(move |context: &mut RequestContext, next: Next<'_>| {
        inner_trace
            .lock()
            .expect("trace")
            .push("inner before".to_owned());
        let response = next.run(context)?;
        inner_trace
            .lock()
            .expect("trace")
            .push("inner after".to_owned());
        Ok(response)
    });

    let app = HttpRouter::new()
        .middleware(outer)
        .middleware(inner)
        .get("/", |context: &mut RequestContext| {
            context.extensions_mut().insert("handler state".to_owned());
            Ok(Response::text(StatusCode::OK, "ok"))
        })
        .expect("route");

    let response = app.handle(Request::new(Method::Get, "/")).expect("handle");
    assert_eq!(response.body(), b"ok");
    assert_eq!(
        trace.lock().expect("trace").as_slice(),
        ["outer before", "inner before", "inner after", "outer after"]
    );

    let blocked: Middleware = Arc::new(|_context: &mut RequestContext, _next: Next<'_>| {
        Ok(Response::text(StatusCode::FORBIDDEN, "blocked"))
    });
    let app = HttpRouter::new()
        .middleware(blocked)
        .get("/", |_| Ok(Response::text(StatusCode::OK, "unreachable")))
        .expect("route");
    let response = app.handle(Request::new(Method::Get, "/")).expect("handle");
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    assert_eq!(response.body(), b"blocked");
}

#[test]
fn bearer_authentication_attaches_and_requires_identities() {
    let auth = BearerAuthenticator::new().with_token(
        "secret-token",
        AuthIdentity::new("user-1").with_role("admin"),
    );
    let app = HttpRouter::new()
        .middleware(require_auth(auth))
        .get("/me", |context: &mut RequestContext| {
            let identity = context.require_identity()?;
            Ok(Response::text(
                StatusCode::OK,
                format!(
                    "{} admin={}",
                    identity.subject(),
                    identity.has_role("admin")
                ),
            ))
        })
        .expect("route");

    let anonymous = app
        .handle(Request::new(Method::Get, "/me"))
        .expect("handle");
    assert_eq!(anonymous.status(), StatusCode::UNAUTHORIZED);

    let request = Request::new(Method::Get, "/me")
        .with_header("authorization", "Bearer secret-token")
        .expect("header");
    let response = app.handle(request).expect("handle");
    assert_eq!(response.body(), b"user-1 admin=true");
}

#[test]
fn optional_authentication_keeps_anonymous_requests_going() {
    let auth = BearerAuthenticator::new().with_token("token", AuthIdentity::new("user"));
    let app = HttpRouter::new()
        .middleware(authenticate(auth))
        .get("/", |context: &mut RequestContext| {
            Ok(Response::text(
                StatusCode::OK,
                context
                    .identity()
                    .map_or("anonymous", |identity| identity.subject()),
            ))
        })
        .expect("route");

    let anonymous = app.handle(Request::new(Method::Get, "/")).expect("handle");
    assert_eq!(anonymous.body(), b"anonymous");

    let authenticated = app
        .handle(
            Request::new(Method::Get, "/")
                .with_header("authorization", "Bearer token")
                .expect("header"),
        )
        .expect("handle");
    assert_eq!(authenticated.body(), b"user");
}

#[test]
fn csrf_issues_tokens_and_validates_unsafe_requests() {
    let app = HttpRouter::new()
        .middleware(csrf())
        .get("/form", |context: &mut RequestContext| {
            let token = context
                .extensions()
                .get::<CsrfToken>()
                .expect("issued token");
            Ok(Response::text(StatusCode::OK, token.as_str()))
        })
        .expect("form route")
        .post("/submit", |_context: &mut RequestContext| {
            Ok(Response::text(StatusCode::OK, "accepted"))
        })
        .expect("submit route");

    let form = app
        .handle(Request::new(Method::Get, "/form"))
        .expect("handle");
    let body = String::from_utf8(form.body().to_vec()).expect("utf8");
    assert!(!body.is_empty());
    let cookie = form.headers().get("set-cookie").expect("set-cookie header");
    assert!(cookie.starts_with(&format!("{CSRF_COOKIE}={body};")));

    let accepted = app
        .handle(
            Request::new(Method::Post, "/submit")
                .with_header("cookie", format!("{CSRF_COOKIE}={body}"))
                .expect("cookie")
                .with_header(CSRF_HEADER, body.clone())
                .expect("csrf header"),
        )
        .expect("handle");
    assert_eq!(accepted.status(), StatusCode::OK);

    let missing = app
        .handle(
            Request::new(Method::Post, "/submit")
                .with_header("cookie", format!("{CSRF_COOKIE}={body}"))
                .expect("cookie"),
        )
        .expect("handle");
    assert_eq!(missing.status(), StatusCode::FORBIDDEN);

    let mismatch = app
        .handle(
            Request::new(Method::Post, "/submit")
                .with_header("cookie", format!("{CSRF_COOKIE}={body}"))
                .expect("cookie")
                .with_header(CSRF_HEADER, "different")
                .expect("csrf header"),
        )
        .expect("handle");
    assert_eq!(mismatch.status(), StatusCode::FORBIDDEN);
}

#[test]
fn extensions_are_type_keyed() {
    let mut extensions = Extensions::new();
    assert!(extensions.is_empty());
    assert_eq!(extensions.insert::<u32>(7), None, "no previous u32");
    assert_eq!(extensions.insert::<u32>(9), Some(7));
    assert_eq!(extensions.get::<u32>(), Some(&9));
    assert!(extensions.get::<String>().is_none());
    assert_eq!(extensions.remove::<u32>(), Some(9));
    assert!(extensions.is_empty());
}
