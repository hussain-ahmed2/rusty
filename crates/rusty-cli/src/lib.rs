//! Command-line tooling for Rusty applications.
//!
//! The CLI deliberately keeps its parser small and dependency-free:
//!
//! ```text
//! rusty new <path>
//! rusty check [--project <path>]
//! rusty generate [--project <path>]
//! rusty dev [--project <path>] [--host <host>] [--port <port>]
//! ```
//!
//! `check` analyzes routes and server functions without writing files.
//! `generate` writes the analyzed application manifest under the configured
//! output directory. `dev` serves live manifest diagnostics and regenerates the
//! manifest as source files change.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

use std::error::Error;
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};

use rusty_compiler::{AppCompiler, CompileError};
use rusty_config::{ConfigError, RustyConfig};
use rusty_server::ServerError;

mod dev;

pub use dev::DevServer;

/// The top-level help text shown by `rusty --help`.
pub const HELP: &str = "\
rusty - build and inspect Rusty applications

Usage:
  rusty new <path>                  Create a new Rusty application
  rusty check [--project <path>]    Analyze routes and server functions
  rusty generate [--project <path>] Write the application manifest
  rusty dev [--project <path>] [--host <host>] [--port <port>]
                                    Serve live manifest diagnostics
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
    /// Serves live manifest diagnostics and watches for changes.
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
        "dev" => parse_dev(&arguments[1..]),
        other => Err(CliError::Usage(format!("unknown command `{other}`"))),
    }
}

fn parse_new(arguments: &[String]) -> Result<Command, CliError> {
    if arguments.iter().any(|argument| is_help(argument)) {
        return Ok(Command::Help);
    }

    let path = match arguments {
        [path] if !path.starts_with('-') => PathBuf::from(path),
        [path] => {
            return Err(CliError::Usage(format!(
                "invalid new-project path `{path}`"
            )))
        }
        [] => {
            return Err(CliError::Usage(
                "new requires a destination path".to_owned(),
            ))
        }
        _ => {
            return Err(CliError::Usage(
                "new accepts exactly one destination path".to_owned(),
            ))
        }
    };
    Ok(Command::New { path })
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
        Command::New { path } => create_project(&path),
        Command::Check { project } => {
            let compiler = AppCompiler::from_project(&project)?;
            let manifest = compiler.analyze()?;
            Ok(Report::new(format!(
                "Checked {} page route(s), {} API route(s), and {} server function(s) in {}.",
                manifest.routes().len(),
                manifest.routes().api_len(),
                manifest.server_functions().len(),
                compiler.project_root().display()
            )))
        }
        Command::Generate { project } => {
            let compiler = AppCompiler::from_project(&project)?;
            let manifest = compiler.analyze()?;
            compiler.write_manifest(&manifest)?;
            Ok(Report::new(format!(
                "Wrote {} with {} page route(s), {} API route(s), and {} server function(s).",
                compiler.manifest_path().display(),
                manifest.routes().len(),
                manifest.routes().api_len(),
                manifest.server_functions().len()
            )))
        }
        Command::Dev {
            project,
            host,
            port,
        } => {
            let mut config = RustyConfig::load(&project)?;
            if let Some(host) = host {
                config.dev.host = host;
            }
            if let Some(port) = port {
                config.dev.port = port;
            }
            config.validate()?;
            let server = DevServer::bind(&project, config)?;
            let address = server.local_addr()?;
            println!("Rusty dev server listening on http://{address}");
            if let Some(error) = server.initial_error() {
                eprintln!("analysis error: {error}");
            }
            server.run()?;
            Ok(Report::new("Rusty dev server stopped."))
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

fn create_project(path: &Path) -> Result<Report, CliError> {
    let name = crate_name(path)?;
    prepare_destination(path)?;

    write_file(
        &path.join("Cargo.toml"),
        &format!(
            "[package]\nname = \"{name}\"\nversion = \"0.1.0\"\nedition = \"2021\"\nrust-version = \"1.75\"\n\n[dependencies]\nrusty = {}\n\n[workspace]\n",
            facade_dependency()
        ),
    )?;
    write_file(
        &path.join("rusty.toml"),
        "[app]\nsource_root = \"src\"\napp_dir = \"src/app\"\n\n[build]\nout_dir = \"target/rusty\"\n\n[dev]\nhost = \"127.0.0.1\"\nport = 3000\n",
    )?;
    write_file(
        &path.join("src/main.rs"),
        &format!(
            "//! {name}\n\n#![forbid(unsafe_code)]\n\nfn main() {{\n    println!(\"Hello from {name}!\");\n}}\n"
        ),
    )?;
    write_file(
        &path.join("src/app/page.rs"),
        "//! Home page.\n\n/// Renders the home page.\npub fn page() -> &'static str {\n    \"Hello from Rusty!\"\n}\n",
    )?;
    write_file(
        &path.join("src/app/api/health/route.rs"),
        "//! Health check endpoint.\n\n/// Returns the current health status.\npub fn GET() -> &'static str {\n    \"ok\"\n}\n",
    )?;
    write_file(&path.join(".gitignore"), "/target\n")?;

    Ok(Report::new(format!(
        "Created Rusty application at {}.",
        path.display()
    )))
}

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

fn io_error(path: &Path, error: &std::io::Error) -> CliError {
    CliError::Io {
        path: path.to_path_buf(),
        message: error.to_string(),
    }
}
