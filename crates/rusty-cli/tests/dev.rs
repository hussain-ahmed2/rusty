//! Development diagnostics server and watcher tests.

#![forbid(unsafe_code)]

use std::fs;
use std::io::{Read, Write};
use std::net::{Shutdown, SocketAddr, TcpStream};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use rusty_cli::{CliError, DevServer};
use rusty_config::RustyConfig;
use serde_json::Value;

struct TempDir {
    path: PathBuf,
}

impl TempDir {
    fn new(label: &str) -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let unique = NEXT.fetch_add(1, Ordering::Relaxed);
        let path =
            std::env::temp_dir().join(format!("rusty-dev-{}-{label}-{unique}", std::process::id()));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).expect("create temp dir");
        Self { path }
    }

    fn write(&self, relative: &str, contents: &str) {
        let path = self.path.join(relative);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("create parent");
        }
        fs::write(path, contents).expect("write file");
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

struct RunningDev {
    address: SocketAddr,
    shutdown: rusty_server::ShutdownHandle,
    thread: Option<JoinHandle<Result<(), CliError>>>,
}

impl RunningDev {
    fn start(project: &PathBuf) -> Self {
        let mut config = RustyConfig::default();
        config.dev.port = 0;
        let server = DevServer::bind(project, config).expect("bind dev server");
        let address = server.local_addr().expect("local address");
        let shutdown = server.shutdown_handle();
        let thread = thread::spawn(move || server.run());
        Self {
            address,
            shutdown,
            thread: Some(thread),
        }
    }

    fn get(&self, path: &str) -> (u16, String) {
        let mut stream = TcpStream::connect(self.address).expect("connect");
        stream
            .set_read_timeout(Some(Duration::from_secs(5)))
            .expect("read timeout");
        stream
            .write_all(format!("GET {path} HTTP/1.1\r\nHost: localhost\r\n\r\n").as_bytes())
            .expect("write");
        stream.shutdown(Shutdown::Write).expect("shutdown write");

        let mut response = Vec::new();
        stream.read_to_end(&mut response).expect("read");
        let response = String::from_utf8(response).expect("utf8 response");
        let (head, body) = response.split_once("\r\n\r\n").expect("response split");
        let status = head
            .lines()
            .next()
            .and_then(|line| line.split_whitespace().nth(1))
            .and_then(|code| code.parse().ok())
            .expect("status code");
        (status, body.to_owned())
    }

    fn status_json(&self) -> Value {
        let (status, body) = self.get("/__rusty/status");
        assert_eq!(status, 200);
        serde_json::from_str(&body).expect("status json")
    }
}

impl Drop for RunningDev {
    fn drop(&mut self) {
        self.shutdown.shutdown();
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

fn project(label: &str) -> TempDir {
    let project = TempDir::new(label);
    project.write(
        "rusty.toml",
        "[app]\nsource_root = \"src\"\napp_dir = \"src/app\"\n",
    );
    project.write("src/app/page.rs", "pub fn page() {}\n");
    project.write("src/app/api/health/route.rs", "pub fn GET() {}\n");
    project
}

fn wait_for_routes(server: &RunningDev, expected: u64) -> Value {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let status = server.status_json();
        if status["routes"].as_u64() == Some(expected) {
            return status;
        }
        assert!(
            Instant::now() < deadline,
            "timed out waiting for {expected} routes: {status}"
        );
        thread::sleep(Duration::from_millis(100));
    }
}

#[test]
fn serves_status_manifest_and_dashboard() {
    let project = project("serve");
    let server = RunningDev::start(&project.path);

    let status = server.status_json();
    assert_eq!(status["ok"], true);
    assert_eq!(status["routes"], 1);
    assert_eq!(status["apiRoutes"], 1);
    assert_eq!(status["serverFunctions"], 0);
    assert_eq!(status["generation"], 1);

    let (code, body) = server.get("/__rusty/manifest");
    assert_eq!(code, 200);
    let manifest = rusty_compiler::AppManifest::from_json(&body).expect("manifest");
    assert_eq!(manifest.routes().len(), 1);
    assert_eq!(manifest.routes().api_len(), 1);

    let (code, body) = server.get("/");
    assert_eq!(code, 200);
    assert!(body.contains("Rusty dev"));
    assert!(body.contains("/api/health"));

    let (code, _) = server.get("/missing");
    assert_eq!(code, 404);

    let (code, body) = server.get("/__rusty/health");
    assert_eq!(code, 200);
    assert_eq!(body, "ok");

    assert!(project
        .path
        .join("target/rusty/app-manifest.json")
        .is_file());
}

#[test]
fn regenerates_the_manifest_when_files_change() {
    let project = project("watch");
    let server = RunningDev::start(&project.path);
    wait_for_routes(&server, 1);

    thread::sleep(Duration::from_millis(300));
    project.write("src/app/about/page.rs", "pub fn page() {}\n");
    let status = wait_for_routes(&server, 2);
    assert_eq!(status["generation"], 2);
    assert_eq!(status["ok"], true);

    let (code, body) = server.get("/__rusty/manifest");
    assert_eq!(code, 200);
    let manifest = rusty_compiler::AppManifest::from_json(&body).expect("manifest");
    assert_eq!(manifest.routes().len(), 2);
}

#[test]
fn reports_analysis_errors_without_stopping_the_server() {
    let project = project("error");
    project.write("src/broken.rs", "pub fn broken(\n");

    let mut config = RustyConfig::default();
    config.dev.port = 0;
    let server = DevServer::bind(&project.path, config).expect("bind");
    assert!(server.initial_error().is_some());
    let running = {
        let address = server.local_addr().expect("address");
        let shutdown = server.shutdown_handle();
        let thread = thread::spawn(move || server.run());
        RunningDev {
            address,
            shutdown,
            thread: Some(thread),
        }
    };

    let status = running.status_json();
    assert_eq!(status["ok"], false);
    assert!(status["error"].as_str().is_some());

    let (code, body) = running.get("/__rusty/manifest");
    assert_eq!(code, 503);
    assert!(body.contains("error"));

    let (code, body) = running.get("/");
    assert_eq!(code, 200);
    assert!(body.contains("Analysis error"));
}
