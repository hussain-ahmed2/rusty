//! Root document layout.

use rusty::prelude::*;

/// Wraps every page in the site shell.
pub fn layout(children: Children) -> Element {
    let base = crate::content::base_path();
    let styles = crate::content::url("styles.css");
    let icon = crate::content::url("favicon.svg");
    let home = crate::content::url("");
    let docs = crate::content::url("docs/");
    let about = crate::content::url("about/");
    Element::fragment([
        Element::raw_html("<!doctype html>"),
        view! {
            <html lang="en">
                <head>
                    <meta charset="utf-8">
                    <meta name="viewport" content="width=device-width, initial-scale=1">
                    <meta name="description" content="Rusty is a Rust-native full-stack framework with file-based routing, streaming SSR, and client navigation.">
                    <meta name="rusty-base" content={base}>
                    <title>"Rusty — the Rust-native web framework"</title>
                    <link rel="icon" href={icon} type="image/svg+xml">
                    <link rel="stylesheet" href={styles}>
                </head>
                <body>
                    <a class="skip-link" href="#main">"Skip to content"</a>
                    <header class="site-header">
                        <a class="brand" href={home}>
                            <span class="brand-mark">"R"</span>
                            <span>"Rusty"</span>
                        </a>
                        <nav aria-label="Primary">
                            <a href={docs}>"Docs"</a>
                            <a href={about}>"About"</a>
                            <a href="https://github.com/hussain-ahmed2/rusty" target="_blank" rel="external">"GitHub"</a>
                        </nav>
                    </header>
                    <div id="main" class="site-main">{children}</div>
                    <footer class="site-footer">
                        <p>"Built with Rusty. Static export deployed by GitHub Actions."</p>
                        <p>"MIT OR Apache-2.0"</p>
                    </footer>
                </body>
            </html>
        },
    ])
}
