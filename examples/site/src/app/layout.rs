//! Root document layout.

use rusty::prelude::*;

/// Wraps every page in the site shell.
pub fn layout(children: Children) -> Element {
    let base = crate::content::base_path();
    let styles = include_str!("../../public/styles.css");
    let icon = crate::content::url("favicon.svg");
    Element::fragment([
        Element::raw_html("<!doctype html>"),
        view! {
            <html lang="en">
                <head>
                    <meta charset="utf-8">
                    <meta name="viewport" content="width=device-width, initial-scale=1">
                    <meta name="rusty-base" content={base}>
                    <link rel="icon" href={icon} type="image/svg+xml">
                    {Element::raw_html(format!("<style>{styles}</style>"))}
                </head>
                <body>{crate::chrome::body(children.into())}</body>
            </html>
        },
    ])
}
