//! Configuration parsing, loading, and validation tests.

#![forbid(unsafe_code)]

use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use rusty_config::{ConfigError, RustyConfig};

struct TempDir {
    path: PathBuf,
}

impl TempDir {
    fn new(label: &str) -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let unique = NEXT.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "rusty-config-{}-{label}-{unique}",
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
fn defaults_apply_when_sections_and_fields_are_missing() {
    let config = RustyConfig::parse(
        r#"
        [dev]
        port = 4100
        "#,
    )
    .expect("parse");

    assert_eq!(config.app.source_root, PathBuf::from("src"));
    assert_eq!(config.app.app_dir, PathBuf::from("src/app"));
    assert_eq!(config.app.public_dir, PathBuf::from("public"));
    assert_eq!(config.build.out_dir, PathBuf::from("target/rusty"));
    assert_eq!(config.build.client_dir, None);
    assert_eq!(config.dev.host, "127.0.0.1");
    assert_eq!(config.dev.port, 4100);
}

#[test]
fn unknown_fields_and_invalid_values_are_rejected() {
    assert!(matches!(
        RustyConfig::parse("[app]\nunknown = true\n"),
        Err(ConfigError::Parse { .. })
    ));
    assert_eq!(
        RustyConfig::parse("[dev]\nport = 0\n")
            .expect("ephemeral port")
            .dev
            .port,
        0
    );
    assert!(matches!(
        RustyConfig::parse("[dev]\nhost = \"two words\"\n"),
        Err(ConfigError::Invalid { field, .. }) if field == "dev.host"
    ));
    assert!(matches!(
        RustyConfig::parse("[app]\nsource_root = \"../src\"\n"),
        Err(ConfigError::Invalid { field, .. }) if field == "app.source_root"
    ));
    assert!(matches!(
        RustyConfig::parse("[app]\npublic_dir = \"../public\"\n"),
        Err(ConfigError::Invalid { field, .. }) if field == "app.public_dir"
    ));
    assert!(matches!(
        RustyConfig::parse("[build]\nout_dir = \"/tmp/out\"\n"),
        Err(ConfigError::Invalid { field, .. }) if field == "build.out_dir"
    ));
    assert!(matches!(
        RustyConfig::parse("[build]\nclient_dir = \"../client\"\n"),
        Err(ConfigError::Invalid { field, .. }) if field == "build.client_dir"
    ));
}

#[test]
fn configuration_round_trips_through_toml() {
    let config = RustyConfig::parse(
        r#"
        [app]
        source_root = "app"
        app_dir = "app/pages"
        public_dir = "static"

        [build]
        out_dir = "dist/rusty"
        client_dir = "client"

        [dev]
        host = "0.0.0.0"
        port = 8080
        "#,
    )
    .expect("parse");

    let encoded = config.to_toml().expect("serialize");
    let restored = RustyConfig::parse(&encoded).expect("restore");
    assert_eq!(restored, config);
}

#[test]
fn missing_config_file_uses_defaults() {
    let directory = TempDir::new("missing");
    let config = RustyConfig::load(&directory.path).expect("load defaults");
    assert_eq!(config, RustyConfig::default());
}

#[test]
fn loads_and_validates_a_config_file() {
    let directory = TempDir::new("load");
    fs::write(
        RustyConfig::path(&directory.path),
        "[dev]\nhost = \"localhost\"\nport = 4000\n",
    )
    .expect("write config");

    let config = RustyConfig::load(&directory.path).expect("load");
    assert_eq!(config.dev.host, "localhost");
    assert_eq!(config.dev.port, 4000);
}

#[test]
fn invalid_config_file_reports_its_path() {
    let directory = TempDir::new("invalid");
    fs::write(RustyConfig::path(&directory.path), "not = [valid").expect("write config");

    let error = RustyConfig::load(&directory.path).expect_err("invalid config");
    assert!(matches!(error, ConfigError::Parse { path, .. } if path.ends_with("rusty.toml")));
}
