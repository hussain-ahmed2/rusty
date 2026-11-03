//! Typed configuration for Rusty applications.
//!
//! Rusty applications may place a `rusty.toml` file at the project root. Every
//! field is optional; missing values use the defaults shown on each config
//! type. Unknown fields are rejected so typos fail loudly.
//!
//! ```no_run
//! use rusty_config::RustyConfig;
//!
//! let config = RustyConfig::load(".").expect("load configuration");
//! assert_eq!(config.app.source_root, std::path::Path::new("src"));
//! ```
//!
//! ```toml
//! [app]
//! source_root = "src"
//! app_dir = "src/app"
//! public_dir = "public"
//!
//! [build]
//! out_dir = "target/rusty"
//!
//! [dev]
//! host = "127.0.0.1"
//! port = 3000
//! ```

#![forbid(unsafe_code)]
#![deny(missing_docs)]

use std::error::Error;
use std::fmt;
use std::fs;
use std::path::{Component, Path, PathBuf};

use serde::{Deserialize, Serialize};

/// The default configuration file name.
pub const CONFIG_FILE: &str = "rusty.toml";

/// An error from configuration loading, parsing, or validation.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum ConfigError {
    /// A configuration file or directory could not be read.
    Io {
        /// The path involved in the failure.
        path: PathBuf,
        /// The operating-system error message.
        message: String,
    },
    /// TOML parsing or serialization failed.
    Parse {
        /// The configuration file path.
        path: PathBuf,
        /// The parser error message.
        message: String,
    },
    /// A configuration value is invalid.
    Invalid {
        /// The field or path involved, when known.
        field: String,
        /// A description of the problem.
        message: String,
    },
}

impl fmt::Display for ConfigError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io { path, message } => {
                write!(
                    formatter,
                    "configuration I/O failed for {}: {message}",
                    path.display()
                )
            }
            Self::Parse { path, message } => {
                write!(
                    formatter,
                    "invalid configuration at {}: {message}",
                    path.display()
                )
            }
            Self::Invalid { field, message } => {
                write!(
                    formatter,
                    "invalid configuration field `{field}`: {message}"
                )
            }
        }
    }
}

impl Error for ConfigError {}

/// Application source layout.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct AppConfig {
    /// Directory containing the application's Rust source, relative to the
    /// project root. Defaults to `src`.
    pub source_root: PathBuf,
    /// Directory containing `page.rs`, `layout.rs`, and `route.rs` files,
    /// relative to the project root. Defaults to `src/app`.
    pub app_dir: PathBuf,
    /// Directory containing static assets served at the site root, relative to
    /// the project root. Defaults to `public`.
    pub public_dir: PathBuf,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            source_root: PathBuf::from("src"),
            app_dir: PathBuf::from("src/app"),
            public_dir: PathBuf::from("public"),
        }
    }
}

/// Build output configuration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct BuildConfig {
    /// Directory for generated manifests and build artifacts, relative to the
    /// project root. Defaults to `target/rusty`.
    pub out_dir: PathBuf,
}

impl Default for BuildConfig {
    fn default() -> Self {
        Self {
            out_dir: PathBuf::from("target/rusty"),
        }
    }
}

/// Development server configuration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct DevConfig {
    /// Interface the development server binds to. Defaults to `127.0.0.1`.
    pub host: String,
    /// Port the development server binds to. Defaults to `3000`. A value of
    /// `0` asks the operating system for an ephemeral port.
    pub port: u16,
}

impl Default for DevConfig {
    fn default() -> Self {
        Self {
            host: "127.0.0.1".to_owned(),
            port: 3000,
        }
    }
}

/// A complete Rusty application configuration.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct RustyConfig {
    /// Application source layout.
    pub app: AppConfig,
    /// Build output settings.
    pub build: BuildConfig,
    /// Development server settings.
    pub dev: DevConfig,
}

impl RustyConfig {
    /// Returns the configuration file path under a project root.
    #[must_use]
    pub fn path(project_root: impl AsRef<Path>) -> PathBuf {
        project_root.as_ref().join(CONFIG_FILE)
    }

    /// Loads configuration from a project root.
    ///
    /// A missing `rusty.toml` produces the default configuration rather than an
    /// error, which keeps convention-driven applications zero-config.
    ///
    /// # Errors
    ///
    /// Returns [`ConfigError::Io`] when the file exists but cannot be read,
    /// [`ConfigError::Parse`] when it is not valid TOML, and
    /// [`ConfigError::Invalid`] when a value fails validation.
    pub fn load(project_root: impl AsRef<Path>) -> Result<Self, ConfigError> {
        let path = Self::path(project_root);
        match fs::metadata(&path) {
            Ok(_) => Self::load_from(path),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(error) => Err(ConfigError::Io {
                path,
                message: error.to_string(),
            }),
        }
    }

    /// Loads configuration from a specific file.
    ///
    /// # Errors
    ///
    /// Returns [`ConfigError::Io`] when the file cannot be read,
    /// [`ConfigError::Parse`] when it is not valid TOML, and
    /// [`ConfigError::Invalid`] when a value fails validation.
    pub fn load_from(path: impl AsRef<Path>) -> Result<Self, ConfigError> {
        let path = path.as_ref();
        let contents = fs::read_to_string(path).map_err(|error| ConfigError::Io {
            path: path.to_path_buf(),
            message: error.to_string(),
        })?;
        let config = Self::parse(&contents).map_err(|error| match error {
            ConfigError::Parse { message, .. } => ConfigError::Parse {
                path: path.to_path_buf(),
                message,
            },
            other => other,
        })?;
        config.validate()?;
        Ok(config)
    }

    /// Parses configuration from TOML text.
    ///
    /// # Errors
    ///
    /// Returns [`ConfigError::Parse`] when the text is not valid TOML and
    /// [`ConfigError::Invalid`] when a value fails validation.
    pub fn parse(contents: &str) -> Result<Self, ConfigError> {
        let config: Self = toml::from_str(contents).map_err(|error| ConfigError::Parse {
            path: PathBuf::from(CONFIG_FILE),
            message: error.to_string(),
        })?;
        config.validate()?;
        Ok(config)
    }

    /// Serializes the configuration as pretty TOML.
    ///
    /// # Errors
    ///
    /// Returns [`ConfigError::Parse`] when serialization fails.
    pub fn to_toml(&self) -> Result<String, ConfigError> {
        toml::to_string_pretty(self).map_err(|error| ConfigError::Parse {
            path: PathBuf::from(CONFIG_FILE),
            message: error.to_string(),
        })
    }

    /// Validates structural configuration values.
    ///
    /// File-system existence is checked by the compiler or CLI, not here.
    ///
    /// # Errors
    ///
    /// Returns [`ConfigError::Invalid`] when a path is absolute or escapes the
    /// project root or when a value is empty.
    pub fn validate(&self) -> Result<(), ConfigError> {
        validate_relative("app.source_root", &self.app.source_root)?;
        validate_relative("app.app_dir", &self.app.app_dir)?;
        validate_relative("app.public_dir", &self.app.public_dir)?;
        validate_relative("build.out_dir", &self.build.out_dir)?;
        if self.dev.host.trim().is_empty() {
            return Err(ConfigError::Invalid {
                field: "dev.host".to_owned(),
                message: "the host cannot be empty".to_owned(),
            });
        }
        if self.dev.host.chars().any(char::is_whitespace) {
            return Err(ConfigError::Invalid {
                field: "dev.host".to_owned(),
                message: "the host cannot contain whitespace".to_owned(),
            });
        }
        Ok(())
    }
}

fn validate_relative(field: &str, path: &Path) -> Result<(), ConfigError> {
    if path.as_os_str().is_empty() {
        return Err(ConfigError::Invalid {
            field: field.to_owned(),
            message: "the path cannot be empty".to_owned(),
        });
    }
    if path.is_absolute() {
        return Err(ConfigError::Invalid {
            field: field.to_owned(),
            message: "the path must be relative to the project root".to_owned(),
        });
    }
    if path
        .components()
        .any(|component| matches!(component, Component::ParentDir))
    {
        return Err(ConfigError::Invalid {
            field: field.to_owned(),
            message: "the path cannot contain `..`".to_owned(),
        });
    }
    Ok(())
}
