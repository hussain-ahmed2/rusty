//! `#[server]` macro integration tests.

#![cfg(not(target_arch = "wasm32"))]
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
fn server_macro_generates_markers_registry_paths_and_invokers() {
    assert_eq!(AddServerFn::PATH, "/_rusty/rpc/add");
    assert_eq!(EchoServerFn::PATH, "/api/echo");
    assert_eq!(ZeroServerFn::PATH, "/custom/zero");
    assert_eq!(add(2, 3).expect("native call"), 5);

    let mut registry = RpcRegistry::new();
    registry.register::<AddServerFn>().expect("register add");
    registry.register::<EchoServerFn>().expect("register echo");
    registry.register::<ZeroServerFn>().expect("register zero");
    assert_eq!(registry.len(), 3);
    assert_eq!(
        registry.paths().collect::<Vec<_>>(),
        ["/_rusty/rpc/add", "/api/echo", "/custom/zero"]
    );

    assert_eq!(
        registry
            .invoke("/_rusty/rpc/add", br#"[2,3]"#)
            .expect("invoke add"),
        b"5"
    );
    assert_eq!(
        registry
            .invoke("/api/echo", br#"["hi"]"#)
            .expect("invoke echo"),
        br#""hi""#
    );
    assert_eq!(
        registry
            .invoke("/custom/zero", b"null")
            .expect("invoke zero"),
        b"7"
    );

    assert!(matches!(
        registry.register::<AddServerFn>(),
        Err(RpcError::Server(_))
    ));

    assert_eq!(
        AddServerFn.invoke(br#"[20,22]"#).expect("direct invoker"),
        b"42"
    );
}

#[test]
fn server_functions_reject_bad_arguments() {
    let registry = {
        let mut registry = RpcRegistry::new();
        registry.register::<AddServerFn>().expect("register");
        registry
    };
    assert!(matches!(
        registry.invoke("/_rusty/rpc/add", b"not json"),
        Err(RpcError::Deserialize(_))
    ));
}
