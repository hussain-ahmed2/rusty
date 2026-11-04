//! File-based application runtime tests.

#![cfg(not(target_arch = "wasm32"))]
#![forbid(unsafe_code)]

use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use rusty::app::{
    ApiHandler, ApiMethods, AsyncApiFuture, AsyncPageFuture, Metadata, PageRoute, Router,
};
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

fn document_layout(children: Children) -> Element {
    Element::fragment([
        Element::raw_html("<!doctype html>"),
        view! {
            <html>
                <head>
                    <title>"Layout title"</title>
                    <meta name="description" content="Layout description">
                    <meta property="og:title" content="Layout title">
                    <meta name="twitter:card" content="player">
                    <link rel="canonical" href="https://example.test/layout">
                </head>
                <body>{children}</body>
            </html>
        },
    ])
}

fn root_metadata(_context: &mut RequestContext) -> Metadata {
    Metadata::new()
        .with_title("Root title")
        .with_description("Root description")
        .with_property("og:title", "Root title")
}

fn page_metadata(context: &mut RequestContext) -> Metadata {
    let slug = context.param("slug").unwrap_or("missing").to_owned();
    Metadata::new()
        .with_title(format!("Page {slug}"))
        .with_description("Page description")
        .with_canonical(format!("https://example.test/blog/{slug}"))
        .with_property("og:title", format!("Page {slug}"))
        .with_name("twitter:card", "summary")
}

fn plain_metadata(_context: &mut RequestContext) -> Metadata {
    Metadata::new()
        .with_description("A plain page")
        .with_name("robots", "noindex")
}

fn error_page(error: &ServerError) -> Element {
    view! { <p>{error.to_string()}</p> }
}

fn not_found() -> Element {
    view! { <p>"missing"</p> }
}

fn loading() -> Element {
    view! { <p>"Loading & more"</p> }
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

async fn async_home(_context: &mut RequestContext) -> Result<Element, ServerError> {
    let label = std::future::ready("async home").await;
    Ok(view! { <p>{label}</p> })
}

fn async_home_handler<'a>(context: &'a mut RequestContext) -> AsyncPageFuture<'a> {
    Box::pin(async_home(context))
}

async fn async_health(_context: &mut RequestContext) -> Result<Response, ServerError> {
    let status = std::future::ready("async").await;
    Ok(Response::json(
        StatusCode::OK,
        format!(r#"{{"status":"{status}"}}"#),
    ))
}

fn async_health_handler<'a>(context: &'a mut RequestContext) -> AsyncApiFuture<'a> {
    Box::pin(async_health(context))
}

fn router() -> Router {
    Router::new()
        .page(PageRoute::new("/", home).layouts(&[root_layout]))
        .page(PageRoute::new("/blog/[slug]", slug))
        .page(PageRoute::new("/broken", broken).error(error_page))
        .page(PageRoute::async_new("/async", Arc::new(async_home_handler)))
        .api(
            "/api/health",
            ApiMethods {
                get: Some(ApiHandler::Sync(health)),
                ..ApiMethods::default()
            },
        )
        .api(
            "/api/async",
            ApiMethods {
                get: Some(ApiHandler::Async(Arc::new(async_health_handler))),
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
    assert!(response.is_streaming());
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
fn metadata_is_inherited_and_replaces_layout_head_tags() {
    let router = Router::new().page(
        PageRoute::new("/blog/[slug]", slug)
            .layouts(&[document_layout])
            .metadata(&[root_metadata, page_metadata]),
    );

    let response = router
        .handle(Request::new(Method::Get, "/blog/rusty"))
        .expect("page");
    let body = String::from_utf8_lossy(response.body());

    assert_eq!(body.matches("<title>").count(), 1);
    assert!(body.contains("<title>Page rusty</title>"));
    assert!(body.contains("<meta name=\"description\" content=\"Page description\">"));
    assert!(body.contains("<link rel=\"canonical\" href=\"https://example.test/blog/rusty\">"));
    assert!(body.contains("<meta property=\"og:title\" content=\"Page rusty\">"));
    assert!(body.contains("<meta name=\"twitter:card\" content=\"summary\">"));
    assert!(!body.contains("Layout title"));
    assert!(!body.contains("Layout description"));
    assert!(!body.contains("https://example.test/layout"));
    assert!(!body.contains("content=\"player\""));
}

#[test]
fn metadata_without_a_title_keeps_the_default_document_title() {
    let router = Router::new().page(PageRoute::new("/plain", home).metadata(&[plain_metadata]));

    let response = router
        .handle(Request::new(Method::Get, "/plain"))
        .expect("page");
    let body = String::from_utf8_lossy(response.body());

    assert!(body.contains("<title>Rusty</title>"));
    assert!(body.contains("<meta name=\"description\" content=\"A plain page\">"));
    assert!(body.contains("<meta name=\"robots\" content=\"noindex\">"));
}

#[test]
fn awaits_async_pages_and_api_handlers() {
    let router = router();

    let response = router
        .handle(Request::new(Method::Get, "/async"))
        .expect("async page");
    assert_eq!(response.status(), StatusCode::OK);
    assert!(String::from_utf8_lossy(response.body()).contains("<p>async home</p>"));

    let response = router
        .handle(Request::new(Method::Get, "/api/async"))
        .expect("async api");
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.body(), br#"{"status":"async"}"#);
}

#[test]
fn middleware_can_short_circuit_or_continue() {
    fn middleware(context: &mut RequestContext) -> Result<Option<Response>, ServerError> {
        if context.request().path() == "/private" {
            return Ok(Some(Response::text(StatusCode::FORBIDDEN, "private")));
        }
        Ok(None)
    }

    let router = Router::new()
        .middleware(middleware)
        .page(PageRoute::new("/", home));

    let response = router
        .handle(Request::new(Method::Get, "/private"))
        .expect("middleware response");
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    assert_eq!(response.body(), b"private");

    let response = router
        .handle(Request::new(Method::Get, "/"))
        .expect("continued request");
    assert_eq!(response.status(), StatusCode::OK);
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
fn builds_client_navigation_data_with_specific_patterns() {
    let router = Router::new()
        .page(PageRoute::new("/blog/[slug]", slug).loading(loading))
        .page(PageRoute::new("/blog/featured", home).loading(loading))
        .page(PageRoute::new("/docs/[...path]", home).loading(loading));

    let response = router
        .handle(Request::new(Method::Get, "/blog/featured"))
        .expect("featured");
    let body = String::from_utf8_lossy(response.body());

    assert!(body.contains("id=\"__rusty_navigation_data\""));
    assert!(body.contains("window.__rustyNavigation"));
    assert!(body.contains("\\u003cp\\u003eLoading \\u0026amp; more\\u003c/p\\u003e"));

    let static_index = body
        .find("\"source\":\"^/blog/featured$\"")
        .expect("static route");
    let dynamic_index = body
        .find("\"source\":\"^/blog/[^/]+$\"")
        .expect("dynamic route");
    assert!(static_index < dynamic_index);

    let catch_all_index = body
        .find("\"source\":\"^/docs/.+$\"")
        .expect("catch-all route");
    assert!(dynamic_index < catch_all_index);
}

#[test]
fn navigation_can_be_disabled_explicitly() {
    let router = Router::new()
        .page(PageRoute::new("/", home).loading(loading))
        .navigation(false);

    let response = router.handle(Request::new(Method::Get, "/")).expect("home");
    assert!(!String::from_utf8_lossy(response.body()).contains("__rusty_navigation_data"));
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
    assert!(body.contains("\"routes\":4"));
    assert!(body.contains("\"apiRoutes\":2"));
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
fn serves_conditional_asset_requests() {
    let public = TempDir::new("conditional");
    fs::write(public.path.join("hello.txt"), "hello static").expect("text");

    let router = Router::new().public_dir(&public.path);
    let full = router
        .handle(Request::new(Method::Get, "/hello.txt"))
        .expect("asset");
    assert_eq!(full.status(), StatusCode::OK);
    let etag = full.headers().get("etag").expect("etag").to_owned();
    assert!(etag.starts_with('"') && etag.ends_with('"'));
    let last_modified = full.headers().get("last-modified").map(str::to_owned);

    let response = router
        .handle(
            Request::new(Method::Get, "/hello.txt")
                .with_header("if-none-match", etag.clone())
                .expect("header"),
        )
        .expect("conditional asset");
    assert_eq!(response.status(), StatusCode::NOT_MODIFIED);
    assert!(response.body().is_empty());
    assert_eq!(response.headers().get("etag"), Some(etag.as_str()));

    let response = router
        .handle(
            Request::new(Method::Get, "/hello.txt")
                .with_header("if-none-match", format!("W/{etag}"))
                .expect("header"),
        )
        .expect("weak conditional asset");
    assert_eq!(response.status(), StatusCode::NOT_MODIFIED);

    let response = router
        .handle(
            Request::new(Method::Get, "/hello.txt")
                .with_header("if-none-match", "\"different\"")
                .expect("header"),
        )
        .expect("stale conditional asset");
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.body(), b"hello static");

    if let Some(last_modified) = last_modified {
        let response = router
            .handle(
                Request::new(Method::Get, "/hello.txt")
                    .with_header("if-modified-since", last_modified)
                    .expect("header"),
            )
            .expect("modified-since asset");
        assert_eq!(response.status(), StatusCode::NOT_MODIFIED);
    }
}

#[test]
fn compresses_complete_responses_and_weakens_etags() {
    let page = Router::new()
        .page(PageRoute::new("/", home))
        .handle(
            Request::new(Method::Get, "/")
                .with_header("accept-encoding", "gzip")
                .expect("header"),
        )
        .expect("compressed page");
    assert!(page.is_streaming());
    assert_eq!(page.headers().get("content-encoding"), Some("gzip"));
    assert!(page.body().starts_with(&[0x1f, 0x8b]));

    let public = TempDir::new("compression");
    let body = "compressible static content\n".repeat(128);
    fs::write(public.path.join("large.txt"), &body).expect("large asset");

    let router = Router::new().public_dir(&public.path);
    let full = router
        .handle(
            Request::new(Method::Get, "/large.txt")
                .with_header("accept-encoding", "gzip")
                .expect("header"),
        )
        .expect("compressed asset");
    assert_eq!(full.status(), StatusCode::OK);
    assert_eq!(full.headers().get("content-encoding"), Some("gzip"));
    assert!(full.body().len() < body.len());
    let etag = full.headers().get("etag").expect("etag").to_owned();
    assert!(etag.starts_with("W/\""));
    assert!(full
        .headers()
        .get_all("vary")
        .iter()
        .any(|value| value.eq_ignore_ascii_case("Accept-Encoding")));

    let not_modified = router
        .handle(
            Request::new(Method::Get, "/large.txt")
                .with_header("accept-encoding", "gzip")
                .expect("header")
                .with_header("if-none-match", etag)
                .expect("header"),
        )
        .expect("conditional compressed asset");
    assert_eq!(not_modified.status(), StatusCode::NOT_MODIFIED);
    assert!(not_modified.body().is_empty());
    assert!(not_modified
        .headers()
        .get("etag")
        .is_some_and(|value| value.starts_with("W/\"")));
    assert!(not_modified
        .headers()
        .get_all("vary")
        .iter()
        .any(|value| value.eq_ignore_ascii_case("Accept-Encoding")));

    let disabled = Router::new()
        .public_dir(&public.path)
        .compression(false)
        .handle(
            Request::new(Method::Get, "/large.txt")
                .with_header("accept-encoding", "gzip")
                .expect("header"),
        )
        .expect("uncompressed asset");
    assert_eq!(disabled.headers().get("content-encoding"), None);
    assert_eq!(disabled.body(), body.as_bytes());
}

#[test]
fn serves_generated_client_artifacts_under_the_reserved_prefix() {
    let client = TempDir::new("client");
    fs::write(client.path.join("demo_client.js"), "export default {};").expect("script");
    fs::write(client.path.join("demo_client_bg.wasm"), b"\0asm").expect("wasm");

    let router = Router::new().client_dir(&client.path);
    let response = router
        .handle(Request::new(Method::Get, "/_rusty/client/demo_client.js"))
        .expect("client script");
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.body(), b"export default {};");
    assert_eq!(
        response.headers().get("content-type"),
        Some("text/javascript; charset=utf-8")
    );

    let response = router
        .handle(Request::new(
            Method::Get,
            "/_rusty/client/demo_client_bg.wasm",
        ))
        .expect("client wasm");
    assert_eq!(
        response.headers().get("content-type"),
        Some("application/wasm")
    );

    for target in [
        "/_rusty/client/../secret",
        "/_rusty/client/%2e%2e/secret",
        "/_rusty/clientish/demo_client.js",
        "/demo_client.js",
    ] {
        let response = router
            .handle(Request::new(Method::Get, target))
            .expect("missing client asset");
        assert_eq!(response.status(), StatusCode::NOT_FOUND, "{target}");
    }
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
