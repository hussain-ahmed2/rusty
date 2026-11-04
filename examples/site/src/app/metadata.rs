//! Root document metadata.

use rusty::prelude::*;

const DESCRIPTION: &str = "Rusty is a Rust-native full-stack framework with file-based routing, streaming SSR, and client navigation.";
const TITLE: &str = "Rusty — the Rust-native web framework";

/// Supplies metadata for every page in the site.
pub fn metadata(_context: &mut RequestContext) -> Metadata {
    Metadata::new()
        .with_title(TITLE)
        .with_description(DESCRIPTION)
        .with_property("og:site_name", "Rusty")
        .with_property("og:type", "website")
        .with_property("og:title", TITLE)
        .with_property("og:description", DESCRIPTION)
        .with_name("twitter:card", "summary")
        .with_name("twitter:title", TITLE)
        .with_name("twitter:description", DESCRIPTION)
}
