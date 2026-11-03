//! Validated manifests for generated WebAssembly client artifacts.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Component, Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::CompileError;

/// The current client manifest format version.
pub const CLIENT_MANIFEST_VERSION: u32 = 1;

/// The file name used for generated client manifests.
pub const CLIENT_MANIFEST_FILE: &str = "client-manifest.json";

/// A generated WebAssembly client artifact.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ClientFile {
    path: PathBuf,
    bytes: u64,
    fingerprint: String,
}

impl ClientFile {
    /// Creates a client artifact entry.
    #[must_use]
    pub fn new(path: impl Into<PathBuf>, bytes: u64, fingerprint: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            bytes,
            fingerprint: fingerprint.into(),
        }
    }

    /// Returns the artifact path relative to the client output directory.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Returns the artifact size in bytes.
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

/// A complete description of generated WebAssembly client artifacts.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ClientManifest {
    version: u32,
    package: String,
    entrypoint: String,
    files: Vec<ClientFile>,
}

impl ClientManifest {
    /// Creates and validates a client manifest.
    ///
    /// # Errors
    ///
    /// Returns [`CompileError::ClientManifest`] when the artifact list is empty,
    /// a path is unsafe or duplicated, the entrypoint is missing from the file
    /// list, or a fingerprint is malformed.
    pub fn new(
        package: impl Into<String>,
        entrypoint: impl Into<String>,
        files: Vec<ClientFile>,
    ) -> Result<Self, CompileError> {
        let manifest = Self {
            version: CLIENT_MANIFEST_VERSION,
            package: package.into(),
            entrypoint: entrypoint.into(),
            files,
        };
        manifest.validate()?;
        Ok(manifest)
    }

    /// Returns the manifest format version.
    #[must_use]
    pub fn version(&self) -> u32 {
        self.version
    }

    /// Returns the Cargo package that produced the client.
    #[must_use]
    pub fn package(&self) -> &str {
        &self.package
    }

    /// Returns the JavaScript entrypoint relative to the client directory.
    #[must_use]
    pub fn entrypoint(&self) -> &str {
        &self.entrypoint
    }

    /// Returns the client artifacts.
    #[must_use]
    pub fn files(&self) -> &[ClientFile] {
        &self.files
    }

    /// Serializes the manifest as compact JSON.
    ///
    /// # Errors
    ///
    /// Returns [`CompileError::ClientManifest`] when serialization fails.
    pub fn to_json(&self) -> Result<String, CompileError> {
        serde_json::to_string(self).map_err(|error| CompileError::ClientManifest(error.to_string()))
    }

    /// Serializes the manifest as pretty-printed JSON.
    ///
    /// # Errors
    ///
    /// Returns [`CompileError::ClientManifest`] when serialization fails.
    pub fn to_json_pretty(&self) -> Result<String, CompileError> {
        serde_json::to_string_pretty(self)
            .map_err(|error| CompileError::ClientManifest(error.to_string()))
    }

    /// Writes the manifest as pretty-printed JSON, creating parent directories.
    ///
    /// # Errors
    ///
    /// Returns [`CompileError::ClientManifest`] when serialization fails and
    /// [`CompileError::Io`] when the file cannot be written.
    pub fn write_to(&self, path: impl AsRef<Path>) -> Result<(), CompileError> {
        let path = path.as_ref();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|error| crate::error::io(parent, &error))?;
        }
        let json = self.to_json_pretty()?;
        fs::write(path, json).map_err(|error| crate::error::io(path, &error))
    }

    /// Restores and validates a serialized client manifest.
    ///
    /// # Errors
    ///
    /// Returns [`CompileError::ClientManifest`] when decoding or validation
    /// fails.
    pub fn from_json(json: &str) -> Result<Self, CompileError> {
        let manifest: Self = serde_json::from_str(json)
            .map_err(|error| CompileError::ClientManifest(error.to_string()))?;
        manifest.validate()?;
        Ok(manifest)
    }

    fn validate(&self) -> Result<(), CompileError> {
        if self.version != CLIENT_MANIFEST_VERSION {
            return Err(CompileError::ClientManifest(format!(
                "unsupported client manifest version {}; expected {CLIENT_MANIFEST_VERSION}",
                self.version
            )));
        }
        if self.package.trim().is_empty() {
            return Err(CompileError::ClientManifest(
                "the client package name cannot be empty".to_owned(),
            ));
        }
        validate_path(Path::new(&self.entrypoint), "entrypoint")?;
        if self.files.is_empty() {
            return Err(CompileError::ClientManifest(
                "the client manifest has no artifacts".to_owned(),
            ));
        }

        let mut paths = BTreeSet::new();
        for file in &self.files {
            validate_path(&file.path, "artifact")?;
            if !paths.insert(file.path.as_path()) {
                return Err(CompileError::ClientManifest(format!(
                    "duplicate client artifact `{}`",
                    file.path.display()
                )));
            }
            if file.fingerprint.len() != 16
                || !file
                    .fingerprint
                    .bytes()
                    .all(|byte| byte.is_ascii_hexdigit())
            {
                return Err(CompileError::ClientManifest(format!(
                    "client artifact `{}` has an invalid fingerprint",
                    file.path.display()
                )));
            }
        }
        if !paths.contains(Path::new(&self.entrypoint)) {
            return Err(CompileError::ClientManifest(format!(
                "client entrypoint `{}` is not listed in the artifact manifest",
                self.entrypoint
            )));
        }
        Ok(())
    }
}

fn validate_path(path: &Path, kind: &str) -> Result<(), CompileError> {
    if path.as_os_str().is_empty()
        || path.is_absolute()
        || path
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err(CompileError::ClientManifest(format!(
            "client {kind} path `{}` must be a non-empty relative path without `.` or `..` components",
            path.display()
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{ClientFile, ClientManifest, CLIENT_MANIFEST_VERSION};
    use crate::CompileError;
    use std::path::Path;

    fn file(path: &str) -> ClientFile {
        ClientFile::new(path, 12, "0123456789abcdef")
    }

    #[test]
    fn round_trips_and_validates_client_manifests() {
        let manifest = ClientManifest::new(
            "demo",
            "demo.js",
            vec![file("demo.js"), file("demo_bg.wasm")],
        )
        .expect("manifest");
        assert_eq!(manifest.version(), CLIENT_MANIFEST_VERSION);
        let restored =
            ClientManifest::from_json(&manifest.to_json_pretty().expect("json")).expect("restore");
        assert_eq!(restored.package(), "demo");
        assert_eq!(restored.entrypoint(), "demo.js");
        assert_eq!(restored.files().len(), 2);
        assert_eq!(restored.files()[1].path(), Path::new("demo_bg.wasm"));
    }

    #[test]
    fn rejects_invalid_or_inconsistent_client_manifests() {
        assert!(matches!(
            ClientManifest::new("demo", "demo.js", Vec::new()),
            Err(CompileError::ClientManifest(_))
        ));
        assert!(matches!(
            ClientManifest::new("demo", "../demo.js", vec![file("../demo.js")]),
            Err(CompileError::ClientManifest(_))
        ));
        assert!(matches!(
            ClientManifest::new("demo", "demo.js", vec![file("other.js")]),
            Err(CompileError::ClientManifest(_))
        ));
        assert!(matches!(
            ClientManifest::new("demo", "demo.js", vec![file("demo.js"), file("demo.js")]),
            Err(CompileError::ClientManifest(_))
        ));
        assert!(matches!(
            ClientManifest::new(
                "demo",
                "demo.js",
                vec![ClientFile::new("demo.js", 1, "not-a-fingerprint")]
            ),
            Err(CompileError::ClientManifest(_))
        ));
        let json = r#"{"version":99,"package":"demo","entrypoint":"demo.js","files":[{"path":"demo.js","bytes":1,"fingerprint":"0123456789abcdef"}]}"#;
        assert!(matches!(
            ClientManifest::from_json(json),
            Err(CompileError::ClientManifest(_))
        ));
    }
}
