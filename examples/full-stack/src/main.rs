//! Exercises routing, middleware, authentication, CSRF, and server functions
//! through the transport-neutral HTTP runtime.

#![forbid(unsafe_code)]

use rusty::prelude::*;

#[server]
fn greet(name: String) -> Result<String, RpcError> {
    Ok(format!("Hello, {name}!"))
}

fn main() {
    let app_root = concat!(env!("CARGO_MANIFEST_DIR"), "/src/app");
    let manifest = RouteManifest::discover(app_root).expect("discover routes");
    for route in manifest.routes() {
        println!("page {} -> {}", route.pattern(), route.page().display());
    }
    for route in manifest.api_routes() {
        println!("api  {} -> {}", route.pattern(), route.route().display());
    }

    let mut registry = RpcRegistry::new();
    registry
        .register::<GreetServerFn>()
        .expect("register greet");

    let authenticator = BearerAuthenticator::new().with_token(
        "demo-token",
        AuthIdentity::new("demo-user").with_role("member"),
    );

    let app = HttpRouter::new()
        .middleware(csrf())
        .middleware(authenticate(authenticator))
        .get("/", |_context: &mut RequestContext| {
            Ok(Response::html(
                StatusCode::OK,
                "<!doctype html><h1>Rusty full stack</h1>",
            ))
        })
        .expect("root route")
        .get("/api/health", |_context: &mut RequestContext| {
            Ok(Response::json(
                StatusCode::OK,
                r#"{"status":"ok"}"#.to_owned(),
            ))
        })
        .expect("health route")
        .get("/me", |context: &mut RequestContext| {
            Ok(Response::text(
                StatusCode::OK,
                context
                    .identity()
                    .map_or("anonymous", |identity| identity.subject()),
            ))
        })
        .expect("me route")
        .post("/_rusty/rpc/greet", move |context: &mut RequestContext| {
            registry.handle(context.request())
        })
        .expect("greet route");

    let form = app
        .handle(Request::new(Method::Get, "/"))
        .expect("render form");
    let cookie = form.headers().get("set-cookie").expect("CSRF cookie");
    let token = cookie
        .split_once('=')
        .and_then(|(_, value)| value.split(';').next())
        .expect("CSRF token")
        .to_owned();

    let rpc = Request::new(Method::Post, "/_rusty/rpc/greet")
        .with_header("cookie", format!("{CSRF_COOKIE}={token}"))
        .expect("cookie")
        .with_header(CSRF_HEADER, token)
        .expect("csrf header")
        .with_header("authorization", "Bearer demo-token")
        .expect("authorization")
        .with_body(br#"["Rusty"]"#.to_vec());
    let response = app.handle(rpc).expect("call server function");
    println!(
        "rpc  {} {}",
        response.status(),
        String::from_utf8_lossy(response.body())
    );

    let anonymous = app
        .handle(Request::new(Method::Get, "/me"))
        .expect("anonymous request");
    println!(
        "me   {} {}",
        anonymous.status(),
        String::from_utf8_lossy(anonymous.body())
    );

    let blocked = app
        .handle(Request::new(Method::Post, "/_rusty/rpc/greet").with_body(br#"["Rusty"]"#.to_vec()))
        .expect("blocked request");
    println!(
        "csrf {} {}",
        blocked.status(),
        String::from_utf8_lossy(blocked.body())
    );
}
