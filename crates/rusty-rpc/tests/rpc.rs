//! Server-function registry and client transport tests.

#![forbid(unsafe_code)]

use std::sync::Arc;

use rusty_rpc::{
    call, invoke, invoke_with, set_transport, with_transport, RpcClient, RpcError, RpcRegistry,
    ServerFunction, ServerInvoker, Transport,
};
use rusty_server::{Method, Request, StatusCode};

#[derive(Default)]
struct Add;

impl ServerFunction for Add {
    const PATH: &'static str = "/_rusty/rpc/add";
    type Args = (i32, i32);
    type Output = i32;
}

impl ServerInvoker for Add {
    fn invoke(&self, args: &[u8]) -> Result<Vec<u8>, RpcError> {
        let (left, right): (i32, i32) = serde_json::from_slice(args)
            .map_err(|error| RpcError::Deserialize(error.to_string()))?;
        let output = add(left, right)?;
        serde_json::to_vec(&output).map_err(|error| RpcError::Serialize(error.to_string()))
    }
}

fn add(left: i32, right: i32) -> Result<i32, RpcError> {
    Ok(left + right)
}

struct RegistryTransport(Arc<RpcRegistry>);

impl Transport for RegistryTransport {
    fn call(&self, path: &str, body: &[u8]) -> Result<Vec<u8>, RpcError> {
        self.0.invoke(path, body)
    }
}

#[test]
fn registry_invokes_and_handles_server_functions() {
    let mut registry = RpcRegistry::new();
    registry.register::<Add>().expect("register");
    assert_eq!(registry.len(), 1);
    assert_eq!(registry.paths().collect::<Vec<_>>(), [Add::PATH]);

    let output = registry.invoke(Add::PATH, br#"[2,3]"#).expect("invoke");
    assert_eq!(output, b"5");

    let request = Request::new(Method::Post, Add::PATH).with_body(br#"[4,5]"#.to_vec());
    let response = registry.handle(&request).expect("handle");
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response.headers().get("content-type"),
        Some("application/json; charset=utf-8")
    );
    assert_eq!(response.body(), b"9");

    let wrong_method = registry
        .handle(&Request::new(Method::Get, Add::PATH))
        .expect("handle");
    assert_eq!(wrong_method.status(), StatusCode::METHOD_NOT_ALLOWED);

    let unknown = registry
        .handle(&Request::new(Method::Post, "/missing").with_body(b"[]".to_vec()))
        .expect("handle");
    assert_eq!(unknown.status(), StatusCode::NOT_FOUND);

    assert!(matches!(
        registry.register::<Add>(),
        Err(RpcError::Server(_))
    ));
}

#[test]
fn typed_clients_invoke_through_transports() {
    let mut registry = RpcRegistry::new();
    registry.register::<Add>().expect("register");
    let transport = Arc::new(RegistryTransport(Arc::new(registry)));

    let client = RpcClient::new(transport.clone());
    assert_eq!(client.invoke::<Add>(&(10, 5)).expect("client"), 15);
    assert_eq!(
        invoke_with::<Add>(transport.as_ref(), &(1, 2)).expect("invoke_with"),
        3
    );

    let _guard = set_transport(transport.clone());
    assert_eq!(invoke::<Add>(&(7, 8)).expect("thread-local invoke"), 15);
    assert_eq!(call(Add::PATH, br#"[20,22]"#).expect("call"), b"42");

    let result = with_transport(transport, || invoke::<Add>(&(3, 4)));
    assert_eq!(result.expect("with_transport"), 7);
}

#[test]
fn missing_transport_reports_a_transport_error() {
    let error = invoke::<Add>(&(1, 2)).expect_err("no transport");
    assert!(matches!(error, RpcError::Transport(_)));
    assert_eq!(error.status_code(), 500);
}
