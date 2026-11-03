//! Compiler errors.

use std::error::Error;
use std::fmt;
use std::path::PathBuf;

use rusty_config::ConfigError;
use rusty_router::RouterError;

/// An error from application analysis or manifest handling.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum CompileError {
    /// A required file or directory could not be read.
    Io {
        /// The path involved in the failure.
        path: PathBuf,
        /// The operating-system error message.
        message: String,
    },
    /// A required directory is missing or is not a directory.
    MissingDirectory {
        /// The missing directory.
        path: PathBuf,
        /// Which configured directory was expected.
        kind: &'static str,
    },
    /// Configuration loading or validation failed.
    Config(ConfigError),
    /// Route discovery failed.
    Router(RouterError),
    /// A Rust source file could not be parsed.
    InvalidSource {
        /// The source file.
        path: PathBuf,
        /// The parser error message.
        message: String,
    },
    /// A source file contains an invalid `#[server]` declaration.
    InvalidServerFunction {
        /// The source file.
        path: PathBuf,
        /// A description of the problem.
        message: String,
    },
    /// Two server functions declare the same path.
    DuplicateServerPath {
        /// The duplicated path.
        path: String,
        /// The first source file.
        first: PathBuf,
        /// The second source file.
        second: PathBuf,
    },
    /// A serialized application manifest is invalid.
    Manifest(String),
}

impl fmt::Display for CompileError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io { path, message } => {
                write!(
                    formatter,
                    "compiler I/O failed for {}: {message}",
                    path.display()
                )
            }
            Self::MissingDirectory { path, kind } => {
                write!(formatter, "missing {kind} directory: {}", path.display())
            }
            Self::Config(error) => error.fmt(formatter),
            Self::Router(error) => error.fmt(formatter),
            Self::InvalidSource { path, message } => {
                write!(
                    formatter,
                    "invalid Rust source at {}: {message}",
                    path.display()
                )
            }
            Self::InvalidServerFunction { path, message } => {
                write!(
                    formatter,
                    "invalid server function at {}: {message}",
                    path.display()
                )
            }
            Self::DuplicateServerPath {
                path,
                first,
                second,
            } => write!(
                formatter,
                "duplicate server function path `{path}` in {} and {}",
                first.display(),
                second.display()
            ),
            Self::Manifest(message) => write!(formatter, "invalid application manifest: {message}"),
        }
    }
}

impl Error for CompileError {}

impl From<ConfigError> for CompileError {
    fn from(error: ConfigError) -> Self {
        Self::Config(error)
    }
}

impl From<RouterError> for CompileError {
    fn from(error: RouterError) -> Self {
        Self::Router(error)
    }
}

pub(crate) fn io(path: &std::path::Path, error: &std::io::Error) -> CompileError {
    CompileError::Io {
        path: path.to_path_buf(),
        message: error.to_string(),
    }
}
