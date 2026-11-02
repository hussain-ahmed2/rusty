//! Discovers and matches the routes in this example's `src/app` tree.

#![forbid(unsafe_code)]

use rusty::prelude::*;

fn main() {
    let app = concat!(env!("CARGO_MANIFEST_DIR"), "/src/app");
    let manifest = RouteManifest::discover(app).expect("discover routes");

    for route in manifest.routes() {
        println!("{} -> {}", route.pattern(), route.page().display());
    }
    for route in manifest.api_routes() {
        println!("{} -> {} (API)", route.pattern(), route.route().display());
    }

    let found = manifest.match_path("/blog/hello").expect("blog route");
    println!(
        "{} captured slug={:?}",
        found.entry().pattern(),
        found.params().get("slug")
    );
}
