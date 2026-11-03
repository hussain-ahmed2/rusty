//! Development diagnostics server with polling-based manifest regeneration.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, RwLock};
use std::thread;
use std::time::{Duration, SystemTime};

use rusty_compiler::AppCompiler;
use rusty_config::RustyConfig;
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

/// A running development diagnostics server.
///
/// The server analyzes the project, serves the current manifest and status, and
/// polls source files for changes. Regeneration is intentionally separate from
/// application route dispatch, which still requires compiler-generated runtime
/// registration.
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
        let mut snapshot = snapshot_project(&root).unwrap_or_default();
        while !stop.load(Ordering::Acquire) {
            thread::sleep(POLL_INTERVAL);
            if stop.load(Ordering::Acquire) {
                break;
            }
            let Ok(next) = snapshot_project(&root) else {
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
                "<p>Generation {}: {} page route(s), {} API route(s), {} server function(s).</p>",
                state.generation,
                manifest.routes().len(),
                manifest.routes().api_len(),
                manifest.server_functions().len()
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

fn snapshot_project(root: &Path) -> std::io::Result<BTreeMap<PathBuf, (SystemTime, u64)>> {
    let mut files = BTreeMap::new();
    visit(root, root, &mut files)?;
    Ok(files)
}

fn visit(
    root: &Path,
    directory: &Path,
    files: &mut BTreeMap<PathBuf, (SystemTime, u64)>,
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
            visit(root, &path, files)?;
        } else if file_type.is_file() && is_watched_file(&name) {
            let metadata = entry.metadata()?;
            let modified = metadata.modified().unwrap_or(SystemTime::UNIX_EPOCH);
            let relative = path.strip_prefix(root).unwrap_or(&path).to_path_buf();
            files.insert(relative, (modified, metadata.len()));
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
