//! Production WebAssembly client builds.

use std::fs;
use std::path::{Component, Path, PathBuf};
use std::process::Command;

use rusty_compiler::fingerprint;
use rusty_config::RustyConfig;
use serde::{Deserialize, Serialize};

use crate::CliError;

const CLIENT_MANIFEST_VERSION: u32 = 1;
const CLIENT_MANIFEST_FILE: &str = "client-manifest.json";

#[derive(Debug)]
pub(crate) struct ClientBuild {
    pub(crate) entrypoint: String,
    pub(crate) files: usize,
}

#[derive(Debug, Deserialize)]
struct Metadata {
    packages: Vec<Package>,
    target_directory: PathBuf,
}

#[derive(Debug, Deserialize)]
struct Package {
    name: String,
    manifest_path: PathBuf,
    targets: Vec<MetadataTarget>,
}

#[derive(Debug, Deserialize)]
struct MetadataTarget {
    name: String,
    crate_types: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ClientManifest {
    version: u32,
    package: String,
    entrypoint: String,
    files: Vec<ClientFile>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ClientFile {
    path: PathBuf,
    bytes: u64,
    fingerprint: String,
}

pub(crate) fn build(
    project: &Path,
    config: &RustyConfig,
    release: bool,
) -> Result<Option<ClientBuild>, CliError> {
    let Some(client_dir) = &config.build.client_dir else {
        return Ok(None);
    };
    let client_dir = normalize(&project.join(client_dir));
    let out_dir = normalize(&project.join(&config.build.out_dir).join("client"));
    let client_manifest_path = normalize(
        &project
            .join(&config.build.out_dir)
            .join(CLIENT_MANIFEST_FILE),
    );
    if outputs_overlap(&client_dir, &out_dir) {
        return Err(CliError::Client(format!(
            "client output directory {} overlaps the client package at {}",
            out_dir.display(),
            client_dir.display()
        )));
    }
    let manifest_path = client_dir.join("Cargo.toml");
    if !manifest_path.is_file() {
        return Err(CliError::Client(format!(
            "client package is missing {}",
            manifest_path.display()
        )));
    }

    let metadata = metadata(&manifest_path)?;
    let package = metadata.package_for(&manifest_path)?;
    let target = package.cdylib_target()?;
    build_package(&manifest_path, release)?;

    let profile = if release { "release" } else { "debug" };
    let wasm = metadata
        .target_directory
        .join("wasm32-unknown-unknown")
        .join(profile)
        .join(format!("{}.wasm", target.name));
    if !wasm.is_file() {
        return Err(CliError::Client(format!(
            "Cargo did not produce the expected WebAssembly module at {}",
            wasm.display()
        )));
    }

    if client_manifest_path.exists() {
        fs::remove_file(&client_manifest_path)
            .map_err(|error| crate::io_error(&client_manifest_path, &error))?;
    }
    reset_directory(&out_dir)?;
    run_wasm_bindgen(&wasm, &out_dir, &target.name)?;

    let entrypoint = format!("{}.js", target.name);
    if !out_dir.join(&entrypoint).is_file() {
        return Err(CliError::Client(format!(
            "wasm-bindgen did not produce the expected entrypoint at {}",
            out_dir.join(&entrypoint).display()
        )));
    }
    let files = collect_files(&out_dir)?;
    let manifest = ClientManifest {
        version: CLIENT_MANIFEST_VERSION,
        package: package.name.clone(),
        entrypoint: entrypoint.clone(),
        files,
    };
    write_manifest(&manifest, &client_manifest_path)?;
    Ok(Some(ClientBuild {
        entrypoint,
        files: manifest.files.len(),
    }))
}

impl Metadata {
    fn package_for(&self, manifest_path: &Path) -> Result<&Package, CliError> {
        let requested =
            fs::canonicalize(manifest_path).unwrap_or_else(|_| manifest_path.to_path_buf());
        self.packages
            .iter()
            .find(|package| {
                fs::canonicalize(&package.manifest_path)
                    .unwrap_or_else(|_| package.manifest_path.clone())
                    == requested
            })
            .ok_or_else(|| {
                CliError::Client(format!(
                    "Cargo metadata did not include the client package at {}",
                    manifest_path.display()
                ))
            })
    }
}

impl Package {
    fn cdylib_target(&self) -> Result<&MetadataTarget, CliError> {
        self.targets
            .iter()
            .find(|target| target.crate_types.iter().any(|kind| kind == "cdylib"))
            .ok_or_else(|| {
                CliError::Client(format!(
                    "client package `{}` does not declare a `cdylib` target",
                    self.name
                ))
            })
    }
}

fn normalize(path: &Path) -> PathBuf {
    path.components()
        .filter(|component| !matches!(component, Component::CurDir))
        .collect()
}

fn outputs_overlap(client_dir: &Path, out_dir: &Path) -> bool {
    client_dir == out_dir || client_dir.starts_with(out_dir) || out_dir.starts_with(client_dir)
}

fn metadata(manifest_path: &Path) -> Result<Metadata, CliError> {
    let output = Command::new("cargo")
        .arg("metadata")
        .arg("--no-deps")
        .arg("--format-version")
        .arg("1")
        .arg("--manifest-path")
        .arg(manifest_path)
        .output()
        .map_err(|error| CliError::Client(format!("cannot run `cargo metadata`: {error}")))?;
    if !output.status.success() {
        return Err(CliError::Client(format!(
            "`cargo metadata` failed for {}",
            manifest_path.display()
        )));
    }
    serde_json::from_slice(&output.stdout)
        .map_err(|error| CliError::Client(format!("invalid `cargo metadata` output: {error}")))
}

fn build_package(manifest_path: &Path, release: bool) -> Result<(), CliError> {
    let mut command = Command::new("cargo");
    command
        .arg("build")
        .arg("--quiet")
        .arg("--manifest-path")
        .arg(manifest_path)
        .arg("--target")
        .arg("wasm32-unknown-unknown");
    if release {
        command.arg("--release");
    }
    let status = command.status().map_err(|error| {
        CliError::Client(format!("cannot run `cargo build` for the client: {error}"))
    })?;
    if !status.success() {
        return Err(CliError::CommandFailed {
            command: "cargo build --target wasm32-unknown-unknown".to_owned(),
            status: status.code(),
        });
    }
    Ok(())
}

fn run_wasm_bindgen(wasm: &Path, out_dir: &Path, name: &str) -> Result<(), CliError> {
    let tool = std::env::var_os("RUSTY_WASM_BINDGEN").unwrap_or_else(|| "wasm-bindgen".into());
    let status = Command::new(&tool)
        .arg("--target")
        .arg("web")
        .arg("--out-dir")
        .arg(out_dir)
        .arg("--out-name")
        .arg(name)
        .arg(wasm)
        .status()
        .map_err(|error| {
            CliError::Client(format!(
                "cannot run wasm-bindgen; install it with `cargo install wasm-bindgen-cli`: {error}"
            ))
        })?;
    if !status.success() {
        return Err(CliError::CommandFailed {
            command: "wasm-bindgen".to_owned(),
            status: status.code(),
        });
    }
    Ok(())
}

fn reset_directory(path: &Path) -> Result<(), CliError> {
    if path.exists() {
        fs::remove_dir_all(path).map_err(|error| crate::io_error(path, &error))?;
    }
    fs::create_dir_all(path).map_err(|error| crate::io_error(path, &error))
}

fn collect_files(root: &Path) -> Result<Vec<ClientFile>, CliError> {
    let mut files = Vec::new();
    visit(root, root, &mut files)?;
    files.sort_by(|left, right| left.path.cmp(&right.path));
    Ok(files)
}

fn visit(root: &Path, directory: &Path, files: &mut Vec<ClientFile>) -> Result<(), CliError> {
    let entries = fs::read_dir(directory).map_err(|error| crate::io_error(directory, &error))?;
    for entry in entries {
        let entry = entry.map_err(|error| crate::io_error(directory, &error))?;
        let path = entry.path();
        let file_type = entry
            .file_type()
            .map_err(|error| crate::io_error(&path, &error))?;
        if file_type.is_dir() {
            visit(root, &path, files)?;
        } else if file_type.is_file() {
            let relative = path
                .strip_prefix(root)
                .map_err(|_| {
                    CliError::Client("client artifact escaped its output directory".to_owned())
                })?
                .to_path_buf();
            let bytes = entry
                .metadata()
                .map_err(|error| crate::io_error(&path, &error))?
                .len();
            let contents = fs::read(&path).map_err(|error| crate::io_error(&path, &error))?;
            files.push(ClientFile {
                path: relative,
                bytes,
                fingerprint: fingerprint(&contents),
            });
        }
    }
    Ok(())
}

fn write_manifest(manifest: &ClientManifest, path: &Path) -> Result<(), CliError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| crate::io_error(parent, &error))?;
    }
    let json = serde_json::to_string_pretty(manifest)
        .map_err(|error| CliError::Client(error.to_string()))?;
    fs::write(path, json).map_err(|error| crate::io_error(path, &error))
}

#[cfg(test)]
mod tests {
    use super::{
        collect_files, outputs_overlap, ClientFile, ClientManifest, CLIENT_MANIFEST_VERSION,
    };
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicU64, Ordering};

    struct TempDir {
        path: PathBuf,
    }

    impl TempDir {
        fn new(label: &str) -> Self {
            static NEXT: AtomicU64 = AtomicU64::new(0);
            let unique = NEXT.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!(
                "rusty-wasm-{}-{label}-{unique}",
                std::process::id()
            ));
            let _ = fs::remove_dir_all(&path);
            fs::create_dir_all(&path).expect("create temp dir");
            Self { path }
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }

    #[test]
    fn parses_cargo_metadata_and_selects_cdylib_targets() {
        let json = br#"{
            "packages": [
                {
                    "name": "demo",
                    "manifest_path": "/tmp/demo/Cargo.toml",
                    "targets": [
                        {"name": "demo", "crate_types": ["cdylib", "rlib"]},
                        {"name": "demo-bin", "crate_types": ["bin"]}
                    ]
                }
            ],
            "target_directory": "/tmp/target"
        }"#;
        let metadata: super::Metadata = serde_json::from_slice(json).expect("metadata");
        let package = metadata
            .package_for(Path::new("/tmp/demo/Cargo.toml"))
            .expect("package");
        assert_eq!(package.name, "demo");
        assert_eq!(package.cdylib_target().expect("cdylib").name, "demo");
        assert_eq!(metadata.target_directory, Path::new("/tmp/target"));

        let bin_only: super::Metadata = serde_json::from_slice(
            br#"{
                "packages": [
                    {
                        "name": "bin-only",
                        "manifest_path": "/tmp/bin-only/Cargo.toml",
                        "targets": [{"name": "bin-only", "crate_types": ["bin"]}]
                    }
                ],
                "target_directory": "/tmp/target"
            }"#,
        )
        .expect("metadata");
        let package = bin_only
            .package_for(Path::new("/tmp/bin-only/Cargo.toml"))
            .expect("package");
        assert!(package.cdylib_target().is_err());
    }

    #[test]
    fn rejects_client_packages_inside_the_output_directory() {
        let project = Path::new("/tmp/project");
        assert!(outputs_overlap(
            &project.join("target/rusty/client"),
            &project.join("target/rusty/client")
        ));
        assert!(outputs_overlap(
            &project.join("target/rusty/client/src"),
            &project.join("target/rusty/client")
        ));
        assert!(outputs_overlap(
            &project.join("client"),
            &project.join("client/target/rusty/client")
        ));
        assert!(!outputs_overlap(
            &project.join("client"),
            &project.join("target/rusty/client")
        ));
    }

    #[test]
    fn collects_sorted_artifacts_with_fingerprints() {
        let directory = TempDir::new("artifacts");
        fs::create_dir_all(directory.path.join("nested")).expect("nested");
        fs::write(directory.path.join("b.js"), "b").expect("b");
        fs::write(directory.path.join("nested/a.wasm"), "a").expect("a");

        let files = collect_files(&directory.path).expect("collect");
        assert_eq!(files.len(), 2);
        assert_eq!(files[0].path, Path::new("b.js"));
        assert_eq!(files[0].bytes, 1);
        assert_eq!(files[0].fingerprint.len(), 16);
        assert_eq!(files[1].path, Path::new("nested/a.wasm"));
    }

    #[test]
    fn client_manifests_round_trip_through_json() {
        let manifest = ClientManifest {
            version: CLIENT_MANIFEST_VERSION,
            package: "demo".to_owned(),
            entrypoint: "demo.js".to_owned(),
            files: vec![ClientFile {
                path: Path::new("demo.js").to_path_buf(),
                bytes: 12,
                fingerprint: "0123456789abcdef".to_owned(),
            }],
        };
        let json = serde_json::to_string_pretty(&manifest).expect("serialize");
        let restored: ClientManifest = serde_json::from_str(&json).expect("restore");
        assert_eq!(restored.package, "demo");
        assert_eq!(restored.files[0].path, Path::new("demo.js"));
    }
}
