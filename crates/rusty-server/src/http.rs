//! HTTP methods, status codes, headers, requests, and responses.

use std::collections::BTreeMap;
use std::fmt;
use std::str::FromStr;
use std::sync::{Mutex, OnceLock};

use crate::ServerError;

/// An HTTP request method.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Method {
    /// `GET`.
    Get,
    /// `POST`.
    Post,
    /// `PUT`.
    Put,
    /// `PATCH`.
    Patch,
    /// `DELETE`.
    Delete,
    /// `HEAD`.
    Head,
    /// `OPTIONS`.
    Options,
}

impl Method {
    /// Returns the uppercase wire representation of the method.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Get => "GET",
            Self::Post => "POST",
            Self::Put => "PUT",
            Self::Patch => "PATCH",
            Self::Delete => "DELETE",
            Self::Head => "HEAD",
            Self::Options => "OPTIONS",
        }
    }

    /// Returns `true` for methods that must not change server state.
    #[must_use]
    pub const fn is_safe(self) -> bool {
        matches!(self, Self::Get | Self::Head | Self::Options)
    }
}

impl fmt::Display for Method {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl FromStr for Method {
    type Err = ServerError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "GET" => Ok(Self::Get),
            "POST" => Ok(Self::Post),
            "PUT" => Ok(Self::Put),
            "PATCH" => Ok(Self::Patch),
            "DELETE" => Ok(Self::Delete),
            "HEAD" => Ok(Self::Head),
            "OPTIONS" => Ok(Self::Options),
            _ => Err(ServerError::InvalidMethod(value.to_owned())),
        }
    }
}

/// An HTTP response status code.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct StatusCode(u16);

impl StatusCode {
    /// `200 OK`.
    pub const OK: Self = Self(200);
    /// `201 Created`.
    pub const CREATED: Self = Self(201);
    /// `204 No Content`.
    pub const NO_CONTENT: Self = Self(204);
    /// `400 Bad Request`.
    pub const BAD_REQUEST: Self = Self(400);
    /// `401 Unauthorized`.
    pub const UNAUTHORIZED: Self = Self(401);
    /// `403 Forbidden`.
    pub const FORBIDDEN: Self = Self(403);
    /// `404 Not Found`.
    pub const NOT_FOUND: Self = Self(404);
    /// `405 Method Not Allowed`.
    pub const METHOD_NOT_ALLOWED: Self = Self(405);
    /// `409 Conflict`.
    pub const CONFLICT: Self = Self(409);
    /// `413 Payload Too Large`.
    pub const PAYLOAD_TOO_LARGE: Self = Self(413);
    /// `422 Unprocessable Entity`.
    pub const UNPROCESSABLE_ENTITY: Self = Self(422);
    /// `431 Request Header Fields Too Large`.
    pub const REQUEST_HEADER_FIELDS_TOO_LARGE: Self = Self(431);
    /// `500 Internal Server Error`.
    pub const INTERNAL_SERVER_ERROR: Self = Self(500);
    /// `501 Not Implemented`.
    pub const NOT_IMPLEMENTED: Self = Self(501);
    /// `503 Service Unavailable`.
    pub const SERVICE_UNAVAILABLE: Self = Self(503);
    /// `505 HTTP Version Not Supported`.
    pub const HTTP_VERSION_NOT_SUPPORTED: Self = Self(505);

    /// Creates a status code in the `100..=599` range.
    ///
    /// # Errors
    ///
    /// Returns [`ServerError::InvalidStatus`] for values outside that range.
    pub const fn new(value: u16) -> Result<Self, ServerError> {
        if value >= 100 && value <= 599 {
            Ok(Self(value))
        } else {
            Err(ServerError::InvalidStatus(value))
        }
    }

    /// Returns the numeric status code.
    #[must_use]
    pub const fn as_u16(self) -> u16 {
        self.0
    }

    /// Returns the standard reason phrase, if one is known.
    #[must_use]
    pub const fn canonical_reason(self) -> Option<&'static str> {
        Some(match self.0 {
            200 => "OK",
            201 => "Created",
            202 => "Accepted",
            204 => "No Content",
            301 => "Moved Permanently",
            302 => "Found",
            304 => "Not Modified",
            307 => "Temporary Redirect",
            308 => "Permanent Redirect",
            400 => "Bad Request",
            401 => "Unauthorized",
            403 => "Forbidden",
            404 => "Not Found",
            405 => "Method Not Allowed",
            409 => "Conflict",
            413 => "Payload Too Large",
            415 => "Unsupported Media Type",
            422 => "Unprocessable Entity",
            429 => "Too Many Requests",
            431 => "Request Header Fields Too Large",
            500 => "Internal Server Error",
            501 => "Not Implemented",
            503 => "Service Unavailable",
            505 => "HTTP Version Not Supported",
            _ => return None,
        })
    }
}

impl fmt::Display for StatusCode {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}", self.0)
    }
}

impl TryFrom<u16> for StatusCode {
    type Error = ServerError;

    fn try_from(value: u16) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

/// Case-insensitive HTTP header fields with multi-value support.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Headers {
    values: BTreeMap<String, Vec<String>>,
}

impl Headers {
    /// Creates an empty header map.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Inserts a header, replacing existing values.
    ///
    /// Header names are normalized to ASCII lowercase.
    ///
    /// # Errors
    ///
    /// Returns [`ServerError::InvalidHeader`] when the name is not a valid
    /// HTTP token or the value contains a control character.
    pub fn insert(
        &mut self,
        name: impl AsRef<str>,
        value: impl Into<String>,
    ) -> Result<(), ServerError> {
        let name = normalize_header_name(name.as_ref())?;
        let value = validate_header_value(value.into())?;
        self.values.insert(name, vec![value]);
        Ok(())
    }

    /// Appends a value to a header.
    ///
    /// # Errors
    ///
    /// Returns [`ServerError::InvalidHeader`] when the name or value is invalid.
    pub fn append(
        &mut self,
        name: impl AsRef<str>,
        value: impl Into<String>,
    ) -> Result<(), ServerError> {
        let name = normalize_header_name(name.as_ref())?;
        let value = validate_header_value(value.into())?;
        self.values.entry(name).or_default().push(value);
        Ok(())
    }

    /// Returns the first value for a header.
    #[must_use]
    pub fn get(&self, name: &str) -> Option<&str> {
        self.values
            .get(&name.to_ascii_lowercase())
            .and_then(|values| values.first())
            .map(String::as_str)
    }

    /// Returns all values for a header in insertion order.
    #[must_use]
    pub fn get_all(&self, name: &str) -> &[String] {
        self.values
            .get(&name.to_ascii_lowercase())
            .map_or(&[], Vec::as_slice)
    }

    /// Returns `true` when the header is present.
    #[must_use]
    pub fn contains(&self, name: &str) -> bool {
        self.values.contains_key(&name.to_ascii_lowercase())
    }

    /// Removes a header and returns its values.
    pub fn remove(&mut self, name: &str) -> Option<Vec<String>> {
        self.values.remove(&name.to_ascii_lowercase())
    }

    /// Iterates over every header value in header-name order.
    pub fn iter(&self) -> impl Iterator<Item = (&str, &str)> {
        self.values.iter().flat_map(|(name, values)| {
            values
                .iter()
                .map(move |value| (name.as_str(), value.as_str()))
        })
    }

    /// Returns the number of distinct header names.
    #[must_use]
    pub fn len(&self) -> usize {
        self.values.len()
    }

    /// Returns `true` when no headers are present.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.values.is_empty()
    }
}

/// An HTTP request independent of a particular network transport.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Request {
    method: Method,
    target: String,
    headers: Headers,
    body: Vec<u8>,
}

impl Request {
    /// Creates a request with an empty body and headers.
    #[must_use]
    pub fn new(method: Method, target: impl Into<String>) -> Self {
        Self {
            method,
            target: target.into(),
            headers: Headers::new(),
            body: Vec::new(),
        }
    }

    /// Creates a request from already-parsed parts.
    ///
    /// Header values in `headers` are assumed to have been validated by
    /// [`Headers`].
    #[must_use]
    pub fn from_parts(
        method: Method,
        target: impl Into<String>,
        headers: Headers,
        body: Vec<u8>,
    ) -> Self {
        Self {
            method,
            target: target.into(),
            headers,
            body,
        }
    }

    /// Adds a header.
    ///
    /// # Errors
    ///
    /// Returns [`ServerError::InvalidHeader`] when the name or value is invalid.
    pub fn with_header(
        mut self,
        name: impl AsRef<str>,
        value: impl Into<String>,
    ) -> Result<Self, ServerError> {
        self.headers.insert(name, value)?;
        Ok(self)
    }

    /// Sets the request body.
    #[must_use]
    pub fn with_body(mut self, body: impl Into<Vec<u8>>) -> Self {
        self.body = body.into();
        self
    }

    /// Returns the request method.
    #[must_use]
    pub fn method(&self) -> Method {
        self.method
    }

    /// Returns the raw request target, including any query string.
    #[must_use]
    pub fn target(&self) -> &str {
        &self.target
    }

    /// Returns the path portion of the request target.
    #[must_use]
    pub fn path(&self) -> &str {
        self.target
            .split(['?', '#'])
            .next()
            .filter(|path| !path.is_empty())
            .unwrap_or("/")
    }

    /// Returns the query string without the leading `?`.
    #[must_use]
    pub fn query(&self) -> Option<&str> {
        self.target
            .split_once('?')
            .map(|(_, query)| query.split('#').next().unwrap_or(""))
    }

    /// Returns an undecoded query parameter by name.
    ///
    /// Percent-encoded values are returned exactly as they appear.
    #[must_use]
    pub fn query_param(&self, name: &str) -> Option<&str> {
        self.query()?.split('&').find_map(|pair| {
            let (key, value) = pair.split_once('=').unwrap_or((pair, ""));
            (key == name).then_some(value)
        })
    }

    /// Returns the request headers.
    #[must_use]
    pub fn headers(&self) -> &Headers {
        &self.headers
    }

    /// Returns the request body.
    #[must_use]
    pub fn body(&self) -> &[u8] {
        &self.body
    }

    /// Consumes the request and returns its parts.
    #[must_use]
    pub fn into_parts(self) -> (Method, String, Headers, Vec<u8>) {
        (self.method, self.target, self.headers, self.body)
    }
}

struct StreamBody {
    chunks: Mutex<Option<Box<dyn Iterator<Item = Vec<u8>> + Send>>>,
    collected: OnceLock<Vec<u8>>,
}

impl StreamBody {
    fn take_chunks(&self) -> Box<dyn Iterator<Item = Vec<u8>> + Send> {
        let mut chunks = self
            .chunks
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        chunks.take().unwrap_or_else(empty_chunks)
    }
}

fn empty_chunks() -> Box<dyn Iterator<Item = Vec<u8>> + Send> {
    Box::new(std::iter::empty())
}

enum ResponseBody {
    Full(Vec<u8>),
    Stream(StreamBody),
}

impl ResponseBody {
    fn as_bytes(&self) -> &[u8] {
        match self {
            Self::Full(body) => body,
            Self::Stream(stream) => stream.collected.get_or_init(|| {
                let mut body = Vec::new();
                for chunk in stream.take_chunks() {
                    body.extend(chunk);
                }
                body
            }),
        }
    }

    fn into_bytes(self) -> Vec<u8> {
        match self {
            Self::Full(body) => body,
            Self::Stream(stream) => {
                if let Some(body) = stream.collected.into_inner() {
                    body
                } else {
                    let chunks = stream
                        .chunks
                        .into_inner()
                        .unwrap_or_else(std::sync::PoisonError::into_inner);
                    chunks.unwrap_or_else(empty_chunks).flatten().collect()
                }
            }
        }
    }
}

pub(crate) enum WireBody {
    Full(Vec<u8>),
    Stream(Box<dyn Iterator<Item = Vec<u8>> + Send>),
}

impl ResponseBody {
    fn into_wire(self) -> WireBody {
        match self {
            Self::Full(body) => WireBody::Full(body),
            Self::Stream(stream) => {
                if let Some(body) = stream.collected.into_inner() {
                    WireBody::Full(body)
                } else {
                    let chunks = stream
                        .chunks
                        .into_inner()
                        .unwrap_or_else(std::sync::PoisonError::into_inner);
                    WireBody::Stream(chunks.unwrap_or_else(empty_chunks))
                }
            }
        }
    }
}

/// An HTTP response independent of a particular network transport.
///
/// Responses normally carry a complete body. [`Response::stream`] creates a
/// chunked response from an iterator; [`Response::body`] collects such a body
/// when a transport-neutral caller needs the full bytes.
pub struct Response {
    status: StatusCode,
    headers: Headers,
    body: ResponseBody,
}

impl fmt::Debug for Response {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Response")
            .field("status", &self.status)
            .field("headers", &self.headers)
            .field("streaming", &matches!(self.body, ResponseBody::Stream(_)))
            .finish()
    }
}

impl PartialEq for Response {
    fn eq(&self, other: &Self) -> bool {
        self.status == other.status && self.headers == other.headers && self.body() == other.body()
    }
}

impl Eq for Response {}

impl Response {
    /// Creates an empty response with the given status.
    #[must_use]
    pub fn new(status: StatusCode) -> Self {
        Self {
            status,
            headers: Headers::new(),
            body: ResponseBody::Full(Vec::new()),
        }
    }

    /// Creates a UTF-8 text response.
    #[must_use]
    pub fn text(status: StatusCode, body: impl Into<String>) -> Self {
        let mut response = Self::new(status);
        let _ = response
            .headers
            .insert("content-type", "text/plain; charset=utf-8");
        response.body = ResponseBody::Full(body.into().into_bytes());
        response
    }

    /// Creates an HTML response.
    #[must_use]
    pub fn html(status: StatusCode, body: impl Into<String>) -> Self {
        let mut response = Self::new(status);
        let _ = response
            .headers
            .insert("content-type", "text/html; charset=utf-8");
        response.body = ResponseBody::Full(body.into().into_bytes());
        response
    }

    /// Creates a JSON response.
    #[must_use]
    pub fn json(status: StatusCode, body: impl Into<String>) -> Self {
        let mut response = Self::new(status);
        let _ = response
            .headers
            .insert("content-type", "application/json; charset=utf-8");
        response.body = ResponseBody::Full(body.into().into_bytes());
        response
    }

    /// Creates a chunked response from raw byte chunks.
    ///
    /// The transport uses `transfer-encoding: chunked` and does not emit a
    /// `content-length` header. Call [`Response::body`] if a transport-neutral
    /// caller needs the collected bytes.
    #[must_use]
    pub fn stream<I>(status: StatusCode, content_type: impl Into<String>, chunks: I) -> Self
    where
        I: IntoIterator<Item = Vec<u8>>,
        I::IntoIter: Send + 'static,
    {
        let mut response = Self::new(status);
        let _ = response.headers.insert("content-type", content_type);
        response.body = ResponseBody::Stream(StreamBody {
            chunks: Mutex::new(Some(Box::new(chunks.into_iter()))),
            collected: OnceLock::new(),
        });
        response
    }

    /// Creates a chunked HTML response.
    ///
    /// Chunks are converted to bytes when the response is consumed.
    #[must_use]
    pub fn stream_html<I, C>(status: StatusCode, chunks: I) -> Self
    where
        I: IntoIterator<Item = C>,
        I::IntoIter: Send + 'static,
        C: Into<Vec<u8>> + Send + 'static,
    {
        Self::stream(
            status,
            "text/html; charset=utf-8",
            chunks.into_iter().map(Into::into),
        )
    }

    /// Adds a header.
    ///
    /// # Errors
    ///
    /// Returns [`ServerError::InvalidHeader`] when the name or value is invalid.
    pub fn with_header(
        mut self,
        name: impl AsRef<str>,
        value: impl Into<String>,
    ) -> Result<Self, ServerError> {
        self.headers.insert(name, value)?;
        Ok(self)
    }

    /// Sets the response status.
    #[must_use]
    pub fn with_status(mut self, status: StatusCode) -> Self {
        self.status = status;
        self
    }

    /// Sets the response body.
    #[must_use]
    pub fn with_body(mut self, body: impl Into<Vec<u8>>) -> Self {
        self.body = ResponseBody::Full(body.into());
        self
    }

    /// Returns the response status.
    #[must_use]
    pub fn status(&self) -> StatusCode {
        self.status
    }

    /// Returns the response headers.
    #[must_use]
    pub fn headers(&self) -> &Headers {
        &self.headers
    }

    /// Returns the response headers mutably.
    pub fn headers_mut(&mut self) -> &mut Headers {
        &mut self.headers
    }

    /// Returns the response body.
    ///
    /// Streaming bodies are collected on first access.
    #[must_use]
    pub fn body(&self) -> &[u8] {
        self.body.as_bytes()
    }

    /// Returns `true` when the response was created with [`Response::stream`].
    #[must_use]
    pub fn is_streaming(&self) -> bool {
        matches!(self.body, ResponseBody::Stream(_))
    }

    /// Consumes the response and returns its parts.
    ///
    /// Streaming bodies are collected into a single byte vector.
    #[must_use]
    pub fn into_parts(self) -> (StatusCode, Headers, Vec<u8>) {
        (self.status, self.headers, self.body.into_bytes())
    }

    pub(crate) fn into_wire_parts(self) -> (StatusCode, Headers, WireBody) {
        (self.status, self.headers, self.body.into_wire())
    }
}

fn normalize_header_name(name: &str) -> Result<String, ServerError> {
    if name.is_empty() || !name.bytes().all(is_header_token) {
        return Err(ServerError::InvalidHeader {
            name: name.to_owned(),
            message: "header names must be non-empty ASCII tokens".to_owned(),
        });
    }
    Ok(name.to_ascii_lowercase())
}

fn is_header_token(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || b"!#$%&'*+-.^_`|~".contains(&byte)
}

fn validate_header_value(value: String) -> Result<String, ServerError> {
    if value
        .bytes()
        .any(|byte| byte == b'\r' || byte == b'\n' || byte == 0)
    {
        return Err(ServerError::InvalidHeader {
            name: String::new(),
            message: "header values cannot contain CR, LF, or NUL".to_owned(),
        });
    }
    Ok(value)
}
