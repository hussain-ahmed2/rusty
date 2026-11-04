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
                    <meta name="description" content="Rusty is a Rust-native full-stack framework with file-based routing, streaming SSR, and client navigation.">
                    <meta name="rusty-base" content={base}>
                    <meta property="og:site_name" content="Rusty">
                    <meta property="og:type" content="website">
                    <meta property="og:title" content="Rusty — the Rust-native web framework">
                    <meta property="og:description" content="Rusty is a Rust-native full-stack framework with file-based routing, streaming SSR, and client navigation.">
                    <meta name="twitter:card" content="summary">
                    <meta name="twitter:title" content="Rusty — the Rust-native web framework">
                    <meta name="twitter:description" content="Rusty is a Rust-native full-stack framework with file-based routing, streaming SSR, and client navigation.">
                    <title>"Rusty — the Rust-native web framework"</title>
                    <link rel="icon" href={icon} type="image/svg+xml">
                    {Element::raw_html(format!("<style>{styles}</style>"))}
                </head>
                <body>{crate::chrome::body(children.into())}</body>
            </html>
        },
    ])
}
