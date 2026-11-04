//! Home page.

use rusty::prelude::*;

/// Renders the framework landing page.
pub fn page(_context: &mut RequestContext) -> Result<Element, ServerError> {
    let features = Element::fragment(vec![
        feature(
            "File-based routes",
            "Pages, layouts, loading states, errors, and API routes emerge from the src/app tree.",
        ),
        feature(
            "Streaming SSR",
            "Render pages into ordered chunks and send them with chunked transfer encoding.",
        ),
        feature(
            "Client navigation",
            "Fetch the next document, show its loading boundary, and swap without a blank screen.",
        ),
        feature(
            "Fine-grained reactivity",
            "Signals, memos, effects, ownership, and hydration share one typed element model.",
        ),
    ]);

    let get_started = crate::content::url("docs/getting-started/");
    let docs = crate::content::url("docs/");
    let static_export = crate::content::url("docs/static-export/");
    Ok(view! {
        <main>
            <section class="hero">
                <p class="eyebrow">"Rust-native full-stack"</p>
                <h1>"Build fast, typed web apps in Rust."</h1>
                <p class="lead">"File-based routing, server rendering, streaming, and client navigation without a JavaScript framework. The same components compile for the server and the browser."</p>
                <div class="actions">
                    <a class="button primary" href={get_started}>"Get started"</a>
                    <a class="button" href={docs}>"Explore the docs"</a>
                </div>
            </section>
            <section class="grid" aria-label="Framework features">{features}</section>
            <section class="callout">
                <div>
                    <p class="eyebrow">"Static export"</p>
                    <h2>"This site is a Rusty application."</h2>
                    <p>"The route table is rendered ahead of time, then deployed to GitHub Pages. Client navigation and loading boundaries keep it feeling immediate."</p>
                </div>
                <a class="button primary" href={static_export}>"How the export works"</a>
            </section>
        </main>
    })
}

fn feature(title: &str, body: &str) -> Element {
    let title = title.to_owned();
    let body = body.to_owned();
    view! {
        <article class="card">
            <h2>{title}</h2>
            <p>{body}</p>
        </article>
    }
}
