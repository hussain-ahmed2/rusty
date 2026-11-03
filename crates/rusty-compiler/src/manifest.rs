//! Serialized application manifests.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use rusty_router::RouteManifest;
use serde::{Deserialize, Serialize};

use crate::CompileError;

/// The current application manifest format version.
pub const MANIFEST_VERSION: u32 = 3;

/// A discovered `#[server]` function.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ServerFunction {
    name: String,
    path: String,
    module: String,
    source: PathBuf,
}

impl ServerFunction {
    pub(crate) fn new(name: String, path: String, module: String, source: PathBuf) -> Self {
        Self {
            name,
            path,
            module,
            source,
        }
    }

    /// Returns the Rust function name.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Returns the HTTP path used to invoke the function.
    #[must_use]
    pub fn path(&self) -> &str {
        &self.path
    }

    /// Returns the inferred Rust module path, such as `crate::rpc`.
    #[must_use]
    pub fn module(&self) -> &str {
        &self.module
    }

    /// Returns the source file, relative to the project root.
    #[must_use]
    pub fn source(&self) -> &Path {
        &self.source
    }
}

/// A file served from the public directory.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Asset {
    path: PathBuf,
    url: String,
    bytes: u64,
    fingerprint: String,
}

impl Asset {
    pub(crate) fn new(path: PathBuf, url: String, bytes: u64, fingerprint: String) -> Self {
        Self {
            path,
            url,
            bytes,
            fingerprint,
        }
    }

    /// Returns the file path relative to the public directory.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Returns the URL path the asset is served from.
    #[must_use]
    pub fn url(&self) -> &str {
        &self.url
    }

    /// Returns the asset size in bytes.
    #[must_use]
    pub fn bytes(&self) -> u64 {
        self.bytes
    }

    /// Returns the stable content fingerprint.
    #[must_use]
    pub fn fingerprint(&self) -> &str {
        &self.fingerprint
    }
}

/// A complete build-time analysis of a Rusty application.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AppManifest {
    version: u32,
    source_root: PathBuf,
    routes: RouteManifest,
    server_functions: Vec<ServerFunction>,
    assets: Vec<Asset>,
}

impl AppManifest {
    pub(crate) fn new(
        source_root: PathBuf,
        routes: RouteManifest,
        server_functions: Vec<ServerFunction>,
        assets: Vec<Asset>,
    ) -> Self {
        Self {
            version: MANIFEST_VERSION,
            source_root,
            routes,
            server_functions,
            assets,
        }
    }

    /// Returns the manifest format version.
    #[must_use]
    pub fn version(&self) -> u32 {
        self.version
    }

    /// Returns the configured source root, relative to the project root.
    #[must_use]
    pub fn source_root(&self) -> &Path {
        &self.source_root
    }

    /// Returns the discovered page and API routes.
    #[must_use]
    pub fn routes(&self) -> &RouteManifest {
        &self.routes
    }

    /// Returns the discovered server functions.
    #[must_use]
    pub fn server_functions(&self) -> &[ServerFunction] {
        &self.server_functions
    }

    /// Returns the static assets found under the public directory.
    #[must_use]
    pub fn assets(&self) -> &[Asset] {
        &self.assets
    }

    /// Serializes the manifest as compact JSON.
    ///
    /// # Errors
    ///
    /// Returns [`CompileError::Manifest`] when serialization fails.
    pub fn to_json(&self) -> Result<String, CompileError> {
        serde_json::to_string(self).map_err(|error| CompileError::Manifest(error.to_string()))
    }

    /// Serializes the manifest as pretty-printed JSON.
    ///
    /// # Errors
    ///
    /// Returns [`CompileError::Manifest`] when serialization fails.
    pub fn to_json_pretty(&self) -> Result<String, CompileError> {
        serde_json::to_string_pretty(self)
            .map_err(|error| CompileError::Manifest(error.to_string()))
    }

    /// Writes the manifest as pretty-printed JSON, creating parent directories.
    ///
    /// # Errors
    ///
    /// Returns [`CompileError::Manifest`] when serialization fails and
    /// [`CompileError::Io`] when the file cannot be written.
    pub fn write_to(&self, path: impl AsRef<Path>) -> Result<(), CompileError> {
        let path = path.as_ref();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|error| crate::error::io(parent, &error))?;
        }
        let json = self.to_json_pretty()?;
        fs::write(path, json).map_err(|error| crate::error::io(path, &error))
    }

    /// Restores and validates a serialized manifest.
    ///
    /// # Errors
    ///
    /// Returns [`CompileError::Manifest`] when decoding or validation fails.
    pub fn from_json(json: &str) -> Result<Self, CompileError> {
        let manifest: Self = serde_json::from_str(json)
            .map_err(|error| CompileError::Manifest(error.to_string()))?;
        manifest.validate()?;
        Ok(manifest)
    }

    fn validate(&self) -> Result<(), CompileError> {
        self.routes
            .validate()
            .map_err(|error| CompileError::Manifest(error.to_string()))?;
        if self.version != MANIFEST_VERSION {
            return Err(CompileError::Manifest(format!(
                "unsupported manifest version {}; expected {MANIFEST_VERSION}",
                self.version
            )));
        }
        if self.source_root.as_os_str().is_empty() {
            return Err(CompileError::Manifest(
                "the source root cannot be empty".to_owned(),
            ));
        }

        let mut asset_urls = BTreeMap::<&str, &PathBuf>::new();
        for asset in &self.assets {
            if asset.path.as_os_str().is_empty() {
                return Err(CompileError::Manifest(
                    "a public asset is missing its path".to_owned(),
                ));
            }
            if asset.path.is_absolute()
                || asset
                    .path
                    .components()
                    .any(|component| matches!(component, std::path::Component::ParentDir))
            {
                return Err(CompileError::Manifest(format!(
                    "public asset path `{}` must be relative and cannot contain `..`",
                    asset.path.display()
                )));
            }
            if asset.fingerprint.len() != 16
                || !asset
                    .fingerprint
                    .bytes()
                    .all(|byte| byte.is_ascii_hexdigit())
            {
                return Err(CompileError::Manifest(format!(
                    "public asset `{}` has an invalid fingerprint",
                    asset.path.display()
                )));
            }
            if !asset.url.starts_with('/') || asset.url.contains(['?', '#']) {
                return Err(CompileError::Manifest(format!(
                    "public asset URL `{}` must start with `/` and cannot contain a query or fragment",
                    asset.url
                )));
            }
            if let Some(first) = asset_urls.insert(&asset.url, &asset.path) {
                return Err(CompileError::Manifest(format!(
                    "duplicate public asset URL `{}` for {} and {}",
                    asset.url,
                    first.display(),
                    asset.path.display()
                )));
            }
        }

        let mut seen = BTreeMap::<&str, &PathBuf>::new();
        for function in &self.server_functions {
            if function.name.is_empty() {
                return Err(CompileError::Manifest(
                    "a server function is missing its name".to_owned(),
                ));
            }
            if function.module.is_empty() {
                return Err(CompileError::Manifest(format!(
                    "server function `{}` is missing its module path",
                    function.name
                )));
            }
            if function.source.as_os_str().is_empty() {
                return Err(CompileError::Manifest(format!(
                    "server function `{}` is missing its source path",
                    function.name
                )));
            }
            if !function.path.starts_with('/') {
                return Err(CompileError::Manifest(format!(
                    "server function `{}` path `{}` must start with `/`",
                    function.name, function.path
                )));
            }
            if function.path.contains(['?', '#']) {
                return Err(CompileError::Manifest(format!(
                    "server function `{}` path `{}` cannot contain a query string or fragment",
                    function.name, function.path
                )));
            }
            if let Some(first) = seen.insert(&function.path, &function.source) {
                return Err(CompileError::Manifest(format!(
                    "duplicate server function path `{}` in {} and {}",
                    function.path,
                    first.display(),
                    function.source.display()
                )));
            }
        }
        Ok(())
    }
}
