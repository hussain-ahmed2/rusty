//! Compile-time check for the WebAssembly server-function client stub.

#![cfg(target_arch = "wasm32")]
#![forbid(unsafe_code)]

use rusty::prelude::*;

#[server]
fn add(left: i32, right: i32) -> Result<i32, RpcError> {
    Ok(left + right)
}

#[server("/api/echo")]
fn echo(value: String) -> Result<String, RpcError> {
    Ok(value)
}

#[server(path = "/custom/zero")]
fn zero() -> Result<u8, RpcError> {
    Ok(7)
}

#[test]
fn client_stubs_have_the_original_signatures() {
    let _add: fn(i32, i32) -> Result<i32, RpcError> = add;
    let _echo: fn(String) -> Result<String, RpcError> = echo;
    let _zero: fn() -> Result<u8, RpcError> = zero;
    assert_eq!(AddServerFn::PATH, "/_rusty/rpc/add");
    assert_eq!(EchoServerFn::PATH, "/api/echo");
    assert_eq!(ZeroServerFn::PATH, "/custom/zero");
}
