//! Standalone deployment bundle creation.

use std::fs;
use std::path::{Path, PathBuf};

use rusty_config::RustyConfig;
use serde::Serialize;

use crate::{cargo, CliError};

const BUNDLE_VERSION: u32 = 1;
const DEPLOYMENT_MANIFEST: &str = "deployment.json";
const APP_MANIFEST: &str = "app-manifest.json";
const CLIENT_MANIFEST: &str = "client-manifest.json";

#[derive(Debug)]
pub(crate) struct Bundle {
    pub(crate) path: PathBuf,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct DeploymentManifest {
    version: u32,
    executable: String,
    app_manifest: String,
    client_manifest: Option<String>,
    public: bool,
    client: bool,
}

pub(crate) fn build(
    project: &Path,
    config: &RustyConfig,
    release: bool,
) -> Result<Bundle, CliError> {
    let manifest_path = project.join("Cargo.toml");
    let metadata = cargo::metadata(&manifest_path).map_err(CliError::Bundle)?;
    let package = metadata.package_for(&manifest_path).ok_or_else(|| {
        CliError::Bundle(format!(
            "Cargo metadata did not include the application package at {}",
            manifest_path.display()
        ))
    })?;
    let target = package.binary_target().ok_or_else(|| {
        CliError::Bundle(format!(
            "application package `{}` does not have a default binary target",
            package.name
        ))
    })?;
    let profile = if release { "release" } else { "debug" };
    let mut executable_dir = metadata.target_directory.clone();
    if let Some(target) = std::env::var_os("CARGO_BUILD_TARGET") {
        executable_dir.push(target);
    }
    let executable = executable_dir.join(profile).join(format!(
        "{}{}",
        target.name,
        std::env::consts::EXE_SUFFIX
    ));
    if !executable.is_file() {
        return Err(CliError::Bundle(format!(
            "Cargo did not produce the expected executable at {}",
            executable.display()
        )));
    }
    create(
        project,
        config,
        &project.join(&config.build.out_dir),
        &executable,
        &target.name,
    )
}

fn create(
    project: &Path,
    config: &RustyConfig,
    out_dir: &Path,
    executable: &Path,
    name: &str,
) -> Result<Bundle, CliError> {
    fs::create_dir_all(out_dir).map_err(|error| crate::io_error(out_dir, &error))?;
    let dist = out_dir.join("dist");
    let temporary = out_dir.join(format!(".dist-{}", std::process::id()));
    if temporary.exists() {
        fs::remove_dir_all(&temporary).map_err(|error| crate::io_error(&temporary, &error))?;
    }
    fs::create_dir_all(&temporary).map_err(|error| crate::io_error(&temporary, &error))?;

    let result = populate(
        project, config, out_dir, executable, name, &temporary, &dist,
    );
    if result.is_err() {
        let _ = fs::remove_dir_all(&temporary);
    }
    result?;
    Ok(Bundle { path: dist })
}

fn populate(
    project: &Path,
    config: &RustyConfig,
    out_dir: &Path,
    executable: &Path,
    name: &str,
    temporary: &Path,
    dist: &Path,
) -> Result<(), CliError> {
    let executable_name = format!("{name}{}", std::env::consts::EXE_SUFFIX);
    fs::copy(executable, temporary.join(&executable_name))
        .map_err(|error| crate::io_error(executable, &error))?;

    let public = project.join(&config.app.public_dir);
    let public_copied = if public.is_dir() {
        copy_tree(&public, &temporary.join("public"))?;
        true
    } else {
        false
    };
    let client = out_dir.join("client");
    let client_copied = if client.is_dir() {
        copy_tree(&client, &temporary.join("client"))?;
        true
    } else {
        false
    };

    let app_manifest = out_dir.join(APP_MANIFEST);
    if !app_manifest.is_file() {
        return Err(CliError::Bundle(format!(
            "application manifest is missing at {}",
            app_manifest.display()
        )));
    }
    fs::copy(&app_manifest, temporary.join(APP_MANIFEST))
        .map_err(|error| crate::io_error(&app_manifest, &error))?;
    let client_manifest = out_dir.join(CLIENT_MANIFEST);
    let client_manifest_copied = if client_copied {
        if !client_manifest.is_file() {
            return Err(CliError::Bundle(format!(
                "client manifest is missing at {}",
                client_manifest.display()
            )));
        }
        fs::copy(&client_manifest, temporary.join(CLIENT_MANIFEST))
            .map_err(|error| crate::io_error(&client_manifest, &error))?;
        true
    } else {
        false
    };

    let mut deploy_config = config.clone();
    deploy_config.app.public_dir = PathBuf::from("public");
    deploy_config.build.out_dir = PathBuf::from(".");
    deploy_config.build.client_dir = None;
    let config = toml::to_string_pretty(&deploy_config)
        .map_err(|error| CliError::Bundle(format!("cannot serialize `rusty.toml`: {error}")))?;
    fs::write(temporary.join("rusty.toml"), config)
        .map_err(|error| crate::io_error(&temporary.join("rusty.toml"), &error))?;

    let deployment = DeploymentManifest {
        version: BUNDLE_VERSION,
        executable: executable_name,
        app_manifest: APP_MANIFEST.to_owned(),
        client_manifest: client_manifest_copied.then(|| CLIENT_MANIFEST.to_owned()),
        public: public_copied,
        client: client_copied,
    };
    let deployment = serde_json::to_string_pretty(&deployment)
        .map_err(|error| CliError::Bundle(error.to_string()))?;
    fs::write(temporary.join(DEPLOYMENT_MANIFEST), deployment)
        .map_err(|error| crate::io_error(&temporary.join(DEPLOYMENT_MANIFEST), &error))?;

    if dist.exists() {
        fs::remove_dir_all(dist).map_err(|error| crate::io_error(dist, &error))?;
    }
    fs::rename(temporary, dist).map_err(|error| crate::io_error(dist, &error))
}

fn copy_tree(source: &Path, destination: &Path) -> Result<(), CliError> {
    fs::create_dir_all(destination).map_err(|error| crate::io_error(destination, &error))?;
    let entries = fs::read_dir(source).map_err(|error| crate::io_error(source, &error))?;
    for entry in entries {
        let entry = entry.map_err(|error| crate::io_error(source, &error))?;
        let path = entry.path();
        let file_type = entry
            .file_type()
            .map_err(|error| crate::io_error(&path, &error))?;
        if file_type.is_symlink() {
            continue;
        }
        let target = destination.join(entry.file_name());
        if file_type.is_dir() {
            copy_tree(&path, &target)?;
        } else if file_type.is_file() {
            fs::copy(&path, &target).map_err(|error| crate::io_error(&path, &error))?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::create;
    use rusty_config::RustyConfig;
    use std::fs;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};

    struct TempDir {
        path: PathBuf,
    }

    impl TempDir {
        fn new(label: &str) -> Self {
            static NEXT: AtomicU64 = AtomicU64::new(0);
            let unique = NEXT.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!(
                "rusty-bundle-{}-{label}-{unique}",
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
    fn writes_a_standalone_bundle_and_replaces_stale_output() {
        let project = TempDir::new("project");
        let config = RustyConfig::default();
        let out_dir = project.path.join("target/rusty");
        let executable = project.path.join("target/debug/demo");
        fs::create_dir_all(&out_dir).expect("out dir");
        fs::create_dir_all(executable.parent().unwrap()).expect("binary dir");
        fs::write(&executable, "binary").expect("binary");
        fs::write(out_dir.join("app-manifest.json"), "{}").expect("app manifest");
        fs::create_dir_all(out_dir.join("client")).expect("client dir");
        fs::write(out_dir.join("client/demo.js"), "client").expect("client");
        fs::write(out_dir.join("client-manifest.json"), "{}").expect("client manifest");
        fs::create_dir_all(project.path.join("public/assets")).expect("public dir");
        fs::write(project.path.join("public/assets/site.css"), "body{}").expect("asset");
        fs::create_dir_all(out_dir.join("dist")).expect("stale dist");
        fs::write(out_dir.join("dist/stale"), "stale").expect("stale file");

        let bundle = create(&project.path, &config, &out_dir, &executable, "demo").expect("bundle");
        assert_eq!(bundle.path, out_dir.join("dist"));
        assert!(!bundle.path.join("stale").exists());
        assert_eq!(
            fs::read(
                bundle
                    .path
                    .join(format!("demo{}", std::env::consts::EXE_SUFFIX))
            )
            .expect("bundled binary"),
            b"binary"
        );
        assert_eq!(
            fs::read(bundle.path.join("public/assets/site.css")).expect("public asset"),
            b"body{}"
        );
        assert_eq!(
            fs::read(bundle.path.join("client/demo.js")).expect("client asset"),
            b"client"
        );
        assert!(bundle.path.join("app-manifest.json").is_file());
        assert!(bundle.path.join("client-manifest.json").is_file());

        let config = fs::read_to_string(bundle.path.join("rusty.toml")).expect("config");
        assert!(config.contains("out_dir = \".\""));
        assert!(config.contains("public_dir = \"public\""));
        assert!(!config.contains("client_dir"));
        let config = RustyConfig::parse(&config).expect("parse bundled config");
        assert_eq!(config.build.out_dir, PathBuf::from("."));
        assert_eq!(config.app.public_dir, PathBuf::from("public"));
        let deployment =
            fs::read_to_string(bundle.path.join("deployment.json")).expect("deployment");
        assert!(deployment.contains("\"client\": true"));
        assert!(deployment.contains("\"public\": true"));
    }

    #[test]
    fn omits_missing_public_and_client_directories() {
        let project = TempDir::new("minimal");
        let config = RustyConfig::default();
        let out_dir = project.path.join("target/rusty");
        let executable = project.path.join("target/debug/demo");
        fs::create_dir_all(&out_dir).expect("out dir");
        fs::create_dir_all(executable.parent().unwrap()).expect("binary dir");
        fs::write(&executable, "binary").expect("binary");
        fs::write(out_dir.join("app-manifest.json"), "{}").expect("app manifest");

        let bundle = create(&project.path, &config, &out_dir, &executable, "demo").expect("bundle");
        assert!(!bundle.path.join("public").exists());
        assert!(!bundle.path.join("client").exists());
        let deployment =
            fs::read_to_string(bundle.path.join("deployment.json")).expect("deployment");
        assert!(deployment.contains("\"client\": false"));
        assert!(deployment.contains("\"public\": false"));
        assert!(deployment.contains("\"clientManifest\": null"));
    }

    #[test]
    fn copy_tree_skips_symlinks() {
        let project = TempDir::new("symlinks");
        let source = project.path.join("source");
        let destination = project.path.join("destination");
        fs::create_dir_all(&source).expect("source");
        fs::write(source.join("file"), "content").expect("file");
        #[cfg(unix)]
        std::os::unix::fs::symlink(source.join("file"), source.join("link")).expect("symlink");
        super::copy_tree(&source, &destination).expect("copy");
        assert_eq!(
            fs::read(destination.join("file")).expect("file"),
            b"content"
        );
        #[cfg(unix)]
        assert!(!destination.join("link").exists());
    }
}
