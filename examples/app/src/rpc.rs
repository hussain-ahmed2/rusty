//! Typed server function example.

use rusty::prelude::*;

/// Echoes a value back to the caller.
///
/// # Errors
///
/// This function does not fail.
#[rusty::server("/api/echo")]
pub fn echo(value: String) -> Result<String, RpcError> {
    Ok(value)
}
