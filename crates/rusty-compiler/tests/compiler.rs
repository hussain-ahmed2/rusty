//! Application analysis and manifest tests.

#![forbid(unsafe_code)]

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use rusty_compiler::{AppCompiler, CompileError, MANIFEST_FILE};
use rusty_config::RustyConfig;

struct Project {
    path: PathBuf,
}

impl Project {
    fn new(label: &str) -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let unique = NEXT.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "rusty-compiler-{}-{label}-{unique}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).expect("create project");
        Self { path }
    }

    fn write(&self, relative: &str, contents: &str) {
        let path = self.path.join(relative);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("create parent");
        }
        fs::write(path, contents).expect("write file");
    }

    fn compiler(&self) -> AppCompiler {
        AppCompiler::from_project(&self.path).expect("compiler")
    }
}

impl Drop for Project {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

fn full_project(label: &str) -> Project {
    let project = Project::new(label);
    project.write("src/app/page.rs", "pub fn page() {}\n");
    project.write("src/app/blog/[slug]/page.rs", "pub fn page() {}\n");
    project.write("src/app/api/health/route.rs", "pub fn GET() {}\n");
    project.write(
        "src/lib.rs",
        r#"
        #[server]
        pub fn root_fn() -> Result<(), RpcError> { Ok(()) }
        "#,
    );
    project.write(
        "src/rpc.rs",
        r#"
        #[server]
        pub fn add(left: i32, right: i32) -> Result<i32, RpcError> { Ok(left + right) }

        #[server("/custom/greet")]
        pub fn greet(name: String) -> Result<String, RpcError> { Ok(name) }

        #[rusty::server(path = "/nested/ping")]
        pub fn ping() -> Result<(), RpcError> { Ok(()) }

        pub mod internal {
            #[server]
            pub fn hidden() -> Result<(), RpcError> { Ok(()) }
        }
        "#,
    );
    project.write(
        "src/handlers/mod.rs",
        r#"
        #[server]
        pub fn status() -> Result<(), RpcError> { Ok(()) }
        "#,
    );
    project.write(
        "src/bin/tool.rs",
        r#"
        #[server]
        pub fn ignored() -> Result<(), RpcError> { Ok(()) }
        "#,
    );
    project.write("public/logo.svg", "<svg></svg>\n");
    project.write("public/fonts/site.css", "body { color: black; }\n");
    project
}

#[test]
fn analyzes_routes_and_server_functions() {
    let project = full_project("full");
    let manifest = project.compiler().analyze().expect("analyze");

    assert_eq!(manifest.version(), rusty_compiler::MANIFEST_VERSION);
    assert_eq!(manifest.source_root(), Path::new("src"));
    assert_eq!(manifest.routes().len(), 2);
    assert_eq!(manifest.routes().api_len(), 1);
    assert_eq!(manifest.assets().len(), 2);
    assert_eq!(manifest.assets()[0].url(), "/fonts/site.css");
    assert_eq!(manifest.assets()[0].path(), Path::new("fonts/site.css"));
    assert_eq!(manifest.assets()[0].bytes(), 23);
    assert_eq!(manifest.assets()[0].fingerprint().len(), 16);

    let pages = manifest
        .routes()
        .routes()
        .iter()
        .map(|route| route.pattern().to_string())
        .collect::<Vec<_>>();
    assert!(pages.contains(&"/blog/[slug]".to_owned()));
    assert!(pages.contains(&"/".to_owned()));
    assert_eq!(
        manifest.routes().api_routes()[0].pattern().to_string(),
        "/api/health"
    );

    let functions = manifest
        .server_functions()
        .iter()
        .map(|function| (function.path(), function))
        .collect::<BTreeMap<_, _>>();
    assert_eq!(functions.len(), 6);
    assert_eq!(functions["/_rusty/rpc/add"].name(), "add");
    assert_eq!(functions["/_rusty/rpc/add"].module(), "crate::rpc");
    assert_eq!(
        functions["/_rusty/rpc/add"].source(),
        Path::new("src/rpc.rs")
    );
    assert_eq!(functions["/custom/greet"].name(), "greet");
    assert_eq!(functions["/nested/ping"].name(), "ping");
    assert_eq!(
        functions["/_rusty/rpc/hidden"].module(),
        "crate::rpc::internal"
    );
    assert_eq!(functions["/_rusty/rpc/root_fn"].module(), "crate");
    assert_eq!(functions["/_rusty/rpc/status"].module(), "crate::handlers");
    assert!(!functions.contains_key("/_rusty/rpc/ignored"));
}

#[test]
fn manifest_round_trips_through_json_and_disk() {
    let project = full_project("manifest");
    let compiler = project.compiler();
    let manifest = compiler.analyze().expect("analyze");

    let json = manifest.to_json_pretty().expect("serialize");
    let restored = rusty_compiler::AppManifest::from_json(&json).expect("restore");
    assert_eq!(restored, manifest);

    let written = compiler.analyze_and_write().expect("write");
    assert_eq!(written, manifest);
    let path = project.path.join("target/rusty").join(MANIFEST_FILE);
    assert!(path.is_file());
    let read = rusty_compiler::read_manifest(&path).expect("read");
    assert_eq!(read, manifest);
}

#[test]
fn rejects_duplicate_server_paths() {
    let project = Project::new("duplicate");
    project.write("src/app/page.rs", "pub fn page() {}\n");
    project.write(
        "src/a.rs",
        "#[server(\"/same\")] pub fn first() -> Result<(), RpcError> { Ok(()) }\n",
    );
    project.write(
        "src/b.rs",
        "#[server(\"/same\")] pub fn second() -> Result<(), RpcError> { Ok(()) }\n",
    );

    let error = project.compiler().analyze().expect_err("duplicate");
    assert!(matches!(
        error,
        CompileError::DuplicateServerPath { path, .. } if path == "/same"
    ));
}

#[test]
fn rejects_invalid_server_declarations() {
    let project = Project::new("invalid-server");
    project.write("src/app/page.rs", "pub fn page() {}\n");
    project.write(
        "src/rpc.rs",
        "#[server(\"no-leading-slash\")] pub fn invalid() -> Result<(), RpcError> { Ok(()) }\n",
    );

    let error = project.compiler().analyze().expect_err("invalid path");
    assert!(matches!(error, CompileError::InvalidServerFunction { .. }));

    project.write(
        "src/rpc.rs",
        "#[server(path = 42)] pub fn invalid() -> Result<(), RpcError> { Ok(()) }\n",
    );
    let error = project.compiler().analyze().expect_err("invalid option");
    assert!(matches!(error, CompileError::InvalidServerFunction { .. }));
}

#[test]
fn rejects_unparsable_source_and_missing_directories() {
    let project = Project::new("broken");
    project.write("src/app/page.rs", "pub fn page() {}\n");
    project.write("src/broken.rs", "pub fn broken(\n");

    let error = project.compiler().analyze().expect_err("parse");
    assert!(matches!(error, CompileError::InvalidSource { .. }));

    let missing = Project::new("missing-app");
    let error = missing.compiler().analyze().expect_err("missing app");
    assert!(matches!(
        error,
        CompileError::MissingDirectory {
            kind: "application",
            ..
        }
    ));
}

#[test]
fn honors_configured_source_and_application_directories() {
    let project = Project::new("configured");
    project.write(
        "rusty.toml",
        "[app]\nsource_root = \"lib\"\napp_dir = \"lib/routes\"\n\n[build]\nout_dir = \"dist/rusty\"\n",
    );
    project.write("lib/routes/page.rs", "pub fn page() {}\n");
    project.write(
        "lib/rpc.rs",
        "#[server] pub fn ping() -> Result<(), RpcError> { Ok(()) }\n",
    );

    let compiler = project.compiler();
    assert_eq!(compiler.source_root(), project.path.join("lib"));
    assert_eq!(compiler.app_dir(), project.path.join("lib/routes"));
    assert_eq!(
        compiler.manifest_path(),
        project.path.join("dist/rusty").join(MANIFEST_FILE)
    );

    let manifest = compiler.analyze().expect("analyze");
    assert_eq!(manifest.source_root(), Path::new("lib"));
    assert_eq!(manifest.routes().len(), 1);
    assert_eq!(
        manifest.server_functions()[0].source(),
        Path::new("lib/rpc.rs")
    );
}

#[test]
fn rejects_unknown_manifest_versions() {
    let project = full_project("version");
    let manifest = project.compiler().analyze().expect("analyze");
    let json = manifest
        .to_json()
        .expect("serialize")
        .replace("\"version\":3", "\"version\":99");

    let error = rusty_compiler::AppManifest::from_json(&json).expect_err("version");
    assert!(matches!(error, CompileError::Manifest(message) if message.contains("version")));
}

#[test]
fn validates_routes_when_restoring_a_manifest() {
    let project = full_project("route-validation");
    let manifest = project.compiler().analyze().expect("analyze");
    let json =
        manifest
            .to_json()
            .expect("serialize")
            .replacen("\"page\":\"page.rs\"", "\"page\":\"\"", 1);

    let error = rusty_compiler::AppManifest::from_json(&json).expect_err("invalid route");
    assert!(matches!(error, CompileError::Manifest(message) if message.contains("page path")));
}

#[test]
fn accepts_an_explicit_configuration() {
    let project = Project::new("explicit-config");
    project.write("src/app/page.rs", "pub fn page() {}\n");
    project.write("src/main.rs", "fn main() {}\n");

    let compiler = AppCompiler::new(&project.path, RustyConfig::default()).expect("compiler");
    let manifest = compiler.analyze().expect("analyze");
    assert!(manifest.server_functions().is_empty());
    assert_eq!(manifest.routes().len(), 1);
}
