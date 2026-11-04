//! A file-based Rusty application.

#![forbid(unsafe_code)]

mod rpc;

#[rusty::app]
mod app {}

fn main() -> Result<(), rusty::app::ServeError> {
    app::serve()
}

#[cfg(test)]
mod tests {
    use rusty::prelude::*;

    #[test]
    fn generated_router_awaits_async_pages_and_apis() {
        let router = crate::app::router();

        let page = router
            .handle(Request::new(Method::Get, "/async"))
            .expect("async page");
        assert_eq!(page.status(), StatusCode::OK);
        let body = String::from_utf8_lossy(page.body());
        assert!(body.contains("Async page"));
        assert!(body.contains("id=\"__rusty_navigation_data\""));
        assert!(body.contains("Loading\u{2026}"));

        let api = router
            .handle(Request::new(Method::Get, "/api/async"))
            .expect("async api");
        assert_eq!(api.status(), StatusCode::OK);
        assert_eq!(api.body(), br#"{"status":"ok"}"#);
    }
}
