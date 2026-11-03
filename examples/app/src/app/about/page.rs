//! About page.

use rusty::prelude::*;

/// Renders the about page.
pub fn page(_context: &mut RequestContext) -> Result<Element, ServerError> {
    Ok(view! {
        <main>
            <h1>"About"</h1>
            <p><a href="/">"Home"</a></p>
        </main>
    })
}
