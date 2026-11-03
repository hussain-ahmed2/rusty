//! Blocking HTTP/1.1 over TCP.
//!
//! The transport parses origin-form requests, supports `Content-Length` request
//! bodies, and always closes the connection after one response. Chunked
//! request bodies and HTTP/2 are rejected rather than partially supported.
//!
//! ```
//! use rusty_server::{HttpRouter, HttpServer, Response, StatusCode};
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let router = HttpRouter::new()
//!     .get("/health", |_| Ok(Response::text(StatusCode::OK, "ok")))?;
//! let server = HttpServer::bind("127.0.0.1:0")?;
//! let address = server.local_addr()?;
//! let shutdown = server.shutdown_handle();
//! let thread = std::thread::spawn(move || server.run_router(router));
//!
//! // Requests can now be sent to `address`.
//! shutdown.shutdown();
//! thread.join().expect("server thread")?;
//! # Ok(())
//! # }
//! ```

use std::io::{self, BufRead, BufReader, Write};
use std::net::{SocketAddr, TcpListener, TcpStream, ToSocketAddrs};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use crate::http::{Headers, Method, Request, Response, StatusCode, WireBody};
use crate::{HttpRouter, ServerError};

const DEFAULT_MAX_HEADER_BYTES: usize = 64 * 1024;
const DEFAULT_MAX_BODY_BYTES: usize = 8 * 1024 * 1024;
const DEFAULT_TIMEOUT: Duration = Duration::from_secs(30);
const ACCEPT_POLL_INTERVAL: Duration = Duration::from_millis(5);

/// Limits and timeouts for the HTTP/1.1 transport.
#[derive(Debug, Clone)]
pub struct ServerConfig {
    max_header_bytes: usize,
    max_body_bytes: usize,
    read_timeout: Option<Duration>,
    write_timeout: Option<Duration>,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            max_header_bytes: DEFAULT_MAX_HEADER_BYTES,
            max_body_bytes: DEFAULT_MAX_BODY_BYTES,
            read_timeout: Some(DEFAULT_TIMEOUT),
            write_timeout: Some(DEFAULT_TIMEOUT),
        }
    }
}

impl ServerConfig {
    /// Creates the default configuration.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets the maximum combined request-line and header size in bytes.
    #[must_use]
    pub fn with_max_header_bytes(mut self, bytes: usize) -> Self {
        self.max_header_bytes = bytes;
        self
    }

    /// Sets the maximum accepted request body size in bytes.
    #[must_use]
    pub fn with_max_body_bytes(mut self, bytes: usize) -> Self {
        self.max_body_bytes = bytes;
        self
    }

    /// Sets the read timeout applied to each connection.
    #[must_use]
    pub fn with_read_timeout(mut self, timeout: Option<Duration>) -> Self {
        self.read_timeout = timeout;
        self
    }

    /// Sets the write timeout applied to each connection.
    #[must_use]
    pub fn with_write_timeout(mut self, timeout: Option<Duration>) -> Self {
        self.write_timeout = timeout;
        self
    }

    /// Returns the maximum combined request-line and header size.
    #[must_use]
    pub fn max_header_bytes(&self) -> usize {
        self.max_header_bytes
    }

    /// Returns the maximum request body size.
    #[must_use]
    pub fn max_body_bytes(&self) -> usize {
        self.max_body_bytes
    }

    /// Returns the read timeout.
    #[must_use]
    pub fn read_timeout(&self) -> Option<Duration> {
        self.read_timeout
    }

    /// Returns the write timeout.
    #[must_use]
    pub fn write_timeout(&self) -> Option<Duration> {
        self.write_timeout
    }
}

/// A handle that stops an [`HttpServer`] accept loop.
#[derive(Debug, Clone)]
pub struct ShutdownHandle {
    shutdown: Arc<AtomicBool>,
}

impl ShutdownHandle {
    /// Requests that the server stop accepting new connections.
    pub fn shutdown(&self) {
        self.shutdown.store(true, Ordering::Release);
    }

    /// Returns `true` when shutdown has been requested.
    #[must_use]
    pub fn is_shutdown(&self) -> bool {
        self.shutdown.load(Ordering::Acquire)
    }
}

/// A blocking HTTP/1.1 server over TCP.
///
/// The server accepts connections in a loop and serves each connection on a
/// scoped thread. It closes each connection after one response.
#[derive(Debug)]
pub struct HttpServer {
    listener: TcpListener,
    config: ServerConfig,
    shutdown: Arc<AtomicBool>,
}

impl HttpServer {
    /// Binds a server with the default configuration.
    ///
    /// # Errors
    ///
    /// Returns an I/O error when the address cannot be bound.
    pub fn bind(address: impl ToSocketAddrs) -> io::Result<Self> {
        Self::bind_with(address, ServerConfig::default())
    }

    /// Binds a server with an explicit configuration.
    ///
    /// # Errors
    ///
    /// Returns an I/O error when the address cannot be bound.
    pub fn bind_with(address: impl ToSocketAddrs, config: ServerConfig) -> io::Result<Self> {
        Ok(Self {
            listener: TcpListener::bind(address)?,
            config,
            shutdown: Arc::new(AtomicBool::new(false)),
        })
    }

    /// Wraps an already-bound listener.
    #[must_use]
    pub fn from_listener(listener: TcpListener, config: ServerConfig) -> Self {
        Self {
            listener,
            config,
            shutdown: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Returns the bound address.
    ///
    /// # Errors
    ///
    /// Returns an I/O error when the local address cannot be queried.
    pub fn local_addr(&self) -> io::Result<SocketAddr> {
        self.listener.local_addr()
    }

    /// Returns the transport configuration.
    #[must_use]
    pub fn config(&self) -> &ServerConfig {
        &self.config
    }

    /// Returns a handle that stops the accept loop.
    #[must_use]
    pub fn shutdown_handle(&self) -> ShutdownHandle {
        ShutdownHandle {
            shutdown: Arc::clone(&self.shutdown),
        }
    }

    /// Serves requests with a handler until shutdown is requested.
    ///
    /// Each connection runs on a scoped thread. Handler errors become 500
    /// responses without exposing their messages.
    ///
    /// # Errors
    ///
    /// Returns an I/O error when the listener cannot be configured or accepting
    /// a connection fails for a reason other than a temporary non-blocking
    /// miss.
    pub fn run<F>(self, handler: F) -> io::Result<()>
    where
        F: Fn(Request) -> Result<Response, ServerError> + Send + Sync,
    {
        self.listener.set_nonblocking(true)?;
        thread::scope(|scope| {
            loop {
                if self.shutdown.load(Ordering::Acquire) {
                    break;
                }
                match self.listener.accept() {
                    Ok((stream, _)) => {
                        if self.shutdown.load(Ordering::Acquire) {
                            break;
                        }
                        let _ = stream.set_nonblocking(false);
                        let config = &self.config;
                        let handler = &handler;
                        scope.spawn(move || {
                            let _ = serve_connection(stream, config, handler);
                        });
                    }
                    Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                        thread::sleep(ACCEPT_POLL_INTERVAL);
                    }
                    Err(error) => return Err(error),
                }
            }
            Ok(())
        })
    }

    /// Serves requests from an [`HttpRouter`] until shutdown is requested.
    ///
    /// # Errors
    ///
    /// Returns an I/O error from the accept loop.
    pub fn run_router(self, router: HttpRouter) -> io::Result<()> {
        self.run(move |request| router.handle(request))
    }
}

/// Serves one HTTP/1.1 connection.
///
/// This is useful for custom accept loops and tests. The connection is closed
/// after the response is written.
///
/// # Errors
///
/// Returns an I/O error when reading or writing the connection fails.
pub fn serve_connection<F>(stream: TcpStream, config: &ServerConfig, handler: &F) -> io::Result<()>
where
    F: Fn(Request) -> Result<Response, ServerError>,
{
    stream.set_read_timeout(config.read_timeout())?;
    stream.set_write_timeout(config.write_timeout())?;

    let parsed = {
        let mut reader = BufReader::new(&stream);
        read_request(&mut reader, config)
    };

    match parsed {
        Ok(Some(request)) => {
            let head = request.method() == Method::Head;
            match handler(request) {
                Ok(response) => write_response(&stream, response, head),
                Err(error) => {
                    let status = error.status();
                    let message = if status.as_u16() >= 500 {
                        "internal server error".to_owned()
                    } else {
                        error.to_string()
                    };
                    write_response(&stream, Response::text(status, message), head)
                }
            }
        }
        Ok(None) => Ok(()),
        Err(RequestError::Http(status, message)) => {
            write_response(&stream, Response::text(status, message), false)
        }
        Err(RequestError::Io(error)) => Err(error),
    }
}

enum RequestError {
    Io(io::Error),
    Http(StatusCode, &'static str),
}

fn read_request<R: BufRead>(
    reader: &mut R,
    config: &ServerConfig,
) -> Result<Option<Request>, RequestError> {
    let Some(request_line) = read_line(reader, config.max_header_bytes())? else {
        return Ok(None);
    };
    if request_line.is_empty() {
        return Err(bad_request("empty request line"));
    }

    let mut parts = request_line.split_whitespace();
    let method = parts.next().ok_or_else(|| bad_request("missing method"))?;
    let target = parts.next().ok_or_else(|| bad_request("missing target"))?;
    let version = parts
        .next()
        .ok_or_else(|| bad_request("missing HTTP version"))?;
    if parts.next().is_some() {
        return Err(bad_request("invalid request line"));
    }
    if !matches!(version, "HTTP/1.1" | "HTTP/1.0") {
        return Err(RequestError::Http(
            StatusCode::HTTP_VERSION_NOT_SUPPORTED,
            "only HTTP/1.0 and HTTP/1.1 are supported",
        ));
    }
    let method = method
        .parse::<Method>()
        .map_err(|_| bad_request("unsupported method"))?;
    validate_target(target)?;

    let mut headers = Headers::new();
    let mut header_bytes = request_line.len().saturating_add(2);
    let mut content_length = None;
    let mut transfer_encoding = false;

    loop {
        let Some(line) = read_line(
            reader,
            config.max_header_bytes().saturating_sub(header_bytes),
        )?
        else {
            return Err(bad_request("incomplete headers"));
        };
        if line.is_empty() {
            break;
        }
        header_bytes = header_bytes.saturating_add(line.len()).saturating_add(2);
        if header_bytes > config.max_header_bytes() {
            return Err(RequestError::Http(
                StatusCode::REQUEST_HEADER_FIELDS_TOO_LARGE,
                "request headers are too large",
            ));
        }

        let (name, value) = line
            .split_once(':')
            .ok_or_else(|| bad_request("malformed header"))?;
        if name.is_empty() || name.bytes().any(|byte| byte.is_ascii_whitespace()) {
            return Err(bad_request("malformed header name"));
        }
        let value = value.trim_matches(|character| character == ' ' || character == '\t');

        if name.eq_ignore_ascii_case("content-length") {
            if content_length.is_some() {
                return Err(bad_request("duplicate content-length"));
            }
            let length = value
                .parse::<usize>()
                .map_err(|_| bad_request("invalid content-length"))?;
            if length > config.max_body_bytes() {
                return Err(RequestError::Http(
                    StatusCode::PAYLOAD_TOO_LARGE,
                    "request body is too large",
                ));
            }
            content_length = Some(length);
        } else if name.eq_ignore_ascii_case("transfer-encoding") {
            transfer_encoding = true;
        }

        headers
            .append(name, value)
            .map_err(|_| bad_request("invalid header"))?;
    }

    if transfer_encoding {
        return Err(RequestError::Http(
            StatusCode::NOT_IMPLEMENTED,
            "transfer-encoding is not supported",
        ));
    }

    let body = if let Some(length) = content_length {
        let mut body = vec![0; length];
        reader.read_exact(&mut body).map_err(|error| {
            if error.kind() == io::ErrorKind::UnexpectedEof {
                bad_request("request body is shorter than content-length")
            } else {
                RequestError::Io(error)
            }
        })?;
        body
    } else {
        Vec::new()
    };

    Ok(Some(Request::from_parts(
        method,
        target.to_owned(),
        headers,
        body,
    )))
}

fn validate_target(target: &str) -> Result<(), RequestError> {
    if !target.starts_with('/') {
        return Err(bad_request("request targets must start with `/`"));
    }
    if target.contains('#') {
        return Err(bad_request("request targets cannot contain a fragment"));
    }
    if target.bytes().any(|byte| !(0x21..=0x7e).contains(&byte)) {
        return Err(bad_request("request targets must be visible ASCII"));
    }
    Ok(())
}

fn read_line<R: BufRead>(reader: &mut R, limit: usize) -> Result<Option<String>, RequestError> {
    let mut line = Vec::new();
    loop {
        let available = reader.fill_buf().map_err(RequestError::Io)?;
        if available.is_empty() {
            if line.is_empty() {
                return Ok(None);
            }
            return Err(bad_request("unexpected end of request"));
        }
        let newline = available.iter().position(|byte| *byte == b'\n');
        let take = newline.map_or(available.len(), |position| position + 1);
        if line.len().saturating_add(take) > limit {
            return Err(RequestError::Http(
                StatusCode::REQUEST_HEADER_FIELDS_TOO_LARGE,
                "request headers are too large",
            ));
        }
        line.extend_from_slice(&available[..take]);
        reader.consume(take);
        if newline.is_some() {
            break;
        }
    }

    if line.last() == Some(&b'\n') {
        line.pop();
    }
    if line.last() == Some(&b'\r') {
        line.pop();
    }
    if line.iter().any(|byte| matches!(byte, b'\r' | b'\n')) {
        return Err(bad_request("invalid line ending"));
    }
    String::from_utf8(line)
        .map(Some)
        .map_err(|_| bad_request("headers must be valid UTF-8"))
}

fn write_response(mut stream: &TcpStream, response: Response, head: bool) -> io::Result<()> {
    let (status, mut headers, body) = response.into_wire_parts();
    let allows_body = !matches!(status.as_u16(), 100..=199 | 204 | 304);

    headers.remove("content-length");
    headers.remove("connection");
    headers.remove("transfer-encoding");

    let reason = status.canonical_reason().unwrap_or("Unknown");
    let mut message = format!("HTTP/1.1 {status} {reason}\r\n");
    for (name, value) in headers.iter() {
        message.push_str(name);
        message.push_str(": ");
        message.push_str(value);
        message.push_str("\r\n");
    }

    match body {
        WireBody::Full(body) => {
            let body = if allows_body { body.as_slice() } else { &[] };
            message.push_str(&format!(
                "content-length: {}\r\nconnection: close\r\n\r\n",
                body.len()
            ));
            stream.write_all(message.as_bytes())?;
            if !head && allows_body {
                stream.write_all(body)?;
            }
        }
        WireBody::Stream(chunks) if allows_body => {
            message.push_str("transfer-encoding: chunked\r\nconnection: close\r\n\r\n");
            stream.write_all(message.as_bytes())?;
            if !head {
                for chunk in chunks {
                    if chunk.is_empty() {
                        continue;
                    }
                    write!(stream, "{:X}\r\n", chunk.len())?;
                    stream.write_all(&chunk)?;
                    stream.write_all(b"\r\n")?;
                }
                stream.write_all(b"0\r\n\r\n")?;
            }
        }
        WireBody::Stream(_) => {
            message.push_str("content-length: 0\r\nconnection: close\r\n\r\n");
            stream.write_all(message.as_bytes())?;
        }
    }

    stream.flush()
}

fn bad_request(message: &'static str) -> RequestError {
    RequestError::Http(StatusCode::BAD_REQUEST, message)
}
