//! Server errors.

use std::error::Error;
use std::fmt;

use rusty_router::RouterError;

use crate::StatusCode;

/// An error from HTTP handling, routing, middleware, or authentication.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum ServerError {
    /// An HTTP method is not supported.
    InvalidMethod(String),
    /// A status code is outside the valid range.
    InvalidStatus(u16),
    /// A header name or value is invalid.
    InvalidHeader {
        /// The offending header name, if known.
        name: String,
        /// A description of the problem.
        message: String,
    },
    /// A route pattern or route table is invalid.
    Route(String),
    /// A handler returned an error.
    Handler(String),
    /// A middleware returned an error.
    Middleware(String),
    /// Authentication failed or was missing.
    Authentication(String),
    /// CSRF validation failed.
    Csrf(String),
    /// Secure random token generation failed.
    Random(String),
}

impl fmt::Display for ServerError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidMethod(method) => write!(formatter, "unsupported HTTP method `{method}`"),
            Self::InvalidStatus(status) => write!(formatter, "invalid HTTP status code {status}"),
            Self::InvalidHeader { name, message } => {
                if name.is_empty() {
                    write!(formatter, "invalid HTTP header: {message}")
                } else {
                    write!(formatter, "invalid HTTP header `{name}`: {message}")
                }
            }
            Self::Route(message) => write!(formatter, "route error: {message}"),
            Self::Handler(message) => write!(formatter, "request handler failed: {message}"),
            Self::Middleware(message) => write!(formatter, "middleware failed: {message}"),
            Self::Authentication(message) => write!(formatter, "authentication failed: {message}"),
            Self::Csrf(message) => write!(formatter, "CSRF validation failed: {message}"),
            Self::Random(message) => {
                write!(formatter, "secure random generation failed: {message}")
            }
        }
    }
}

impl Error for ServerError {}

impl From<RouterError> for ServerError {
    fn from(error: RouterError) -> Self {
        Self::Route(error.to_string())
    }
}

impl ServerError {
    /// Returns the HTTP status recommended for this error.
    #[must_use]
    pub fn status(&self) -> StatusCode {
        match self {
            Self::InvalidMethod(_) | Self::InvalidStatus(_) | Self::InvalidHeader { .. } => {
                StatusCode::BAD_REQUEST
            }
            Self::Authentication(_) => StatusCode::UNAUTHORIZED,
            Self::Csrf(_) => StatusCode::FORBIDDEN,
            Self::Route(_) | Self::Handler(_) | Self::Middleware(_) | Self::Random(_) => {
                StatusCode::INTERNAL_SERVER_ERROR
            }
        }
    }

    /// Converts the error into a plain-text response.
    #[must_use]
    pub fn into_response(self) -> crate::Response {
        crate::Response::text(self.status(), self.to_string())
    }
}
