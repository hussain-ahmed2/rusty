//! Async page fixture.

use rusty::prelude::*;

/// Renders a page from an asynchronous handler.
pub async fn page(_context: &mut RequestContext) -> Result<Element, ServerError> {
    let message = std::future::ready("Async page").await;
    Ok(view! {
        <main>
            <h1>{message}</h1>
            <p><a href="/">"Back home"</a></p>
        </main>
    })
}
