//! Build-time analysis for Rusty applications.
//!
//! [`AppCompiler`] combines three analyses into a single [`AppManifest`]:
//!
//! - file-system routes and API routes from [`rusty_router::RouteManifest`];
//! - `#[server]` function declarations parsed from Rust source with `syn`;
//! - static files under the configured public directory;
//! - the configured source root and manifest format version.
//!
//! ```no_run
//! use rusty_compiler::AppCompiler;
//!
//! let compiler = AppCompiler::from_project(".").expect("load project");
//! let manifest = compiler.analyze().expect("analyze application");
//! compiler.write_manifest(&manifest).expect("write manifest");
//! # let _ = manifest;
//! ```

#![forbid(unsafe_code)]
#![deny(missing_docs)]

mod assets;
mod error;
mod fingerprint;
mod manifest;
mod server;

use std::fs;
use std::path::{Path, PathBuf};

use rusty_config::RustyConfig;
use rusty_router::RouteManifest;

pub use error::CompileError;
pub use fingerprint::fingerprint;
pub use manifest::{AppManifest, Asset, ServerFunction, MANIFEST_VERSION};

/// The file name used for generated application manifests.
pub const MANIFEST_FILE: &str = "app-manifest.json";

/// Analyzes a Rusty application.
#[derive(Debug, Clone)]
pub struct AppCompiler {
    project_root: PathBuf,
    config: RustyConfig,
}

impl AppCompiler {
    /// Creates a compiler for a project root and configuration.
    ///
    /// The configuration is validated immediately. The project root must be an
    /// existing directory.
    ///
    /// # Errors
    ///
    /// Returns [`CompileError::Config`] for invalid configuration and
    /// [`CompileError::MissingDirectory`] when the project root is missing.
    pub fn new(project_root: impl AsRef<Path>, config: RustyConfig) -> Result<Self, CompileError> {
        config.validate()?;
        let project_root = project_root.as_ref();
        if !project_root.is_dir() {
            return Err(CompileError::MissingDirectory {
                path: project_root.to_path_buf(),
                kind: "project root",
            });
        }
        Ok(Self {
            project_root: project_root.to_path_buf(),
            config,
        })
    }

    /// Loads `rusty.toml` from a project root and creates a compiler.
    ///
    /// # Errors
    ///
    /// Returns [`CompileError::Config`] when configuration is invalid and
    /// [`CompileError::MissingDirectory`] when the project root is missing.
    pub fn from_project(project_root: impl AsRef<Path>) -> Result<Self, CompileError> {
        let project_root = project_root.as_ref();
        if !project_root.is_dir() {
            return Err(CompileError::MissingDirectory {
                path: project_root.to_path_buf(),
                kind: "project root",
            });
        }
        let config = RustyConfig::load(project_root)?;
        Self::new(project_root, config)
    }

    /// Returns the project root.
    #[must_use]
    pub fn project_root(&self) -> &Path {
        &self.project_root
    }

    /// Returns the loaded configuration.
    #[must_use]
    pub fn config(&self) -> &RustyConfig {
        &self.config
    }

    /// Returns the resolved application directory.
    #[must_use]
    pub fn app_dir(&self) -> PathBuf {
        self.project_root.join(&self.config.app.app_dir)
    }

    /// Returns the resolved source root.
    #[must_use]
    pub fn source_root(&self) -> PathBuf {
        self.project_root.join(&self.config.app.source_root)
    }

    /// Returns the resolved public asset directory.
    #[must_use]
    pub fn public_dir(&self) -> PathBuf {
        self.project_root.join(&self.config.app.public_dir)
    }

    /// Returns the generated manifest path under the configured output
    /// directory.
    #[must_use]
    pub fn manifest_path(&self) -> PathBuf {
        self.project_root
            .join(&self.config.build.out_dir)
            .join(MANIFEST_FILE)
    }

    /// Analyzes routes and server functions without writing anything.
    ///
    /// # Errors
    ///
    /// Returns an error when the application or source directory is missing,
    /// route discovery fails, a Rust file cannot be parsed, a `#[server]`
    /// declaration is invalid, or two server functions declare the same path.
    pub fn analyze(&self) -> Result<AppManifest, CompileError> {
        let app_dir = self.app_dir();
        if !app_dir.is_dir() {
            return Err(CompileError::MissingDirectory {
                path: app_dir,
                kind: "application",
            });
        }
        let source_root = self.source_root();
        if !source_root.is_dir() {
            return Err(CompileError::MissingDirectory {
                path: source_root,
                kind: "source",
            });
        }

        let routes = RouteManifest::discover(&app_dir)?;
        let server_functions = server::discover(&self.project_root, &source_root)?;
        let assets = assets::discover(&self.public_dir())?;
        Ok(AppManifest::new(
            self.config.app.source_root.clone(),
            routes,
            server_functions,
            assets,
        ))
    }

    /// Writes a manifest to its configured path.
    ///
    /// # Errors
    ///
    /// Returns an error when serialization fails or the file cannot be written.
    pub fn write_manifest(&self, manifest: &AppManifest) -> Result<(), CompileError> {
        manifest.write_to(self.manifest_path())
    }

    /// Analyzes the application and writes the manifest.
    ///
    /// # Errors
    ///
    /// Returns any error from [`analyze`](Self::analyze) or
    /// [`write_manifest`](Self::write_manifest).
    pub fn analyze_and_write(&self) -> Result<AppManifest, CompileError> {
        let manifest = self.analyze()?;
        self.write_manifest(&manifest)?;
        Ok(manifest)
    }
}

/// Writes a manifest, creating parent directories, and returns its path.
///
/// # Errors
///
/// Returns an error when serialization fails or the file cannot be written.
pub fn write_manifest(
    manifest: &AppManifest,
    path: impl AsRef<Path>,
) -> Result<PathBuf, CompileError> {
    let path = path.as_ref();
    manifest.write_to(path)?;
    Ok(path.to_path_buf())
}

/// Reads a manifest from disk and validates it.
///
/// # Errors
///
/// Returns [`CompileError::Io`] when the file cannot be read and
/// [`CompileError::Manifest`] when its contents are invalid.
pub fn read_manifest(path: impl AsRef<Path>) -> Result<AppManifest, CompileError> {
    let path = path.as_ref();
    let contents = fs::read_to_string(path).map_err(|error| error::io(path, &error))?;
    AppManifest::from_json(&contents)
}
