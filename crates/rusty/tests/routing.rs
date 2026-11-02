//! Routing integration tests for the facade.

use std::fs;

use rusty::prelude::*;

#[test]
fn discovers_and_matches_routes_through_the_facade() {
    let root = std::env::temp_dir().join(format!("rusty-facade-routing-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(root.join("blog/[slug]")).expect("create app");
    fs::write(root.join("page.rs"), "").expect("root page");
    fs::write(root.join("blog/[slug]/page.rs"), "").expect("blog page");

    let manifest = RouteManifest::discover(&root).expect("discover");
    let found = manifest.match_path("/blog/hello").expect("match");

    assert_eq!(found.entry().pattern().to_string(), "/blog/[slug]");
    assert_eq!(found.params().get("slug"), Some("hello"));
    assert_eq!(manifest.len(), 2);

    fs::remove_dir_all(&root).expect("clean app");
}
