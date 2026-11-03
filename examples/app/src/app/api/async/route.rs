//! Async API route fixture.

use rusty::prelude::*;

/// Returns an asynchronous health response.
pub async fn get(_context: &mut RequestContext) -> Result<Response, ServerError> {
    let status = std::future::ready("ok").await;
    Ok(Response::json(
        StatusCode::OK,
        format!(r#"{{"status":"{status}"}}"#),
    ))
}
