//! Route discovery and matching integration tests.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use rusty_router::{RouteManifest, RoutePattern, RouteSegment, RouterError};

struct TempApp {
    root: PathBuf,
}

impl TempApp {
    fn new(label: &str) -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let unique = NEXT.fetch_add(1, Ordering::Relaxed);
        let root = std::env::temp_dir().join(format!(
            "rusty-router-{}-{label}-{unique}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).expect("create temp app");
        Self { root }
    }

    fn file(&self, relative: &str) -> PathBuf {
        let path = self.root.join(relative);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("create fixture directory");
        }
        fs::write(&path, "").expect("create fixture file");
        path
    }
}

impl Drop for TempApp {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn nested_app() -> TempApp {
    let app = TempApp::new("nested");
    for file in [
        "layout.rs",
        "metadata.rs",
        "page.rs",
        "loading.rs",
        "error.rs",
        "not_found.rs",
        "about/page.rs",
        "blog/layout.rs",
        "blog/metadata.rs",
        "blog/[slug]/page.rs",
        "blog/[slug]/loading.rs",
        "docs/[...path]/page.rs",
        "(marketing)/layout.rs",
        "(marketing)/pricing/page.rs",
        "_private/page.rs",
        "components.rs",
        "api/health/route.rs",
        "api/users/[id]/route.rs",
        "api/[...path]/route.rs",
    ] {
        app.file(file);
    }
    app
}

#[test]
fn discovers_pages_layouts_boundaries_and_groups() {
    let app = nested_app();
    let manifest = RouteManifest::discover(&app.root).expect("discover");

    let paths = manifest
        .routes()
        .iter()
        .map(|route| route.pattern().to_string())
        .collect::<Vec<_>>();
    assert_eq!(
        paths,
        ["/", "/about", "/blog/[slug]", "/docs/[...path]", "/pricing"]
    );

    let root = &manifest.routes()[0];
    assert_eq!(root.page(), Path::new("page.rs"));
    assert_eq!(root.layouts(), [PathBuf::from("layout.rs")]);
    assert_eq!(root.metadata(), [PathBuf::from("metadata.rs")]);
    assert_eq!(root.loading(), Some(Path::new("loading.rs")));
    assert_eq!(root.error(), Some(Path::new("error.rs")));
    assert_eq!(root.not_found(), Some(Path::new("not_found.rs")));

    let blog = manifest
        .routes()
        .iter()
        .find(|route| route.pattern().to_string() == "/blog/[slug]")
        .expect("blog route");
    assert_eq!(
        blog.layouts(),
        [PathBuf::from("layout.rs"), PathBuf::from("blog/layout.rs")]
    );
    assert_eq!(
        blog.metadata(),
        [
            PathBuf::from("metadata.rs"),
            PathBuf::from("blog/metadata.rs")
        ]
    );
    assert_eq!(blog.loading(), Some(Path::new("blog/[slug]/loading.rs")));

    let pricing = manifest
        .routes()
        .iter()
        .find(|route| route.pattern().to_string() == "/pricing")
        .expect("pricing route");
    assert_eq!(
        pricing.layouts(),
        [
            PathBuf::from("layout.rs"),
            PathBuf::from("(marketing)/layout.rs")
        ]
    );

    assert_eq!(manifest.not_found(), Some(Path::new("not_found.rs")));
}

#[test]
fn matches_static_dynamic_and_catch_all_routes() {
    let app = nested_app();
    for file in [
        "shop/[...path]/page.rs",
        "shop/[category]/page.rs",
        "shop/new/page.rs",
        "[section]/new/page.rs",
    ] {
        app.file(file);
    }
    let manifest = RouteManifest::discover(&app.root).expect("discover");

    let root = manifest.match_path("/").expect("root");
    assert!(root.params().is_empty());

    let blog = manifest.match_path("/blog/hello?from=test").expect("blog");
    assert_eq!(blog.entry().pattern().to_string(), "/blog/[slug]");
    assert_eq!(blog.params().get("slug"), Some("hello"));

    let docs = manifest.match_path("/docs/guides/rust/").expect("docs");
    assert_eq!(docs.entry().pattern().to_string(), "/docs/[...path]");
    assert_eq!(docs.params().get("path"), Some("guides/rust"));

    assert_eq!(
        manifest
            .match_path("/shop/new")
            .expect("static")
            .entry()
            .pattern()
            .to_string(),
        "/shop/new"
    );
    assert_eq!(
        manifest
            .match_path("/shop/books")
            .expect("dynamic")
            .entry()
            .pattern()
            .to_string(),
        "/shop/[category]"
    );
    assert_eq!(
        manifest
            .match_path("/shop/books/rust")
            .expect("catch-all")
            .entry()
            .pattern()
            .to_string(),
        "/shop/[...path]"
    );

    assert!(manifest.match_path("/blog").is_none());
    assert!(manifest.match_path("/docs").is_none());
    assert!(manifest.match_path("relative/path").is_none());
}

#[test]
fn discovers_and_matches_api_routes() {
    let app = nested_app();
    let manifest = RouteManifest::discover(&app.root).expect("discover");

    let paths = manifest
        .api_routes()
        .iter()
        .map(|route| route.pattern().to_string())
        .collect::<Vec<_>>();
    assert_eq!(paths, ["/api/[...path]", "/api/health", "/api/users/[id]"]);

    let health = manifest.match_api_path("/api/health").expect("health");
    assert_eq!(health.route().route(), Path::new("api/health/route.rs"));
    assert!(health.params().is_empty());

    let user = manifest
        .match_api_path("/api/users/42?expand=true")
        .expect("user");
    assert_eq!(user.route().route(), Path::new("api/users/[id]/route.rs"));
    assert_eq!(user.params().get("id"), Some("42"));

    let fallback = manifest
        .match_api_path("/api/missing/nested")
        .expect("catch-all");
    assert_eq!(
        fallback.route().route(),
        Path::new("api/[...path]/route.rs")
    );
    assert_eq!(fallback.params().get("path"), Some("missing/nested"));
}

#[test]
fn rejects_a_page_and_api_route_with_the_same_shape() {
    let app = TempApp::new("page-api-duplicate");
    app.file("status/page.rs");
    app.file("status/route.rs");

    assert!(matches!(
        RouteManifest::discover(&app.root),
        Err(RouterError::DuplicateRoute { .. })
    ));
}

#[test]
fn manifests_round_trip_through_json() {
    let app = nested_app();
    let manifest = RouteManifest::discover(&app.root).expect("discover");
    let json = manifest.to_json_pretty().expect("serialize");
    let restored = RouteManifest::from_json(&json).expect("restore");

    assert_eq!(restored, manifest);
    assert_eq!(
        restored
            .match_path("/blog/hello")
            .expect("restored match")
            .params()
            .get("slug"),
        Some("hello")
    );
}

#[test]
fn rejects_duplicate_routes_across_groups() {
    let app = TempApp::new("duplicates");
    app.file("(one)/about/page.rs");
    app.file("(two)/about/page.rs");

    assert!(matches!(
        RouteManifest::discover(&app.root),
        Err(RouterError::DuplicateRoute { .. })
    ));
}

#[test]
fn rejects_invalid_route_directories() {
    let optional = TempApp::new("optional");
    optional.file("[[...path]]/page.rs");
    assert!(matches!(
        RouteManifest::discover(&optional.root),
        Err(RouterError::InvalidRoute { .. })
    ));

    let nested = TempApp::new("catch-all-child");
    nested.file("docs/[...path]/page.rs");
    nested.file("docs/[...path]/child/page.rs");
    assert!(matches!(
        RouteManifest::discover(&nested.root),
        Err(RouterError::InvalidRoute { .. })
    ));
}

#[test]
fn rejects_invalid_serialized_manifests() {
    let json = r#"{
        "routes": [{
            "pattern": {
                "segments": [
                    { "kind": "catch_all", "name": "rest" },
                    { "kind": "static", "value": "child" }
                ]
            },
            "page": "page.rs"
        }]
    }"#;

    assert!(matches!(
        RouteManifest::from_json(json),
        Err(RouterError::InvalidPattern { .. })
    ));
}

#[test]
fn validates_manually_built_patterns() {
    assert!(RoutePattern::new(vec![
        RouteSegment::catch_all("rest").expect("catch-all"),
        RouteSegment::dynamic("slug").expect("dynamic"),
    ])
    .is_err());

    assert!(RoutePattern::new(vec![
        RouteSegment::dynamic("value").expect("dynamic"),
        RouteSegment::dynamic("value").expect("dynamic"),
    ])
    .is_err());

    let parsed = RoutePattern::parse("/docs/[...path]").expect("parse");
    assert_eq!(parsed.to_string(), "/docs/[...path]");
    assert!(parsed.same_shape(&RoutePattern::parse("/docs/[...other]").expect("parse")));
    assert!(RoutePattern::parse("docs").is_err());
    assert!(RoutePattern::parse("/docs//intro").is_err());
    assert!(RoutePattern::parse("/docs?lang=en").is_err());
    assert!(RoutePattern::parse("/docs/[...]").is_err());

    assert_eq!(
        RoutePattern::new(vec![
            RouteSegment::static_segment("blog").expect("static"),
            RouteSegment::dynamic("slug").expect("dynamic"),
        ])
        .expect("pattern")
        .to_string(),
        "/blog/[slug]"
    );
}
