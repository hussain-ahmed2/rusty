//! External CSS preprocessing.

use std::path::Path;
use std::process::Command;

use rusty_config::RustyConfig;

use crate::CliError;

pub(crate) fn run(project: &Path, config: &RustyConfig) -> Result<bool, CliError> {
    let Some(command) = config.build.css.command.as_deref() else {
        return Ok(false);
    };

    let mut process = shell_command(command);
    process.current_dir(project);
    let status = process
        .status()
        .map_err(|error| CliError::Css(format!("cannot run `{command}`: {error}")))?;
    if !status.success() {
        return Err(CliError::CommandFailed {
            command: format!("css: {command}"),
            status: status.code(),
        });
    }
    println!("Preprocessed CSS with `{command}`.");
    Ok(true)
}

#[cfg(windows)]
fn shell_command(command: &str) -> Command {
    let mut process = Command::new("cmd");
    process.arg("/C").arg(command);
    process
}

#[cfg(not(windows))]
fn shell_command(command: &str) -> Command {
    let mut process = Command::new("sh");
    process.arg("-c").arg(command);
    process
}

#[cfg(test)]
mod tests {
    use super::run;
    use rusty_config::RustyConfig;
    use std::fs;
    use std::path::PathBuf;

    struct TempDir {
        path: PathBuf,
    }

    impl TempDir {
        fn new(label: &str) -> Self {
            let path =
                std::env::temp_dir().join(format!("rusty-css-{}-{label}", std::process::id()));
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
    fn does_nothing_without_a_command() {
        let directory = TempDir::new("disabled");
        assert!(!run(&directory.path, &RustyConfig::default()).expect("run"));
    }

    #[cfg(unix)]
    #[test]
    fn runs_the_configured_command_in_the_project_root() {
        let directory = TempDir::new("success");
        fs::create_dir(directory.path.join("public")).expect("create public");
        let mut config = RustyConfig::default();
        config.build.css.command =
            Some("printf 'body { color: red; }' > public/app.css".to_owned());

        assert!(run(&directory.path, &config).expect("run"));
        let output =
            fs::read_to_string(directory.path.join("public/app.css")).expect("read output");
        assert_eq!(output, "body { color: red; }");
    }

    #[cfg(unix)]
    #[test]
    fn reports_a_failed_command() {
        let directory = TempDir::new("failure");
        let mut config = RustyConfig::default();
        config.build.css.command = Some("exit 3".to_owned());

        let error = run(&directory.path, &config).expect_err("command failure");
        assert!(matches!(
            error,
            crate::CliError::CommandFailed {
                status: Some(3),
                ..
            }
        ));
    }
}
