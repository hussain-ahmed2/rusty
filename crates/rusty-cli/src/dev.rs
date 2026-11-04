//! Development process supervision and polling-based manifest diagnostics.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, RwLock};
use std::thread;
use std::time::{Duration, SystemTime};

use rusty_compiler::AppCompiler;
use rusty_config::{CssConfig, RustyConfig};
use rusty_server::{HttpRouter, HttpServer, Response, ShutdownHandle, StatusCode};
use serde_json::json;

use crate::CliError;

const POLL_INTERVAL: Duration = Duration::from_millis(250);

#[derive(Debug, Default)]
struct DevState {
    manifest: Option<rusty_compiler::AppManifest>,
    error: Option<String>,
    generation: u64,
}

/// Runs an application with rebuild-on-change for development.
#[derive(Debug, Clone)]
pub struct DevRunner {
    project: PathBuf,
    host: Option<String>,
    port: Option<u16>,
}

impl DevRunner {
    /// Creates a development process supervisor.
    #[must_use]
    pub fn new(project: impl Into<PathBuf>, host: Option<String>, port: Option<u16>) -> Self {
        Self {
            project: project.into(),
            host,
            port,
        }
    }

    /// Runs the application and restarts it when watched files change.
    ///
    /// The application is started with `cargo run` and `RUSTY_DEV=1`. When the
    /// application exits on its own, the supervisor waits for the next change
    /// before restarting it. Configured CSS preprocessing and the WebAssembly
    /// client are built before the first start and rebuilt when their inputs
    /// change.
    ///
    /// # Errors
    ///
    /// Returns a configuration error when `rusty.toml` cannot be read, or an
    /// I/O error when `cargo` cannot be started or the project snapshot cannot
    /// be read.
    pub fn run(self) -> Result<(), CliError> {
        let root = self.project.clone();
        let mut config = RustyConfig::load(&root)?;
        let mut css_stale = !build_css(&root, &config);
        let mut client_stale = !build_client(&root, &config);
        let mut snapshot =
            snapshot_project(&root, &config.build.css).map_err(|error| io_error(&root, &error))?;
        let mut child = Some(self.spawn()?);
        println!(
            "Rusty dev is watching {}; press Ctrl-C to stop.",
            root.display()
        );

        loop {
            thread::sleep(POLL_INTERVAL);
            match snapshot_project(&root, &config.build.css) {
                Ok(next) if next != snapshot => {
                    let loaded = RustyConfig::load(&root);
                    let css_dirty = match &loaded {
                        Ok(next_config) => {
                            css_inputs_changed(&snapshot, &next, &next_config.build.css)
                        }
                        Err(_) => css_inputs_changed(&snapshot, &next, &config.build.css),
                    };
                    let client_dirty = match &loaded {
                        Ok(next_config) => {
                            next_config
                                .build
                                .client_dir
                                .as_deref()
                                .is_some_and(|client_dir| {
                                    client_inputs_changed(&snapshot, &next, client_dir)
                                })
                        }
                        Err(_) => config
                            .build
                            .client_dir
                            .as_deref()
                            .is_some_and(|client_dir| {
                                client_inputs_changed(&snapshot, &next, client_dir)
                            }),
                    };
                    snapshot = next;
                    match loaded {
                        Ok(next) => config = next,
                        Err(error) => eprintln!("cannot reload rusty.toml: {error}"),
                    }
                    if let Some(mut child) = child.take() {
                        stop(&mut child);
                    }
                    if css_dirty || css_stale {
                        css_stale = !build_css(&root, &config);
                    }
                    if client_dirty || client_stale {
                        client_stale = !build_client(&root, &config);
                    }
                    snapshot = snapshot_project(&root, &config.build.css).unwrap_or(snapshot);
                    child = Some(self.spawn()?);
                }
                Ok(_) => {
                    if let Some(status) = child.as_mut().and_then(wait_status) {
                        eprintln!("application exited with {status}; waiting for changes");
                        child = None;
                    }
                }
                Err(error) => {
                    eprintln!("cannot watch {}: {error}", root.display());
                }
            }
        }
    }

    fn spawn(&self) -> Result<Child, CliError> {
        let mut command = Command::new("cargo");
        command
            .arg("run")
            .arg("--quiet")
            .current_dir(&self.project)
            .env("RUSTY_DEV", "1");
        if let Some(host) = &self.host {
            command.env("RUSTY_HOST", host);
        }
        if let Some(port) = self.port {
            command.env("RUSTY_PORT", port.to_string());
        }
        command.spawn().map_err(|error| CliError::Io {
            path: PathBuf::from("cargo"),
            message: error.to_string(),
        })
    }
}

fn build_css(root: &Path, config: &RustyConfig) -> bool {
    match crate::css::run(root, config) {
        Ok(true) => true,
        Ok(false) => true,
        Err(error) => {
            eprintln!("{error}");
            false
        }
    }
}

fn build_client(root: &Path, config: &RustyConfig) -> bool {
    if config.build.client_dir.is_none() {
        return true;
    }
    match crate::wasm::build(root, config, false) {
        Ok(Some(client)) => {
            println!(
                "Built client {} with {} artifact(s).",
                client.entrypoint, client.files
            );
            true
        }
        Ok(None) => true,
        Err(error) => {
            eprintln!("client build failed: {error}");
            false
        }
    }
}

fn client_inputs_changed(
    old: &BTreeMap<PathBuf, (SystemTime, u64)>,
    new: &BTreeMap<PathBuf, (SystemTime, u64)>,
    client_dir: &Path,
) -> bool {
    old.iter()
        .any(|(path, stamp)| new.get(path) != Some(stamp) && is_client_input(path, client_dir))
        || new
            .iter()
            .any(|(path, _)| !old.contains_key(path) && is_client_input(path, client_dir))
}

fn is_client_input(path: &Path, client_dir: &Path) -> bool {
    path.starts_with(client_dir)
        || path == Path::new("rusty.toml")
        || path.file_name() == Some(std::ffi::OsStr::new("Cargo.toml"))
}

fn css_inputs_changed(
    old: &BTreeMap<PathBuf, (SystemTime, u64)>,
    new: &BTreeMap<PathBuf, (SystemTime, u64)>,
    css: &CssConfig,
) -> bool {
    if css.command.is_none() {
        return false;
    }
    old.iter()
        .any(|(path, stamp)| new.get(path) != Some(stamp) && is_css_input(path, css))
        || new
            .iter()
            .any(|(path, _)| !old.contains_key(path) && is_css_input(path, css))
}

fn is_css_input(path: &Path, css: &CssConfig) -> bool {
    if css.command.is_none() {
        return false;
    }
    if path == Path::new("rusty.toml") {
        return true;
    }
    if !css.inputs.is_empty() {
        return css.inputs.iter().any(|input| path.starts_with(input));
    }
    let extension = path
        .extension()
        .and_then(std::ffi::OsStr::to_str)
        .map(str::to_ascii_lowercase);
    matches!(
        extension.as_deref(),
        Some("css" | "scss" | "sass" | "less" | "pcss")
    ) || path
        .file_name()
        .and_then(std::ffi::OsStr::to_str)
        .is_some_and(|name| {
            matches!(
                name,
                "package.json"
                    | "tailwind.config.js"
                    | "tailwind.config.cjs"
                    | "tailwind.config.mjs"
                    | "tailwind.config.ts"
                    | "postcss.config.js"
                    | "postcss.config.cjs"
                    | "postcss.config.mjs"
                    | "postcss.config.ts"
            )
        })
}

fn wait_status(child: &mut Child) -> Option<ExitStatus> {
    child.try_wait().ok().flatten()
}

fn stop(child: &mut Child) {
    let _ = child.kill();
    let _ = child.wait();
}

fn io_error(path: &Path, error: &std::io::Error) -> CliError {
    CliError::Io {
        path: path.to_path_buf(),
        message: error.to_string(),
    }
}

/// A standalone development manifest server.
///
/// `rusty dev` now runs the application itself. This server remains available to
/// tooling that only needs manifest analysis, status JSON, and a diagnostics
/// dashboard without executing application code.
#[derive(Debug)]
pub struct DevServer {
    compiler: AppCompiler,
    server: HttpServer,
    state: Arc<RwLock<DevState>>,
}

impl DevServer {
    /// Analyzes the project, writes the initial manifest, and binds the server.
    ///
    /// Analysis errors are reported through the status endpoint instead of
    /// preventing the server from starting. Binding errors are returned.
    ///
    /// # Errors
    ///
    /// Returns a configuration, compiler, or server error when the project
    /// cannot be analyzed structurally or the listener cannot bind.
    pub fn bind(project: impl AsRef<Path>, config: RustyConfig) -> Result<Self, CliError> {
        let compiler = AppCompiler::new(project, config.clone())?;
        let state = Arc::new(RwLock::new(DevState::default()));
        refresh(&compiler, &state);
        let server = HttpServer::bind((config.dev.host.as_str(), config.dev.port))
            .map_err(|error| CliError::Server(error.to_string()))?;
        Ok(Self {
            compiler,
            server,
            state,
        })
    }

    /// Returns the bound address.
    ///
    /// # Errors
    ///
    /// Returns a server error when the local address cannot be queried.
    pub fn local_addr(&self) -> Result<std::net::SocketAddr, CliError> {
        self.server
            .local_addr()
            .map_err(|error| CliError::Server(error.to_string()))
    }

    /// Returns a handle that stops the server and watcher.
    #[must_use]
    pub fn shutdown_handle(&self) -> ShutdownHandle {
        self.server.shutdown_handle()
    }

    /// Returns the most recent analysis error, if any.
    #[must_use]
    pub fn initial_error(&self) -> Option<String> {
        read_state(&self.state).error.clone()
    }

    /// Serves diagnostics and watches for source changes until shutdown.
    ///
    /// # Errors
    ///
    /// Returns a server error when the accept loop fails.
    pub fn run(self) -> Result<(), CliError> {
        let project = Arc::new(self.compiler.project_root().to_path_buf());
        let router = build_router(Arc::clone(&self.state), project)?;
        let stop = Arc::new(AtomicBool::new(false));
        let watcher = spawn_watcher(
            self.compiler.clone(),
            Arc::clone(&self.state),
            Arc::clone(&stop),
        );

        let result = self.server.run_router(router);
        stop.store(true, Ordering::Release);
        let _ = watcher.join();
        result.map_err(|error| CliError::Server(error.to_string()))
    }
}

fn refresh(compiler: &AppCompiler, state: &Arc<RwLock<DevState>>) {
    let analyzed = compiler.analyze_and_write();
    let mut state = write_state(state);
    state.generation = state.generation.saturating_add(1);
    match analyzed {
        Ok(manifest) => {
            state.manifest = Some(manifest);
            state.error = None;
        }
        Err(error) => {
            state.error = Some(error.to_string());
        }
    }
}

fn spawn_watcher(
    compiler: AppCompiler,
    state: Arc<RwLock<DevState>>,
    stop: Arc<AtomicBool>,
) -> thread::JoinHandle<()> {
    thread::spawn(move || {
        let root = compiler.project_root().to_path_buf();
        let mut snapshot =
            snapshot_project(&root, &compiler.config().build.css).unwrap_or_default();
        while !stop.load(Ordering::Acquire) {
            thread::sleep(POLL_INTERVAL);
            if stop.load(Ordering::Acquire) {
                break;
            }
            let Ok(next) = snapshot_project(&root, &compiler.config().build.css) else {
                continue;
            };
            if next != snapshot {
                snapshot = next;
                refresh(&compiler, &state);
            }
        }
    })
}

fn build_router(
    state: Arc<RwLock<DevState>>,
    project: Arc<PathBuf>,
) -> Result<HttpRouter, CliError> {
    let status_state = Arc::clone(&state);
    let status_project = Arc::clone(&project);
    let manifest_state = Arc::clone(&state);
    let dashboard_state = Arc::clone(&state);

    let router = HttpRouter::new()
        .get("/__rusty/health", |_| {
            Ok(Response::text(StatusCode::OK, "ok"))
        })?
        .get("/__rusty/status", move |_| {
            Ok(status_response(&status_state, &status_project))
        })?
        .get("/__rusty/manifest", move |_| {
            Ok(manifest_response(&manifest_state))
        })?
        .get("/", move |_| {
            Ok(dashboard_response(&dashboard_state, &project))
        })?
        .fallback(|_| {
            Ok(Response::text(
                StatusCode::NOT_FOUND,
                "not found; try /, /__rusty/status, or /__rusty/manifest",
            ))
        });
    Ok(router)
}

fn status_response(state: &Arc<RwLock<DevState>>, project: &Path) -> Response {
    let state = read_state(state);
    let manifest = state.manifest.as_ref();
    let payload = json!({
        "ok": state.error.is_none(),
        "generation": state.generation,
        "project": project.display().to_string(),
        "routes": manifest.map_or(0, |manifest| manifest.routes().len()),
        "apiRoutes": manifest.map_or(0, |manifest| manifest.routes().api_len()),
        "serverFunctions": manifest.map_or(0, |manifest| manifest.server_functions().len()),
        "assets": manifest.map_or(0, |manifest| manifest.assets().len()),
        "error": state.error,
    });
    Response::json(StatusCode::OK, payload.to_string())
}

fn manifest_response(state: &Arc<RwLock<DevState>>) -> Response {
    let state = read_state(state);
    if let Some(error) = &state.error {
        return Response::json(
            StatusCode::SERVICE_UNAVAILABLE,
            json!({ "error": error }).to_string(),
        );
    }
    let Some(manifest) = &state.manifest else {
        return Response::json(
            StatusCode::SERVICE_UNAVAILABLE,
            json!({ "error": "no manifest has been generated yet" }).to_string(),
        );
    };
    match manifest.to_json_pretty() {
        Ok(json) => Response::json(StatusCode::OK, json),
        Err(error) => Response::json(
            StatusCode::INTERNAL_SERVER_ERROR,
            json!({ "error": error.to_string() }).to_string(),
        ),
    }
}

fn dashboard_response(state: &Arc<RwLock<DevState>>, project: &Path) -> Response {
    let state = read_state(state);
    let manifest = state.manifest.as_ref();
    let mut body = String::from(
        "<!doctype html><html><head><meta charset=\"utf-8\"><title>Rusty dev</title></head><body><h1>Rusty dev</h1>",
    );
    body.push_str("<p>Project: <code>");
    body.push_str(&escape_html(&project.display().to_string()));
    body.push_str("</code></p>");

    match (&state.error, manifest) {
        (Some(error), _) => {
            body.push_str("<p><strong>Analysis error</strong></p><pre>");
            body.push_str(&escape_html(error));
            body.push_str("</pre>");
        }
        (None, Some(manifest)) => {
            body.push_str(&format!(
                "<p>Generation {}: {} page route(s), {} API route(s), {} server function(s), and {} asset(s).</p>",
                state.generation,
                manifest.routes().len(),
                manifest.routes().api_len(),
                manifest.server_functions().len(),
                manifest.assets().len()
            ));
        }
        (None, None) => body.push_str("<p>No manifest has been generated yet.</p>"),
    }

    if let Some(manifest) = manifest {
        body.push_str("<h2>Pages</h2><ul>");
        for route in manifest.routes().routes() {
            body.push_str("<li><code>");
            body.push_str(&escape_html(&route.pattern().to_string()));
            body.push_str("</code> &rarr; ");
            body.push_str(&escape_html(&route.page().display().to_string()));
            body.push_str("</li>");
        }
        body.push_str("</ul><h2>API routes</h2><ul>");
        for route in manifest.routes().api_routes() {
            body.push_str("<li><code>");
            body.push_str(&escape_html(&route.pattern().to_string()));
            body.push_str("</code> &rarr; ");
            body.push_str(&escape_html(&route.route().display().to_string()));
            body.push_str("</li>");
        }
        body.push_str("</ul><h2>Server functions</h2><ul>");
        for function in manifest.server_functions() {
            body.push_str("<li><code>");
            body.push_str(&escape_html(function.path()));
            body.push_str("</code> &rarr; ");
            body.push_str(&escape_html(function.module()));
            body.push_str("::");
            body.push_str(&escape_html(function.name()));
            body.push_str("</li>");
        }
        body.push_str("</ul>");
    }
    body.push_str("</body></html>");
    Response::html(StatusCode::OK, body)
}

fn escape_html(text: &str) -> String {
    let mut escaped = String::with_capacity(text.len());
    for character in text.chars() {
        match character {
            '&' => escaped.push_str("&amp;"),
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            '"' => escaped.push_str("&quot;"),
            '\'' => escaped.push_str("&#39;"),
            other => escaped.push(other),
        }
    }
    escaped
}

fn snapshot_project(
    root: &Path,
    css: &CssConfig,
) -> std::io::Result<BTreeMap<PathBuf, (SystemTime, u64)>> {
    let mut files = BTreeMap::new();
    visit(root, root, &mut files, css)?;
    Ok(files)
}

fn visit(
    root: &Path,
    directory: &Path,
    files: &mut BTreeMap<PathBuf, (SystemTime, u64)>,
    css: &CssConfig,
) -> std::io::Result<()> {
    for entry in fs::read_dir(directory)? {
        let entry = entry?;
        let path = entry.path();
        let file_type = entry.file_type()?;
        let name = entry.file_name();
        let name = name.to_string_lossy();

        if file_type.is_dir() {
            if matches!(name.as_ref(), "target" | ".git" | "node_modules") {
                continue;
            }
            visit(root, &path, files, css)?;
        } else if file_type.is_file() {
            let relative = path.strip_prefix(root).unwrap_or(&path);
            if is_watched_file(&name) || is_css_input(relative, css) {
                let metadata = entry.metadata()?;
                let modified = metadata.modified().unwrap_or(SystemTime::UNIX_EPOCH);
                files.insert(relative.to_path_buf(), (modified, metadata.len()));
            }
        }
    }
    Ok(())
}

fn is_watched_file(name: &str) -> bool {
    name == "rusty.toml" || name == "Cargo.toml" || name.ends_with(".rs")
}

fn read_state(state: &Arc<RwLock<DevState>>) -> std::sync::RwLockReadGuard<'_, DevState> {
    state
        .read()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

fn write_state(state: &Arc<RwLock<DevState>>) -> std::sync::RwLockWriteGuard<'_, DevState> {
    state
        .write()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

#[cfg(test)]
mod tests {
    use super::{client_inputs_changed, css_inputs_changed, is_css_input};
    use rusty_config::CssConfig;
    use std::collections::BTreeMap;
    use std::path::{Path, PathBuf};
    use std::time::{Duration, SystemTime};

    fn snapshot(entries: &[(&str, u64)]) -> BTreeMap<PathBuf, (SystemTime, u64)> {
        entries
            .iter()
            .map(|(path, size)| {
                (
                    PathBuf::from(path),
                    (SystemTime::UNIX_EPOCH + Duration::from_secs(*size), *size),
                )
            })
            .collect()
    }

    #[test]
    fn only_client_inputs_trigger_client_rebuilds() {
        let client = Path::new("client");
        let old = snapshot(&[
            ("client/src/lib.rs", 1),
            ("src/app/page.rs", 1),
            ("rusty.toml", 1),
            ("Cargo.toml", 1),
        ]);
        let app_changed = snapshot(&[
            ("client/src/lib.rs", 1),
            ("src/app/page.rs", 2),
            ("rusty.toml", 1),
            ("Cargo.toml", 1),
        ]);
        assert!(!client_inputs_changed(&old, &app_changed, client));

        let client_changed = snapshot(&[
            ("client/src/lib.rs", 2),
            ("src/app/page.rs", 1),
            ("rusty.toml", 1),
            ("Cargo.toml", 1),
        ]);
        assert!(client_inputs_changed(&old, &client_changed, client));

        let manifest_changed = snapshot(&[
            ("client/src/lib.rs", 1),
            ("src/app/page.rs", 1),
            ("rusty.toml", 2),
            ("Cargo.toml", 1),
        ]);
        assert!(client_inputs_changed(&old, &manifest_changed, client));

        let dependency_changed = snapshot(&[
            ("client/src/lib.rs", 1),
            ("src/app/page.rs", 1),
            ("rusty.toml", 1),
            ("Cargo.toml", 2),
        ]);
        assert!(client_inputs_changed(&old, &dependency_changed, client));
    }

    #[test]
    fn only_css_inputs_trigger_css_rebuilds() {
        let css = CssConfig {
            command: Some("sass styles/app.scss public/app.css".to_owned()),
            inputs: vec![PathBuf::from("styles"), PathBuf::from("sass.config.js")],
        };
        let old = snapshot(&[
            ("styles/app.scss", 1),
            ("sass.config.js", 1),
            ("src/app/page.rs", 1),
        ]);
        let rust_changed = snapshot(&[
            ("styles/app.scss", 1),
            ("sass.config.js", 1),
            ("src/app/page.rs", 2),
        ]);
        assert!(!css_inputs_changed(&old, &rust_changed, &css));

        let styles_changed = snapshot(&[
            ("styles/app.scss", 2),
            ("sass.config.js", 1),
            ("src/app/page.rs", 1),
        ]);
        assert!(css_inputs_changed(&old, &styles_changed, &css));

        let config_changed = snapshot(&[
            ("styles/app.scss", 1),
            ("sass.config.js", 2),
            ("src/app/page.rs", 1),
        ]);
        assert!(css_inputs_changed(&old, &config_changed, &css));
    }

    #[test]
    fn css_defaults_watch_stylesheets_and_tool_configs() {
        let css = CssConfig {
            command: Some("postcss styles/app.css -o public/app.css".to_owned()),
            inputs: Vec::new(),
        };
        assert!(is_css_input(Path::new("styles/app.css"), &css));
        assert!(is_css_input(Path::new("styles/app.scss"), &css));
        assert!(is_css_input(Path::new("tailwind.config.js"), &css));
        assert!(is_css_input(Path::new("package.json"), &css));
        assert!(!is_css_input(Path::new("src/app/page.rs"), &css));

        let disabled = CssConfig::default();
        assert!(!is_css_input(Path::new("styles/app.css"), &disabled));
        assert!(!css_inputs_changed(
            &snapshot(&[("styles/app.css", 1)]),
            &snapshot(&[("styles/app.css", 2)]),
            &disabled
        ));
    }
}
