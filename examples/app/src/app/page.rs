//! Home page.

use rusty::prelude::*;

/// Renders the home page.
pub fn page(_context: &mut RequestContext) -> Result<Element, ServerError> {
    Ok(view! {
        <main>
            <h1>"Hello from Rusty!"</h1>
            <p>"This page was discovered from " <code>"src/app/page.rs"</code> "."</p>
            <p><a href="/about">"About"</a></p>
        </main>
    })
}
