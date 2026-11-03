//! Root layout.

use rusty::prelude::*;

/// Wraps every page in the application document.
pub fn layout(children: Children) -> Element {
    Element::fragment([
        Element::raw_html("<!doctype html>"),
        view! {
            <html>
                <head><title>"Rusty app"</title></head>
                <body>{children}</body>
            </html>
        },
    ])
}
