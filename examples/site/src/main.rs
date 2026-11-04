//! Static export for the Rusty documentation site.

#![forbid(unsafe_code)]

mod content;

#[rusty::app]
mod app {}

use std::env;
use std::error::Error;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use rusty::prelude::*;

fn main() -> Result<(), Box<dyn Error>> {
    content::set_base(&env::var("RUSTY_BASE_PATH").unwrap_or_default());
    let output =
        env::var_os("SITE_OUT_DIR").map_or_else(|| PathBuf::from("target/site"), PathBuf::from);
    generate(&output)?;
    println!("site written to {}", output.display());
    Ok(())
}

fn generate(output: &Path) -> Result<(), Box<dyn Error>> {
    reset_output(output)?;
    let router = app::router();

    for path in content::paths() {
        let response = router.handle(Request::new(Method::Get, &path))?;
        if response.status() != StatusCode::OK {
            return Err(
                io::Error::other(format!("route `{path}` returned {}", response.status())).into(),
            );
        }
        write_file(&output_file(output, &path)?, response.body())?;
    }

    let not_found = router.handle(Request::new(Method::Get, "/__rusty_not_found__"))?;
    if not_found.status() != StatusCode::NOT_FOUND {
        return Err(
            io::Error::other(format!("not-found route returned {}", not_found.status())).into(),
        );
    }
    write_file(&output.join("404.html"), not_found.body())?;
    fs::write(output.join(".nojekyll"), [])?;

    let public = Path::new(env!("CARGO_MANIFEST_DIR")).join("public");
    copy_directory(&public, output)?;
    Ok(())
}

fn reset_output(output: &Path) -> io::Result<()> {
    if output.as_os_str().is_empty() || output == Path::new("/") {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "refusing to write the site to the filesystem root",
        ));
    }
    if output.exists() {
        fs::remove_dir_all(output)?;
    }
    fs::create_dir_all(output)
}

fn output_file(output: &Path, path: &str) -> io::Result<PathBuf> {
    let route = path.trim_matches('/');
    let mut file = output.to_path_buf();
    if route.is_empty() {
        file.push("index.html");
    } else {
        if route
            .split('/')
            .any(|segment| segment.is_empty() || matches!(segment, "." | ".."))
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("invalid route path `{path}`"),
            ));
        }
        file.push(route);
        file.push("index.html");
    }
    Ok(file)
}

fn write_file(path: &Path, body: &[u8]) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, body)
}

fn copy_directory(source: &Path, destination: &Path) -> io::Result<()> {
    if !source.exists() {
        return Ok(());
    }
    for entry in fs::read_dir(source)? {
        let entry = entry?;
        let file_type = entry.file_type()?;
        if file_type.is_symlink() {
            continue;
        }
        let target = destination.join(entry.file_name());
        if file_type.is_dir() {
            fs::create_dir_all(&target)?;
            copy_directory(&entry.path(), &target)?;
        } else if file_type.is_file() {
            fs::copy(entry.path(), target)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicU64, Ordering};

    use super::*;

    struct TempDir {
        path: PathBuf,
    }

    impl TempDir {
        fn new(label: &str) -> Self {
            static NEXT: AtomicU64 = AtomicU64::new(0);
            let unique = NEXT.fetch_add(1, Ordering::Relaxed);
            let path = env::temp_dir().join(format!(
                "rusty-site-{}-{label}-{unique}",
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
    fn writes_pages_assets_and_a_404() {
        let temp = TempDir::new("export");
        let output = temp.path.join("site");
        generate(&output).expect("generate site");

        let home = fs::read_to_string(output.join("index.html")).expect("home");
        assert!(home.contains("Build fast, typed web apps in Rust."));
        assert!(home.contains("__rusty_navigation_data"));
        assert!(home.contains("<meta name=\"rusty-base\" content=\"/\">"));
        assert!(output.join("docs/getting-started/index.html").is_file());
        assert!(output.join("404.html").is_file());
        assert!(output.join("styles.css").is_file());
        assert!(output.join(".nojekyll").is_file());

        let docs = fs::read_to_string(output.join("docs/routing/index.html")).expect("docs");
        assert!(docs.contains("File-based routing"));
        assert!(docs.contains("docs/getting-started/"));
    }

    #[test]
    fn rejects_paths_that_escape_the_output() {
        let temp = TempDir::new("paths");
        assert!(output_file(&temp.path, "/../secret").is_err());
        assert!(output_file(&temp.path, "/docs//page").is_err());
    }
}
