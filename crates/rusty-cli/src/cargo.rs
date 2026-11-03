//! Shared Cargo metadata queries.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub(crate) struct Metadata {
    pub(crate) packages: Vec<Package>,
    pub(crate) target_directory: PathBuf,
}

#[derive(Debug, Deserialize)]
pub(crate) struct Package {
    pub(crate) name: String,
    pub(crate) manifest_path: PathBuf,
    pub(crate) targets: Vec<Target>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct Target {
    pub(crate) name: String,
    pub(crate) kind: Vec<String>,
    pub(crate) crate_types: Vec<String>,
}

pub(crate) fn metadata(manifest_path: &Path) -> Result<Metadata, String> {
    let output = Command::new("cargo")
        .arg("metadata")
        .arg("--no-deps")
        .arg("--format-version")
        .arg("1")
        .arg("--manifest-path")
        .arg(manifest_path)
        .output()
        .map_err(|error| format!("cannot run `cargo metadata`: {error}"))?;
    if !output.status.success() {
        let detail = String::from_utf8_lossy(&output.stderr);
        let detail = detail.trim();
        if detail.is_empty() {
            return Err(format!(
                "`cargo metadata` failed for {}",
                manifest_path.display()
            ));
        }
        return Err(format!(
            "`cargo metadata` failed for {}: {detail}",
            manifest_path.display()
        ));
    }
    serde_json::from_slice(&output.stdout)
        .map_err(|error| format!("invalid `cargo metadata` output: {error}"))
}

impl Metadata {
    pub(crate) fn package_for(&self, manifest_path: &Path) -> Option<&Package> {
        let requested =
            fs::canonicalize(manifest_path).unwrap_or_else(|_| manifest_path.to_path_buf());
        self.packages.iter().find(|package| {
            fs::canonicalize(&package.manifest_path)
                .unwrap_or_else(|_| package.manifest_path.clone())
                == requested
        })
    }
}

impl Package {
    pub(crate) fn cdylib_target(&self) -> Option<&Target> {
        self.targets
            .iter()
            .find(|target| target.crate_types.iter().any(|kind| kind == "cdylib"))
    }

    pub(crate) fn binary_target(&self) -> Option<&Target> {
        let mut binaries = self
            .targets
            .iter()
            .filter(|target| target.kind.iter().any(|kind| kind == "bin"));
        let first = binaries.next()?;
        if first.name == self.name || binaries.next().is_none() {
            Some(first)
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Metadata;
    use std::path::Path;

    fn parse(json: &str) -> Metadata {
        serde_json::from_str(json).expect("metadata")
    }

    #[test]
    fn selects_cdylib_and_default_binary_targets() {
        let metadata = parse(
            r#"{
                "packages": [
                    {
                        "name": "demo",
                        "manifest_path": "/tmp/demo/Cargo.toml",
                        "targets": [
                            {"name": "demo", "kind": ["bin"], "crate_types": ["bin"]},
                            {"name": "demo", "kind": ["lib"], "crate_types": ["cdylib", "rlib"]}
                        ]
                    }
                ],
                "target_directory": "/tmp/target"
            }"#,
        );
        let package = metadata
            .package_for(Path::new("/tmp/demo/Cargo.toml"))
            .expect("package");
        assert_eq!(package.name, "demo");
        assert_eq!(package.cdylib_target().expect("cdylib").name, "demo");
        assert_eq!(package.binary_target().expect("binary").name, "demo");

        let bin_only = parse(
            r#"{
                "packages": [
                    {
                        "name": "bin-only",
                        "manifest_path": "/tmp/bin-only/Cargo.toml",
                        "targets": [{"name": "bin-only", "kind": ["bin"], "crate_types": ["bin"]}]
                    }
                ],
                "target_directory": "/tmp/target"
            }"#,
        );
        let package = bin_only
            .package_for(Path::new("/tmp/bin-only/Cargo.toml"))
            .expect("package");
        assert!(package.cdylib_target().is_none());
    }

    #[test]
    fn chooses_a_single_binary_but_rejects_ambiguous_packages() {
        let metadata = parse(
            r#"{
                "packages": [
                    {
                        "name": "demo",
                        "manifest_path": "/tmp/demo/Cargo.toml",
                        "targets": [
                            {"name": "server", "kind": ["bin"], "crate_types": ["bin"]},
                            {"name": "worker", "kind": ["bin"], "crate_types": ["bin"]}
                        ]
                    },
                    {
                        "name": "one",
                        "manifest_path": "/tmp/one/Cargo.toml",
                        "targets": [
                            {"name": "renamed", "kind": ["bin"], "crate_types": ["bin"]}
                        ]
                    }
                ],
                "target_directory": "/tmp/target"
            }"#,
        );
        let package = metadata
            .package_for(Path::new("/tmp/demo/Cargo.toml"))
            .expect("package");
        assert!(package.binary_target().is_none());
        let package = metadata
            .package_for(Path::new("/tmp/one/Cargo.toml"))
            .expect("package");
        assert_eq!(package.binary_target().expect("binary").name, "renamed");
    }
}
