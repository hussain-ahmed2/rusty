//! Application not-found page.

use rusty::prelude::*;

/// Renders the static 404 page.
pub fn not_found() -> Element {
    let home = crate::content::url("");
    let docs = crate::content::url("docs/");
    view! {
        <main class="page narrow">
            <p class="eyebrow">"404"</p>
            <h1>"That page has not been compiled."</h1>
            <p class="lead">"The route does not exist, or the static export has not generated it yet."</p>
            <div class="actions">
                <a class="button primary" href={home}>"Back home"</a>
                <a class="button" href={docs}>"Read the docs"</a>
            </div>
        </main>
    }
}
