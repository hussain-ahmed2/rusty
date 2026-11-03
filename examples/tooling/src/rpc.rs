//! Typed server functions discovered by the compiler.

use rusty::rpc::RpcError;

/// Returns a greeting for the compiler example.
///
/// # Errors
///
/// This function does not currently fail.
#[allow(dead_code)]
#[rusty::server]
pub fn ping() -> Result<String, RpcError> {
    Ok("pong".to_owned())
}
