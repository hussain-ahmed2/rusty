//! CLI parsing, scaffolding, and process tests.

#![forbid(unsafe_code)]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command as ProcessCommand;
use std::sync::atomic::{AtomicU64, Ordering};

use rusty_cli::{CliError, Command, HELP};

struct TempDir {
    path: PathBuf,
}

impl TempDir {
    fn new(label: &str) -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let unique = NEXT.fetch_add(1, Ordering::Relaxed);
        let path =
            std::env::temp_dir().join(format!("rusty-cli-{}-{label}-{unique}", std::process::id()));
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
fn parses_commands_and_usage_errors() {
    assert_eq!(
        rusty_cli::parse::<_, &str>([]).expect("empty"),
        Command::Help
    );
    assert_eq!(rusty_cli::parse(["--help"]).expect("help"), Command::Help);
    assert_eq!(
        rusty_cli::parse(["--version"]).expect("version"),
        Command::Version
    );
    assert_eq!(
        rusty_cli::parse(["new", "demo"]).expect("new"),
        Command::New {
            path: PathBuf::from("demo"),
            client: false,
        }
    );
    assert_eq!(
        rusty_cli::parse(["new", "--client", "demo"]).expect("new client"),
        Command::New {
            path: PathBuf::from("demo"),
            client: true,
        }
    );
    assert_eq!(
        rusty_cli::parse(["check"]).expect("check"),
        Command::Check {
            project: PathBuf::from(".")
        }
    );
    assert_eq!(
        rusty_cli::parse(["generate", "--project", "app"]).expect("generate"),
        Command::Generate {
            project: PathBuf::from("app")
        }
    );
    assert_eq!(
        rusty_cli::parse(["check", "--project=app"]).expect("project eq"),
        Command::Check {
            project: PathBuf::from("app")
        }
    );
    assert_eq!(
        rusty_cli::parse(["new", "--help"]).expect("new help"),
        Command::Help
    );
    assert_eq!(
        rusty_cli::parse([
            "dev",
            "--project",
            "app",
            "--host",
            "0.0.0.0",
            "--port",
            "0"
        ])
        .expect("dev"),
        Command::Dev {
            project: PathBuf::from("app"),
            host: Some("0.0.0.0".to_owned()),
            port: Some(0),
        }
    );
    assert_eq!(
        rusty_cli::parse(["dev", "--project=app", "--port=4100"]).expect("dev eq"),
        Command::Dev {
            project: PathBuf::from("app"),
            host: None,
            port: Some(4100),
        }
    );
    assert_eq!(
        rusty_cli::parse(["build", "--project", "app", "--release"]).expect("build"),
        Command::Build {
            project: PathBuf::from("app"),
            release: true,
        }
    );
    assert_eq!(
        rusty_cli::parse(["start", "--project=app"]).expect("start"),
        Command::Start {
            project: PathBuf::from("app"),
            release: false,
        }
    );

    assert!(matches!(
        rusty_cli::parse(["unknown"]),
        Err(CliError::Usage(message)) if message.contains("unknown command")
    ));
    assert!(matches!(
        rusty_cli::parse(["new"]),
        Err(CliError::Usage(message)) if message.contains("destination path")
    ));
    assert!(matches!(
        rusty_cli::parse(["new", "one", "two"]),
        Err(CliError::Usage(message)) if message.contains("exactly one")
    ));
    assert!(matches!(
        rusty_cli::parse(["new", "one", "--bogus"]),
        Err(CliError::Usage(message)) if message.contains("unexpected")
    ));
    assert!(matches!(
        rusty_cli::parse(["check", "--project"]),
        Err(CliError::Usage(message)) if message.contains("requires")
    ));
    assert!(matches!(
        rusty_cli::parse(["check", "extra"]),
        Err(CliError::Usage(message)) if message.contains("unexpected")
    ));
    assert!(matches!(
        rusty_cli::parse(["check", "--project="]),
        Err(CliError::Usage(message)) if message.contains("requires")
    ));
    assert!(matches!(
        rusty_cli::parse(["dev", "--port", "70000"]),
        Err(CliError::Usage(message)) if message.contains("invalid port")
    ));
    assert!(matches!(
        rusty_cli::parse(["dev", "--host"]),
        Err(CliError::Usage(message)) if message.contains("requires")
    ));
    assert!(matches!(
        rusty_cli::parse(["dev", "extra"]),
        Err(CliError::Usage(message)) if message.contains("unexpected")
    ));
}

#[test]
fn build_runs_css_preprocessing_before_cargo() {
    let directory = TempDir::new("css-build");
    let project = directory.path.join("my-app");
    rusty_cli::run(["new", project.to_str().expect("utf8")]).expect("create");
    let config_path = project.join("rusty.toml");
    let mut config = fs::read_to_string(&config_path).expect("rusty.toml");
    config.push_str("\n[build.css]\ncommand = \"exit 7\"\n");
    fs::write(&config_path, config).expect("write config");

    let error = rusty_cli::run([
        "build".to_owned(),
        format!("--project={}", project.display()),
    ])
    .expect_err("css failure");
    assert!(matches!(
        error,
        CliError::CommandFailed {
            status: Some(7),
            ..
        }
    ));
}

#[test]
fn scaffolds_a_project_and_analyzes_it() {
    let directory = TempDir::new("scaffold");
    let project = directory.path.join("my-app");

    let report = rusty_cli::run(["new", project.to_str().expect("utf8")]).expect("create");
    assert!(report.message().contains("Created Rusty application"));

    let cargo = fs::read_to_string(project.join("Cargo.toml")).expect("Cargo.toml");
    assert!(cargo.contains("name = \"my-app\""));
    assert!(cargo.contains("rusty = { path ="));
    assert!(cargo.contains("edition = \"2021\""));
    assert!(project.join("rusty.toml").is_file());
    assert!(project.join("build.rs").is_file());
    assert!(project.join("src/main.rs").is_file());
    assert!(project.join("src/app/layout.rs").is_file());
    assert!(project.join("src/app/page.rs").is_file());
    assert!(project.join("src/app/not_found.rs").is_file());
    assert!(project.join("src/app/api/health/route.rs").is_file());
    assert!(project.join("public/robots.txt").is_file());
    assert!(project.join(".gitignore").is_file());
    assert!(!project.join("client").exists());
    assert!(!fs::read_to_string(project.join("rusty.toml"))
        .expect("rusty.toml")
        .contains("client_dir"));

    let main = fs::read_to_string(project.join("src/main.rs")).expect("main");
    assert!(main.contains("#[rusty::app]"));
    assert!(main.contains("app::serve()"));
    let route =
        fs::read_to_string(project.join("src/app/api/health/route.rs")).expect("health route");
    assert!(route.contains("pub fn get("));

    let checked = rusty_cli::run([
        "check".to_owned(),
        "--project".to_owned(),
        project.display().to_string(),
    ])
    .expect("check");
    assert!(checked.message().contains("1 page route(s)"));
    assert!(checked.message().contains("1 API route(s)"));
    assert!(checked.message().contains("0 server function(s)"));
    assert!(checked.message().contains("1 asset(s)"));

    let generated = rusty_cli::run([
        "generate".to_owned(),
        format!("--project={}", project.display()),
    ])
    .expect("generate");
    assert!(generated.message().contains("Wrote"));
    let manifest_path = project.join("target/rusty/app-manifest.json");
    assert!(manifest_path.is_file());
    let manifest = rusty_compiler::read_manifest(&manifest_path).expect("manifest");
    assert_eq!(manifest.routes().len(), 1);
    assert_eq!(manifest.routes().api_len(), 1);
    assert!(manifest.server_functions().is_empty());
    assert_eq!(manifest.assets().len(), 1);
    assert_eq!(manifest.assets()[0].url(), "/robots.txt");
}

#[test]
fn scaffolds_a_project_with_a_client() {
    let directory = TempDir::new("client");
    let project = directory.path.join("client-app");

    let report =
        rusty_cli::run(["new", project.to_str().expect("utf8"), "--client"]).expect("create");
    assert!(report.message().contains("WebAssembly client"));

    let config = fs::read_to_string(project.join("rusty.toml")).expect("rusty.toml");
    assert!(config.contains("client_dir = \"client\""));
    let client = fs::read_to_string(project.join("client/Cargo.toml")).expect("client Cargo.toml");
    assert!(client.contains("name = \"client-app-client\""));
    assert!(client.contains("name = \"client_app_client\""));
    assert!(client.contains("crate-type = [\"cdylib\", \"rlib\"]"));
    assert!(project.join("client/src/lib.rs").is_file());
    let layout = fs::read_to_string(project.join("src/app/layout.rs")).expect("layout");
    assert!(layout.contains("/_rusty/client/client_app_client.js"));
    let ignore = fs::read_to_string(project.join(".gitignore")).expect("gitignore");
    assert!(ignore.contains("/client/target"));

    rusty_cli::run([
        "check".to_owned(),
        "--project".to_owned(),
        project.display().to_string(),
    ])
    .expect("check");
}

#[test]
fn refuses_non_empty_destinations() {
    let directory = TempDir::new("non-empty");
    let project = directory.path.join("existing");
    fs::create_dir_all(&project).expect("create");
    fs::write(project.join("keep.txt"), "keep").expect("write");

    let error = rusty_cli::run(["new", project.to_str().expect("utf8")]).expect_err("exists");
    assert!(matches!(error, CliError::ExistingProject(path) if path == project));
    assert!(project.join("keep.txt").is_file());
}

#[test]
fn accepts_an_existing_empty_destination() {
    let directory = TempDir::new("empty");
    let project = directory.path.join("empty");
    fs::create_dir_all(&project).expect("create");

    rusty_cli::run(["new", project.to_str().expect("utf8")]).expect("create");
    assert!(project.join("Cargo.toml").is_file());
}

#[test]
fn reports_a_missing_project() {
    let directory = TempDir::new("missing");
    let project = directory.path.join("missing");

    let error = rusty_cli::run([
        "check".to_owned(),
        format!("--project={}", project.display()),
    ])
    .expect_err("missing");
    assert!(matches!(error, CliError::Compile(_)));
}

#[test]
fn help_and_version_reports_are_stable() {
    let help = rusty_cli::run(["--help"]).expect("help");
    assert_eq!(help.message(), HELP);
    assert!(help.message().contains("rusty new <path>"));

    let version = rusty_cli::run(["--version"]).expect("version");
    assert!(version.message().starts_with("rusty "));
}

#[test]
fn binary_uses_exit_codes_for_help_and_usage_errors() {
    let binary = env!("CARGO_BIN_EXE_rusty");

    let help = ProcessCommand::new(binary)
        .arg("--help")
        .output()
        .expect("run");
    assert!(help.status.success());
    assert!(String::from_utf8_lossy(&help.stdout).contains("Usage:"));

    let usage = ProcessCommand::new(binary)
        .arg("bogus")
        .output()
        .expect("run");
    assert_eq!(usage.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&usage.stderr).contains("unknown command"));
}

#[test]
fn derived_crate_names_are_sanitized() {
    let directory = TempDir::new("names");
    let project = directory.path.join("9 demo app");

    rusty_cli::run(["new", project.to_str().expect("utf8")]).expect("create");
    let cargo = fs::read_to_string(project.join("Cargo.toml")).expect("Cargo.toml");
    assert!(cargo.contains("name = \"app-9-demo-app\""));
}

#[test]
fn facade_dependency_is_a_local_path() {
    let directory = TempDir::new("facade");
    let project = directory.path.join("facade-app");
    rusty_cli::run(["new", project.to_str().expect("utf8")]).expect("create");

    let cargo = fs::read_to_string(project.join("Cargo.toml")).expect("Cargo.toml");
    assert!(cargo.contains(Path::new("crates/rusty").display().to_string().as_str()));
}
