//! Command-line tooling for Rusty applications.
//!
//! The CLI deliberately keeps its parser small and dependency-free:
//!
//! ```text
//! rusty new <path> [--client]
//! rusty check [--project <path>]
//! rusty generate [--project <path>]
//! rusty build [--project <path>] [--release]
//! rusty start [--project <path>] [--release]
//! rusty dev [--project <path>] [--host <host>] [--port <port>]
//! ```
//!
//! `check` analyzes routes and server functions without writing files.
//! `generate` writes the analyzed application manifest under the configured
//! output directory. `build` compiles the application, any configured
//! WebAssembly client, and a standalone deployment bundle. `start` runs the
//! application, and `dev` rebuilds and restarts it when source files change.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

use std::error::Error;
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command as ProcessCommand;

use rusty_compiler::{AppCompiler, CompileError};
use rusty_config::{ConfigError, RustyConfig};
use rusty_server::ServerError;

mod bundle;
mod cargo;
mod dev;
mod wasm;

pub use dev::{DevRunner, DevServer};

/// The top-level help text shown by `rusty --help`.
pub const HELP: &str = "\
rusty - build and inspect Rusty applications

Usage:
  rusty new <path> [--client]       Create a new Rusty application
  rusty check [--project <path>]    Analyze routes and server functions
  rusty generate [--project <path>] Write the application manifest
  rusty build [--project <path>] [--release]
                                    Build native, client, and deployment artifacts
  rusty start [--project <path>] [--release]
                                    Run the compiled application
  rusty dev [--project <path>] [--host <host>] [--port <port>]
                                    Run with rebuild-on-change
  rusty help                        Show this help
  rusty --version                   Show the CLI version

Run `rusty <command> --help` for command help.
";

const VERSION: &str = env!("CARGO_PKG_VERSION");

/// A parsed CLI command.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    /// Prints help.
    Help,
    /// Prints the CLI version.
    Version,
    /// Creates a new application at the given path.
    New {
        /// Destination directory.
        path: PathBuf,
        /// Whether to scaffold a WebAssembly client package.
        client: bool,
    },
    /// Analyzes an application without writing files.
    Check {
        /// Project root.
        project: PathBuf,
    },
    /// Analyzes an application and writes its manifest.
    Generate {
        /// Project root.
        project: PathBuf,
    },
    /// Compiles the application with Cargo.
    Build {
        /// Project root.
        project: PathBuf,
        /// Whether to compile with optimizations.
        release: bool,
    },
    /// Runs the application with Cargo.
    Start {
        /// Project root.
        project: PathBuf,
        /// Whether to run the release binary.
        release: bool,
    },
    /// Runs the application and restarts it when source files change.
    Dev {
        /// Project root.
        project: PathBuf,
        /// Optional host override.
        host: Option<String>,
        /// Optional port override. Zero selects an ephemeral port.
        port: Option<u16>,
    },
}

/// A successful command outcome.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Report {
    message: String,
}

impl Report {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }

    /// Returns the human-readable outcome message.
    #[must_use]
    pub fn message(&self) -> &str {
        &self.message
    }
}

impl fmt::Display for Report {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

/// An error from parsing or executing a CLI command.
#[derive(Debug)]
#[non_exhaustive]
pub enum CliError {
    /// The command line is invalid.
    Usage(String),
    /// A filesystem operation failed.
    Io {
        /// The path involved in the failure.
        path: PathBuf,
        /// The operating-system error message.
        message: String,
    },
    /// The destination already exists and is not empty.
    ExistingProject(PathBuf),
    /// The destination project name cannot be used as a crate name.
    InvalidProjectName {
        /// The destination path.
        path: PathBuf,
        /// The offending name.
        name: String,
    },
    /// Configuration loading failed.
    Config(ConfigError),
    /// Application analysis failed.
    Compile(CompileError),
    /// The development server failed.
    Server(String),
    /// The WebAssembly client build failed.
    Client(String),
    /// The deployment bundle build failed.
    Bundle(String),
    /// A child command exited unsuccessfully.
    CommandFailed {
        /// The command that failed.
        command: String,
        /// The process exit status, when available.
        status: Option<i32>,
    },
}

impl CliError {
    /// Returns `true` for command-line usage errors.
    #[must_use]
    pub fn is_usage(&self) -> bool {
        matches!(self, Self::Usage(_))
    }
}

impl fmt::Display for CliError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Usage(message) => write!(formatter, "{message}\n\n{HELP}"),
            Self::Io { path, message } => {
                write!(formatter, "I/O failed for {}: {message}", path.display())
            }
            Self::ExistingProject(path) => write!(
                formatter,
                "refusing to create a project in {} because the directory is not empty",
                path.display()
            ),
            Self::InvalidProjectName { path, name } => write!(
                formatter,
                "cannot derive a Rust crate name from `{name}` at {}",
                path.display()
            ),
            Self::Config(error) => error.fmt(formatter),
            Self::Compile(error) => error.fmt(formatter),
            Self::Server(message) => write!(formatter, "server error: {message}"),
            Self::Client(message) => write!(formatter, "client build failed: {message}"),
            Self::Bundle(message) => write!(formatter, "deployment bundle failed: {message}"),
            Self::CommandFailed { command, status } => match status {
                Some(status) => write!(formatter, "`{command}` failed with status {status}"),
                None => write!(formatter, "`{command}` failed"),
            },
        }
    }
}

impl Error for CliError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Config(error) => Some(error),
            Self::Compile(error) => Some(error),
            _ => None,
        }
    }
}

impl From<ConfigError> for CliError {
    fn from(error: ConfigError) -> Self {
        Self::Config(error)
    }
}

impl From<CompileError> for CliError {
    fn from(error: CompileError) -> Self {
        Self::Compile(error)
    }
}

impl From<ServerError> for CliError {
    fn from(error: ServerError) -> Self {
        Self::Server(error.to_string())
    }
}

/// Parses command-line arguments, excluding the program name.
///
/// # Errors
///
/// Returns [`CliError::Usage`] when the command, options, or arguments are
/// invalid.
pub fn parse<I, S>(arguments: I) -> Result<Command, CliError>
where
    I: IntoIterator<Item = S>,
    S: Into<String>,
{
    let arguments = arguments.into_iter().map(Into::into).collect::<Vec<_>>();
    let Some(command) = arguments.first().map(String::as_str) else {
        return Ok(Command::Help);
    };

    match command {
        "help" | "-h" | "--help" => Ok(Command::Help),
        "version" | "-V" | "--version" => Ok(Command::Version),
        "new" => parse_new(&arguments[1..]),
        "check" => parse_project(&arguments[1..], false),
        "generate" => parse_project(&arguments[1..], true),
        "build" => parse_build(&arguments[1..], false),
        "start" => parse_build(&arguments[1..], true),
        "dev" => parse_dev(&arguments[1..]),
        other => Err(CliError::Usage(format!("unknown command `{other}`"))),
    }
}

fn parse_new(arguments: &[String]) -> Result<Command, CliError> {
    if arguments.iter().any(|argument| is_help(argument)) {
        return Ok(Command::Help);
    }

    let mut path = None;
    let mut client = false;
    for argument in arguments {
        if argument == "--client" {
            client = true;
        } else if argument.starts_with('-') {
            return Err(CliError::Usage(format!("unexpected argument `{argument}`")));
        } else if path.replace(PathBuf::from(argument)).is_some() {
            return Err(CliError::Usage(
                "new accepts exactly one destination path".to_owned(),
            ));
        }
    }
    let path = path.ok_or_else(|| CliError::Usage("new requires a destination path".to_owned()))?;
    Ok(Command::New { path, client })
}

fn parse_project(arguments: &[String], generate: bool) -> Result<Command, CliError> {
    if arguments.iter().any(|argument| is_help(argument)) {
        return Ok(Command::Help);
    }

    let mut project = None;
    let mut index = 0;
    while index < arguments.len() {
        let argument = &arguments[index];
        if argument == "--project" {
            let Some(value) = arguments.get(index + 1) else {
                return Err(CliError::Usage(
                    "--project requires a directory path".to_owned(),
                ));
            };
            if value.starts_with('-') {
                return Err(CliError::Usage(
                    "--project requires a directory path".to_owned(),
                ));
            }
            project = Some(PathBuf::from(value));
            index += 2;
        } else if let Some(value) = argument.strip_prefix("--project=") {
            if value.is_empty() {
                return Err(CliError::Usage(
                    "--project requires a directory path".to_owned(),
                ));
            }
            project = Some(PathBuf::from(value));
            index += 1;
        } else {
            return Err(CliError::Usage(format!("unexpected argument `{argument}`")));
        }
    }

    let project = project.unwrap_or_else(|| PathBuf::from("."));
    Ok(if generate {
        Command::Generate { project }
    } else {
        Command::Check { project }
    })
}

fn parse_build(arguments: &[String], start: bool) -> Result<Command, CliError> {
    if arguments.iter().any(|argument| is_help(argument)) {
        return Ok(Command::Help);
    }

    let mut project = None;
    let mut release = false;
    let mut index = 0;
    while index < arguments.len() {
        let argument = &arguments[index];
        if argument == "--release" {
            release = true;
            index += 1;
            continue;
        }
        if argument == "--project" {
            let Some(value) = arguments.get(index + 1) else {
                return Err(CliError::Usage(
                    "--project requires a directory path".to_owned(),
                ));
            };
            if value.starts_with('-') {
                return Err(CliError::Usage(
                    "--project requires a directory path".to_owned(),
                ));
            }
            project = Some(PathBuf::from(value));
            index += 2;
            continue;
        }
        if let Some(value) = argument.strip_prefix("--project=") {
            if value.is_empty() {
                return Err(CliError::Usage(
                    "--project requires a directory path".to_owned(),
                ));
            }
            project = Some(PathBuf::from(value));
            index += 1;
            continue;
        }
        return Err(CliError::Usage(format!("unexpected argument `{argument}`")));
    }

    let project = project.unwrap_or_else(|| PathBuf::from("."));
    Ok(if start {
        Command::Start { project, release }
    } else {
        Command::Build { project, release }
    })
}

fn parse_dev(arguments: &[String]) -> Result<Command, CliError> {
    if arguments.iter().any(|argument| is_help(argument)) {
        return Ok(Command::Help);
    }

    let mut project = None;
    let mut host = None;
    let mut port = None;
    let mut index = 0;
    while index < arguments.len() {
        let argument = &arguments[index];
        if let Some((name, value)) = split_option(argument) {
            if value.is_empty() {
                return Err(CliError::Usage(format!("{name} requires a value")));
            }
            match name {
                "--project" => project = Some(PathBuf::from(value)),
                "--host" => host = Some(value.to_owned()),
                "--port" => {
                    port = Some(
                        value
                            .parse::<u16>()
                            .map_err(|_| CliError::Usage(format!("invalid port `{value}`")))?,
                    )
                }
                _ => return Err(CliError::Usage(format!("unexpected argument `{argument}`"))),
            }
            index += 1;
            continue;
        }

        if matches!(argument.as_str(), "--project" | "--host" | "--port") {
            let Some(value) = arguments.get(index + 1) else {
                return Err(CliError::Usage(format!("{argument} requires a value")));
            };
            if value.starts_with('-') {
                return Err(CliError::Usage(format!("{argument} requires a value")));
            }
            match argument.as_str() {
                "--project" => project = Some(PathBuf::from(value)),
                "--host" => host = Some(value.clone()),
                "--port" => {
                    port = Some(
                        value
                            .parse::<u16>()
                            .map_err(|_| CliError::Usage(format!("invalid port `{value}`")))?,
                    )
                }
                _ => unreachable!(),
            }
            index += 2;
        } else {
            return Err(CliError::Usage(format!("unexpected argument `{argument}`")));
        }
    }

    Ok(Command::Dev {
        project: project.unwrap_or_else(|| PathBuf::from(".")),
        host,
        port,
    })
}

fn split_option(argument: &str) -> Option<(&str, &str)> {
    let (name, value) = argument.split_once('=')?;
    matches!(name, "--project" | "--host" | "--port").then_some((name, value))
}

fn is_help(argument: &str) -> bool {
    matches!(argument, "-h" | "--help" | "help")
}

/// Executes a parsed command.
///
/// # Errors
///
/// Returns [`CliError::Io`] when scaffolding files cannot be written,
/// [`CliError::ExistingProject`] when a destination is not empty, or a
/// configuration/compiler error when analysis fails.
pub fn execute(command: Command) -> Result<Report, CliError> {
    match command {
        Command::Help => Ok(Report::new(HELP)),
        Command::Version => Ok(Report::new(format!("rusty {VERSION}"))),
        Command::New { path, client } => create_project(&path, client),
        Command::Check { project } => {
            let compiler = AppCompiler::from_project(&project)?;
            let manifest = compiler.analyze()?;
            Ok(Report::new(format!(
                "Checked {} page route(s), {} API route(s), {} server function(s), and {} asset(s) in {}.",
                manifest.routes().len(),
                manifest.routes().api_len(),
                manifest.server_functions().len(),
                manifest.assets().len(),
                compiler.project_root().display()
            )))
        }
        Command::Generate { project } => {
            let compiler = AppCompiler::from_project(&project)?;
            let manifest = compiler.analyze()?;
            compiler.write_manifest(&manifest)?;
            Ok(Report::new(format!(
                "Wrote {} with {} page route(s), {} API route(s), {} server function(s), and {} asset(s).",
                compiler.manifest_path().display(),
                manifest.routes().len(),
                manifest.routes().api_len(),
                manifest.server_functions().len(),
                manifest.assets().len()
            )))
        }
        Command::Build { project, release } => {
            let compiler = AppCompiler::from_project(&project)?;
            let config = compiler.config().clone();
            let manifest = compiler.analyze()?;
            compiler.write_manifest(&manifest)?;
            run_cargo(&project, "build", release)?;
            let client = wasm::build(&project, &config, release)?;
            let bundle = bundle::build(&project, &config, release)?;
            let mut message = format!(
                "Built Rusty application in {} and wrote {}.",
                project.display(),
                compiler.manifest_path().display()
            );
            if let Some(client) = client {
                message.push_str(&format!(
                    " Built client {} with {} artifact(s).",
                    client.entrypoint, client.files
                ));
            }
            message.push_str(&format!(
                " Wrote deployment bundle to {}.",
                bundle.path.display()
            ));
            Ok(Report::new(message))
        }
        Command::Start { project, release } => {
            let config = RustyConfig::load(&project)?;
            config.validate()?;
            run_cargo(&project, "run", release)?;
            Ok(Report::new("Rusty application stopped."))
        }
        Command::Dev {
            project,
            host,
            port,
        } => {
            let mut config = RustyConfig::load(&project)?;
            if let Some(host) = host.clone() {
                config.dev.host = host;
            }
            if let Some(port) = port {
                config.dev.port = port;
            }
            config.validate()?;
            DevRunner::new(project, host, port).run()?;
            Ok(Report::new("Rusty dev stopped."))
        }
    }
}

/// Parses and executes command-line arguments.
///
/// # Errors
///
/// Returns any error from [`parse`] or [`execute`].
pub fn run<I, S>(arguments: I) -> Result<Report, CliError>
where
    I: IntoIterator<Item = S>,
    S: Into<String>,
{
    execute(parse(arguments)?)
}

fn run_cargo(project: &Path, command: &str, release: bool) -> Result<(), CliError> {
    let mut process = ProcessCommand::new("cargo");
    process.arg(command).arg("--quiet");
    if release {
        process.arg("--release");
    }
    process.current_dir(project);
    let status = process.status().map_err(|error| CliError::Io {
        path: PathBuf::from("cargo"),
        message: error.to_string(),
    })?;
    if status.success() {
        Ok(())
    } else {
        Err(CliError::CommandFailed {
            command: format!("cargo {command}"),
            status: status.code(),
        })
    }
}

fn create_project(path: &Path, client: bool) -> Result<Report, CliError> {
    let name = crate_name(path)?;
    let client_target = format!("{}_client", name.replace('-', "_"));
    prepare_destination(path)?;

    write_file(
        &path.join("Cargo.toml"),
        &format!(
            "[package]\nname = \"{name}\"\nversion = \"0.1.0\"\nedition = \"2021\"\nrust-version = \"1.75\"\nbuild = \"build.rs\"\n\n[dependencies]\nrusty = {}\n\n[workspace]\n",
            facade_dependency()
        ),
    )?;
    let client_dir = if client {
        "client_dir = \"client\"\n"
    } else {
        ""
    };
    write_file(
        &path.join("rusty.toml"),
        &format!(
            "[app]\nsource_root = \"src\"\napp_dir = \"src/app\"\npublic_dir = \"public\"\n\n[build]\nout_dir = \"target/rusty\"\n{client_dir}\n[dev]\nhost = \"127.0.0.1\"\nport = 3000\n"
        ),
    )?;
    write_file(&path.join("build.rs"), BUILD_SCRIPT)?;
    write_file(
        &path.join("src/main.rs"),
        &format!(
            "//! {name}\n\n#![forbid(unsafe_code)]\n\n#[rusty::app]\nmod app {{}}\n\nfn main() -> Result<(), rusty::app::ServeError> {{\n    app::serve()\n}}\n"
        ),
    )?;
    if client {
        let entrypoint = format!("{client_target}.js");
        let layout = ROOT_LAYOUT_CLIENT.replace("__CLIENT_ENTRYPOINT__", &entrypoint);
        write_file(&path.join("src/app/layout.rs"), &layout)?;
        write_file(
            &path.join("client/Cargo.toml"),
            &format!(
                "[package]\nname = \"{name}-client\"\nversion = \"0.1.0\"\nedition = \"2021\"\nrust-version = \"1.75\"\n\n[workspace]\n\n[lib]\nname = \"{client_target}\"\ncrate-type = [\"cdylib\", \"rlib\"]\n\n[dependencies]\nrusty = {}\nwasm-bindgen = \"0.2\"\n",
                facade_dependency()
            ),
        )?;
        write_file(&path.join("client/src/lib.rs"), CLIENT_LIB)?;
        write_file(&path.join(".gitignore"), "/target\n/client/target\n")?;
    } else {
        write_file(&path.join("src/app/layout.rs"), ROOT_LAYOUT)?;
        write_file(&path.join(".gitignore"), "/target\n")?;
    }
    write_file(&path.join("src/app/page.rs"), HOME_PAGE)?;
    write_file(&path.join("src/app/not_found.rs"), NOT_FOUND_PAGE)?;
    write_file(&path.join("src/app/api/health/route.rs"), HEALTH_ROUTE)?;
    write_file(&path.join("public/robots.txt"), PUBLIC_ROBOTS)?;

    Ok(Report::new(format!(
        "Created Rusty application{} at {}.",
        if client {
            " with a WebAssembly client"
        } else {
            ""
        },
        path.display()
    )))
}

const BUILD_SCRIPT: &str = r#"//! Tracks file-based routes so Cargo re-expands `#[rusty::app]` after changes.

use std::fs;
use std::path::Path;

fn main() {
    println!("cargo:rerun-if-changed=rusty.toml");
    visit(Path::new("src"));
}

fn visit(path: &Path) {
    println!("cargo:rerun-if-changed={}", path.display());
    let Ok(entries) = fs::read_dir(path) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            visit(&path);
        }
    }
}
"#;

const ROOT_LAYOUT: &str = r#"//! Root layout.

use rusty::prelude::*;

/// Wraps every page in the application document.
pub fn layout(children: Children) -> Element {
    Element::fragment([
        Element::raw_html("<!doctype html>"),
        view! {
            <html>
                <head><title>"Rusty app"</title></head>
                <body>{children}</body>
            </html>
        },
    ])
}
"#;

const ROOT_LAYOUT_CLIENT: &str = r#"//! Root layout.

use rusty::prelude::*;

/// Wraps every page in the application document.
pub fn layout(children: Children) -> Element {
    Element::fragment([
        Element::raw_html("<!doctype html>"),
        view! {
            <html>
                <head>
                    <title>"Rusty app"</title>
                    {Element::raw_html("<script type=\"module\">import init from \"/_rusty/client/__CLIENT_ENTRYPOINT__\"; init();</script>")}
                </head>
                <body>{children}</body>
            </html>
        },
    ])
}
"#;

const CLIENT_LIB: &str = r#"//! Browser client bootstrap.

#![forbid(unsafe_code)]

use rusty::prelude::*;

fn counter(count: Signal<u32>) -> Element {
    let handler_count = count.clone();

    view! {
        <aside>
            <h2>"Rusty client"</h2>
            <button on:click={move |_| {
                handler_count.set(handler_count.get() + 1);
            }}>"Increment"</button>
            <p>{count}</p>
        </aside>
    }
}

/// Starts the browser client.
#[cfg(target_arch = "wasm32")]
#[wasm_bindgen::prelude::wasm_bindgen(start)]
pub fn start() {
    let count = Signal::new(0_u32);
    let root = rusty::client::mount_to_body(move || counter(count.clone()))
        .expect("failed to mount the Rusty client");
    std::mem::forget(root);
}
"#;

const HOME_PAGE: &str = r#"//! Home page.

use rusty::prelude::*;

/// Renders the home page.
pub fn page(_context: &mut RequestContext) -> Result<Element, ServerError> {
    Ok(view! {
        <main>
            <h1>"Hello from Rusty!"</h1>
        </main>
    })
}
"#;

const NOT_FOUND_PAGE: &str = r#"//! Application not-found page.

use rusty::prelude::*;

/// Renders the not-found page.
pub fn not_found() -> Element {
    view! {
        <main>
            <h1>"Not found"</h1>
            <p><a href="/">"Back home"</a></p>
        </main>
    }
}
"#;

const PUBLIC_ROBOTS: &str = "User-agent: *\nDisallow:\n";

const HEALTH_ROUTE: &str = r##"//! Health check endpoint.

use rusty::prelude::*;

/// Returns the current health status.
pub fn get(_context: &mut RequestContext) -> Result<Response, ServerError> {
    Ok(Response::json(
        StatusCode::OK,
        r#"{"status":"ok"}"#.to_owned(),
    ))
}
"##;

fn prepare_destination(path: &Path) -> Result<(), CliError> {
    match fs::metadata(path) {
        Ok(metadata) if !metadata.is_dir() => Err(CliError::ExistingProject(path.to_path_buf())),
        Ok(_) => {
            let mut entries = fs::read_dir(path).map_err(|error| io_error(path, &error))?;
            if entries.next().is_some() {
                return Err(CliError::ExistingProject(path.to_path_buf()));
            }
            Ok(())
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            fs::create_dir_all(path).map_err(|error| io_error(path, &error))
        }
        Err(error) => Err(io_error(path, &error)),
    }
}

fn crate_name(path: &Path) -> Result<String, CliError> {
    let raw = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| CliError::InvalidProjectName {
            path: path.to_path_buf(),
            name: path.display().to_string(),
        })?;
    let mut name = raw
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || character == '-' || character == '_' {
                character
            } else {
                '-'
            }
        })
        .collect::<String>()
        .trim_matches('-')
        .to_owned();
    if name.is_empty() {
        return Err(CliError::InvalidProjectName {
            path: path.to_path_buf(),
            name: raw.to_owned(),
        });
    }
    if name.starts_with(|character: char| character.is_ascii_digit()) {
        name.insert_str(0, "app-");
    }
    Ok(name)
}

fn facade_dependency() -> String {
    let local = Path::new(env!("CARGO_MANIFEST_DIR")).join("../rusty");
    if local.join("Cargo.toml").is_file() {
        let path = local
            .canonicalize()
            .unwrap_or(local)
            .display()
            .to_string()
            .replace('\\', "\\\\")
            .replace('"', "\\\"");
        format!("{{ path = \"{path}\" }}")
    } else {
        format!("\"{VERSION}\"")
    }
}

fn write_file(path: &Path, contents: &str) -> Result<(), CliError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| io_error(parent, &error))?;
    }
    fs::write(path, contents).map_err(|error| io_error(path, &error))
}

pub(crate) fn io_error(path: &Path, error: &std::io::Error) -> CliError {
    CliError::Io {
        path: path.to_path_buf(),
        message: error.to_string(),
    }
}
