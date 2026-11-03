//! Health check endpoint.

use rusty::prelude::*;

/// Returns the current health status.
pub fn get(_context: &mut RequestContext) -> Result<Response, ServerError> {
    Ok(Response::json(
        StatusCode::OK,
        r#"{"status":"ok"}"#.to_owned(),
    ))
}
