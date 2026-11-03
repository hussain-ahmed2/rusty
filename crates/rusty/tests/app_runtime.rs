//! File-based application runtime tests.

#![cfg(not(target_arch = "wasm32"))]
#![forbid(unsafe_code)]

use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use rusty::app::{ApiMethods, PageRoute, Router};
use rusty::prelude::*;

struct TempDir {
    path: PathBuf,
}

impl TempDir {
    fn new(label: &str) -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let unique = NEXT.fetch_add(1, Ordering::Relaxed);
        let path =
            std::env::temp_dir().join(format!("rusty-app-{}-{label}-{unique}", std::process::id()));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).expect("create temp dir");
        Self { path }
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

fn home(_context: &mut RequestContext) -> Result<Element, ServerError> {
    Ok(view! { <p>"home"</p> })
}

fn slug(context: &mut RequestContext) -> Result<Element, ServerError> {
    let slug = context.param("slug").unwrap_or("missing").to_owned();
    Ok(view! { <p>{slug}</p> })
}

fn broken(_context: &mut RequestContext) -> Result<Element, ServerError> {
    Err(ServerError::Route("broken page".to_owned()))
}

fn root_layout(children: Children) -> Element {
    view! { <main>{children}</main> }
}

fn error_page(error: &ServerError) -> Element {
    view! { <p>{error.to_string()}</p> }
}

fn not_found() -> Element {
    view! { <p>"missing"</p> }
}

fn invalid(_context: &mut RequestContext) -> Result<Element, ServerError> {
    let tag: TagName = TagName::new("br").expect("valid tag");
    Ok(Element::html(tag, Vec::new(), vec![Element::text("x")]))
}

fn health(_context: &mut RequestContext) -> Result<Response, ServerError> {
    Ok(Response::json(
        StatusCode::OK,
        r#"{"status":"ok"}"#.to_owned(),
    ))
}

fn router() -> Router {
    Router::new()
        .page(PageRoute::new("/", home).layouts(&[root_layout]))
        .page(PageRoute::new("/blog/[slug]", slug))
        .page(PageRoute::new("/broken", broken).error(error_page))
        .api(
            "/api/health",
            ApiMethods {
                get: Some(health),
                ..ApiMethods::default()
            },
        )
        .not_found(not_found)
}

#[test]
fn renders_pages_with_layouts_and_parameters() {
    let router = router();

    let response = router.handle(Request::new(Method::Get, "/")).expect("home");
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.body(), b"<main><p>home</p></main>");

    let response = router
        .handle(Request::new(Method::Get, "/blog/rusty"))
        .expect("slug");
    assert_eq!(response.status(), StatusCode::OK);
    let body = String::from_utf8_lossy(response.body());
    assert!(body.starts_with("<!doctype html>"));
    assert!(body.contains("<p>rusty</p>"));
}

#[test]
fn rendering_failures_return_internal_server_error() {
    let router = Router::new().page(PageRoute::new("/invalid", invalid));
    let response = router
        .handle(Request::new(Method::Get, "/invalid"))
        .expect("invalid page");

    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
    assert_eq!(response.body(), b"Internal Server Error");
}

#[test]
fn dispatches_api_methods_and_rejects_missing_ones() {
    let router = router();

    let response = router
        .handle(Request::new(Method::Get, "/api/health"))
        .expect("health");
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.body(), br#"{"status":"ok"}"#);

    let response = router
        .handle(Request::new(Method::Post, "/api/health"))
        .expect("post");
    assert_eq!(response.status(), StatusCode::METHOD_NOT_ALLOWED);
}

#[test]
fn renders_error_and_not_found_boundaries() {
    let router = router();

    let response = router
        .handle(Request::new(Method::Get, "/broken"))
        .expect("broken");
    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
    assert!(String::from_utf8_lossy(response.body()).contains("broken page"));

    let response = router
        .handle(Request::new(Method::Get, "/missing"))
        .expect("not found");
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    assert!(String::from_utf8_lossy(response.body()).contains("missing"));
}

#[rusty::server("/api/echo")]
fn echo(value: String) -> Result<String, RpcError> {
    Ok(value)
}

#[test]
fn registers_server_functions() {
    let router = Router::new().rpc::<EchoServerFn>();
    let response = router
        .handle(
            Request::new(Method::Post, "/api/echo")
                .with_header("content-type", "application/json")
                .expect("header")
                .with_body(br#"["hello"]"#.to_vec()),
        )
        .expect("rpc");
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.body(), br#""hello""#);
}

#[test]
fn not_found_uses_layouts_when_provided() {
    let router = Router::new().not_found_with_layouts(not_found, &[root_layout]);
    let response = router
        .handle(Request::new(Method::Get, "/missing"))
        .expect("not found");
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    assert_eq!(response.body(), b"<main><p>missing</p></main>");
}

#[test]
fn diagnostics_are_opt_in() {
    let router = router();
    let response = router
        .handle(Request::new(Method::Get, "/__rusty/health"))
        .expect("disabled diagnostics");
    assert_eq!(response.status(), StatusCode::NOT_FOUND);

    let router = router.diagnostics(true);
    let response = router
        .handle(Request::new(Method::Get, "/__rusty/health"))
        .expect("health");
    assert_eq!(response.body(), b"ok");

    let response = router
        .handle(Request::new(Method::Get, "/__rusty/status"))
        .expect("status");
    let body = String::from_utf8_lossy(response.body());
    assert!(body.contains("\"routes\":3"));
    assert!(body.contains("\"apiRoutes\":1"));
}

#[test]
fn serves_public_assets_with_content_types_and_caching() {
    let public = TempDir::new("public");
    fs::write(public.path.join("hello.txt"), "hello static").expect("text");
    fs::write(public.path.join("index.html"), "<h1>index</h1>").expect("index");
    fs::create_dir_all(public.path.join("assets")).expect("assets dir");
    fs::write(public.path.join("assets/app.js"), "console.log('hi');").expect("script");

    let router = Router::new().public_dir(&public.path);
    let response = router
        .handle(Request::new(Method::Get, "/hello.txt"))
        .expect("text asset");
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.body(), b"hello static");
    assert_eq!(
        response.headers().get("content-type"),
        Some("text/plain; charset=utf-8")
    );
    assert_eq!(
        response.headers().get("cache-control"),
        Some("public, max-age=3600")
    );
    assert_eq!(
        response.headers().get("x-content-type-options"),
        Some("nosniff")
    );

    let response = router
        .handle(Request::new(Method::Get, "/assets/app.js"))
        .expect("script asset");
    assert_eq!(
        response.headers().get("content-type"),
        Some("text/javascript; charset=utf-8")
    );

    let response = router
        .handle(Request::new(Method::Get, "/"))
        .expect("index asset");
    assert_eq!(response.body(), b"<h1>index</h1>");

    let response = router
        .handle(Request::new(Method::Get, "/hello%2Etxt?download=1"))
        .expect("encoded asset");
    assert_eq!(response.body(), b"hello static");
}

#[test]
fn static_files_reject_traversal_and_fall_back_to_not_found() {
    let public = TempDir::new("public-traversal");
    fs::write(public.path.join("hello.txt"), "hello").expect("asset");

    let router = Router::new().public_dir(&public.path);
    for target in ["/../Cargo.toml", "/%2e%2e/Cargo.toml", "/missing.txt"] {
        let response = router
            .handle(Request::new(Method::Get, target))
            .expect("missing asset");
        assert_eq!(response.status(), StatusCode::NOT_FOUND, "{target}");
        assert_eq!(response.body(), b"not found");
    }

    let response = router
        .handle(Request::new(Method::Post, "/hello.txt"))
        .expect("post");
    assert_eq!(response.status(), StatusCode::NOT_FOUND);

    let router = Router::new().public_dir(&public.path).not_found(not_found);
    let response = router
        .handle(Request::new(Method::Get, "/missing.txt"))
        .expect("not found");
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    assert!(String::from_utf8_lossy(response.body()).contains("<p>missing</p>"));
}
