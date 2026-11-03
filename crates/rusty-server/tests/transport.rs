//! HTTP/1.1 transport tests over real TCP sockets.

#![forbid(unsafe_code)]

use std::io::{Read, Write};
use std::net::{Shutdown, SocketAddr, TcpStream};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use rusty_server::{
    HttpRouter, HttpServer, Method, Response, ServerConfig, ServerError, ShutdownHandle, StatusCode,
};

struct TestServer {
    address: SocketAddr,
    shutdown: ShutdownHandle,
    thread: Option<JoinHandle<std::io::Result<()>>>,
}

impl TestServer {
    fn new(config: ServerConfig, router: HttpRouter) -> Self {
        let server = HttpServer::bind_with("127.0.0.1:0", config).expect("bind server");
        let address = server.local_addr().expect("local address");
        let shutdown = server.shutdown_handle();
        let thread = thread::spawn(move || server.run_router(router));
        Self {
            address,
            shutdown,
            thread: Some(thread),
        }
    }

    fn address(&self) -> SocketAddr {
        self.address
    }
}

impl Drop for TestServer {
    fn drop(&mut self) {
        self.shutdown.shutdown();
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

fn exchange(address: SocketAddr, request: &str) -> String {
    let mut stream = TcpStream::connect(address).expect("connect");
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .expect("read timeout");
    stream.write_all(request.as_bytes()).expect("write request");
    stream.shutdown(Shutdown::Write).expect("shutdown write");

    let mut response = Vec::new();
    stream.read_to_end(&mut response).expect("read response");
    String::from_utf8(response).expect("utf8 response")
}

fn health_router() -> HttpRouter {
    HttpRouter::new()
        .get("/hello", |_| Ok(Response::text(StatusCode::OK, "hello")))
        .expect("route")
}

#[test]
fn serves_a_get_request_and_closes_the_connection() {
    let server = TestServer::new(ServerConfig::default(), health_router());
    let response = exchange(
        server.address(),
        "GET /hello HTTP/1.1\r\nHost: localhost\r\n\r\n",
    );

    assert!(response.starts_with("HTTP/1.1 200 OK\r\n"));
    assert!(response.contains("content-type: text/plain; charset=utf-8\r\n"));
    assert!(response.contains("content-length: 5\r\n"));
    assert!(response.contains("connection: close\r\n"));
    assert!(response.ends_with("\r\n\r\nhello"));
}

#[test]
fn writes_streaming_responses_with_chunked_encoding() {
    let router = HttpRouter::new()
        .get("/stream", |_| {
            Ok(Response::stream(
                StatusCode::OK,
                "text/plain; charset=utf-8",
                vec![b"hello ".to_vec(), b"world".to_vec()],
            ))
        })
        .expect("route");
    let server = TestServer::new(ServerConfig::default(), router);

    let response = exchange(
        server.address(),
        "GET /stream HTTP/1.1\r\nHost: localhost\r\n\r\n",
    );

    assert!(response.starts_with("HTTP/1.1 200 OK\r\n"));
    assert!(response.contains("transfer-encoding: chunked\r\n"));
    assert!(!response.contains("content-length:"));
    assert!(response.ends_with("\r\n\r\n6\r\nhello \r\n5\r\nworld\r\n0\r\n\r\n"));
}

#[test]
fn reads_content_length_request_bodies() {
    let router = HttpRouter::new()
        .post("/echo", |context| {
            let body = String::from_utf8(context.request().body().to_vec())
                .map_err(|error| ServerError::Handler(error.to_string()))?;
            Ok(Response::text(StatusCode::OK, body))
        })
        .expect("route");
    let server = TestServer::new(ServerConfig::default(), router);

    let response = exchange(
        server.address(),
        "POST /echo HTTP/1.1\r\nHost: localhost\r\nContent-Length: 5\r\n\r\nhello",
    );
    assert!(response.starts_with("HTTP/1.1 200 OK\r\n"));
    assert!(response.ends_with("\r\n\r\nhello"));
}

#[test]
fn returns_not_found_and_method_not_allowed() {
    let server = TestServer::new(ServerConfig::default(), health_router());

    let missing = exchange(server.address(), "GET /missing HTTP/1.1\r\nHost: x\r\n\r\n");
    assert!(missing.starts_with("HTTP/1.1 404 Not Found\r\n"));

    let wrong_method = exchange(
        server.address(),
        "POST /hello HTTP/1.1\r\nHost: x\r\nContent-Length: 0\r\n\r\n",
    );
    assert!(wrong_method.starts_with("HTTP/1.1 405 Method Not Allowed\r\n"));
}

#[test]
fn rejects_malformed_requests_and_oversized_bodies() {
    let server = TestServer::new(
        ServerConfig::default().with_max_body_bytes(4),
        health_router(),
    );

    let malformed = exchange(server.address(), "GET /hello HTTP/1.1\r\nBroken\r\n\r\n");
    assert!(malformed.starts_with("HTTP/1.1 400 Bad Request\r\n"));

    let too_large = exchange(
        server.address(),
        "POST /hello HTTP/1.1\r\nHost: x\r\nContent-Length: 5\r\n\r\nhello",
    );
    assert!(too_large.starts_with("HTTP/1.1 413 Payload Too Large\r\n"));

    let chunked = exchange(
        server.address(),
        "POST /hello HTTP/1.1\r\nHost: x\r\nTransfer-Encoding: chunked\r\n\r\n",
    );
    assert!(chunked.starts_with("HTTP/1.1 501 Not Implemented\r\n"));

    let version = exchange(server.address(), "GET /hello HTTP/2.0\r\nHost: x\r\n\r\n");
    assert!(version.starts_with("HTTP/1.1 505 HTTP Version Not Supported\r\n"));
}

#[test]
fn bounds_header_size() {
    let server = TestServer::new(
        ServerConfig::default().with_max_header_bytes(32),
        health_router(),
    );
    let response = exchange(
        server.address(),
        "GET /hello HTTP/1.1\r\nHost: localhost\r\nX-Long: value\r\n\r\n",
    );

    assert!(response.starts_with("HTTP/1.1 431 Request Header Fields Too Large\r\n"));
}

#[test]
fn handler_errors_do_not_leak_details() {
    let router = HttpRouter::new()
        .get("/boom", |_| {
            Err::<Response, _>(ServerError::Handler("secret detail".to_owned()))
        })
        .expect("route");
    let server = TestServer::new(ServerConfig::default(), router);

    let response = exchange(server.address(), "GET /boom HTTP/1.1\r\nHost: x\r\n\r\n");
    assert!(response.starts_with("HTTP/1.1 500 Internal Server Error\r\n"));
    assert!(response.ends_with("\r\n\r\ninternal server error"));
    assert!(!response.contains("secret detail"));
}

#[test]
fn head_responses_keep_content_length_without_a_body() {
    let router = HttpRouter::new()
        .method(Method::Head, "/hello", |_| {
            Ok(Response::text(StatusCode::OK, "hello"))
        })
        .expect("route");
    let server = TestServer::new(ServerConfig::default(), router);

    let response = exchange(server.address(), "HEAD /hello HTTP/1.1\r\nHost: x\r\n\r\n");
    assert!(response.starts_with("HTTP/1.1 200 OK\r\n"));
    assert!(response.contains("content-length: 5\r\n"));
    assert!(response.ends_with("\r\n\r\n"));
}

#[test]
fn rejects_fragments_and_incomplete_bodies() {
    let server = TestServer::new(ServerConfig::default(), health_router());

    let fragment = exchange(
        server.address(),
        "GET /hello#frag HTTP/1.1\r\nHost: x\r\n\r\n",
    );
    assert!(fragment.starts_with("HTTP/1.1 400 Bad Request\r\n"));

    let short = exchange(
        server.address(),
        "POST /hello HTTP/1.1\r\nHost: x\r\nContent-Length: 10\r\n\r\nshort",
    );
    assert!(short.starts_with("HTTP/1.1 400 Bad Request\r\n"));
}
