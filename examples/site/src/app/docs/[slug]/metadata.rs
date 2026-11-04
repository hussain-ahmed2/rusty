//! Per-page documentation metadata.

use rusty::prelude::*;

/// Uses the selected document's title and summary.
pub fn metadata(context: &mut RequestContext) -> Metadata {
    let Some(doc) = context
        .param("slug")
        .and_then(crate::content::doc)
    else {
        return Metadata::new().with_title("Documentation — Rusty");
    };
    let title = format!("{} — Rusty", doc.title);
    Metadata::new()
        .with_title(&title)
        .with_description(doc.summary)
        .with_property("og:title", &title)
        .with_property("og:description", doc.summary)
        .with_name("twitter:title", &title)
        .with_name("twitter:description", doc.summary)
}
