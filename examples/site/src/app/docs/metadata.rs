//! Documentation index metadata.

use rusty::prelude::*;

const TITLE: &str = "Documentation — Rusty";
const DESCRIPTION: &str =
    "Guides for routing, streaming, client navigation, static export, and deployment.";

/// Supplies metadata for the documentation index and its descendants.
pub fn metadata(_context: &mut RequestContext) -> Metadata {
    Metadata::new()
        .with_title(TITLE)
        .with_description(DESCRIPTION)
        .with_property("og:title", TITLE)
        .with_property("og:description", DESCRIPTION)
        .with_name("twitter:title", TITLE)
        .with_name("twitter:description", DESCRIPTION)
}
