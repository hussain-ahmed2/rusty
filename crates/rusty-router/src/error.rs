//! Router errors.

use std::error::Error;
use std::fmt;
use std::path::PathBuf;

/// An error from route discovery, matching, or manifest handling.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum RouterError {
    /// A filesystem operation failed.
    Io {
        /// The path involved in the failure.
        path: PathBuf,
        /// The operating-system error message.
        message: String,
    },
    /// A directory or route file is not valid under the routing conventions.
    InvalidRoute {
        /// The path involved in the failure.
        path: PathBuf,
        /// A description of the invalid convention.
        message: String,
    },
    /// A route pattern is invalid.
    InvalidPattern {
        /// The rendered pattern.
        pattern: String,
        /// A description of the invalid pattern.
        message: String,
    },
    /// Two routes resolve to the same URL shape.
    DuplicateRoute {
        /// The rendered pattern shared by both routes.
        pattern: String,
        /// The first page file.
        first: PathBuf,
        /// The second page file.
        second: PathBuf,
    },
    /// A serialized route manifest is invalid.
    Manifest(String),
}

impl fmt::Display for RouterError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io { path, message } => {
                write!(
                    formatter,
                    "router I/O failed for {}: {message}",
                    path.display()
                )
            }
            Self::InvalidRoute { path, message } => {
                write!(formatter, "invalid route at {}: {message}", path.display())
            }
            Self::InvalidPattern { pattern, message } => {
                write!(formatter, "invalid route pattern `{pattern}`: {message}")
            }
            Self::DuplicateRoute {
                pattern,
                first,
                second,
            } => write!(
                formatter,
                "duplicate route `{pattern}` from {} and {}",
                first.display(),
                second.display()
            ),
            Self::Manifest(message) => write!(formatter, "invalid route manifest: {message}"),
        }
    }
}

impl Error for RouterError {}

impl RouterError {
    pub(crate) fn io(path: impl Into<PathBuf>, error: &std::io::Error) -> Self {
        Self::Io {
            path: path.into(),
            message: error.to_string(),
        }
    }

    pub(crate) fn invalid_route(path: impl Into<PathBuf>, message: impl Into<String>) -> Self {
        Self::InvalidRoute {
            path: path.into(),
            message: message.into(),
        }
    }

    pub(crate) fn invalid_pattern(pattern: impl Into<String>, message: impl Into<String>) -> Self {
        Self::InvalidPattern {
            pattern: pattern.into(),
            message: message.into(),
        }
    }

    pub(crate) fn manifest(message: impl Into<String>) -> Self {
        Self::Manifest(message.into())
    }
}
