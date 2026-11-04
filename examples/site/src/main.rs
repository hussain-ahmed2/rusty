//! Static export for the Rusty documentation site.

#![forbid(unsafe_code)]

mod chrome;
mod content;

#[rusty::app]
mod app {}

use std::env;
use std::error::Error;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use rusty::prelude::*;

const DEFAULT_SITE_URL: &str = "https://hussain-ahmed2.github.io";

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
        for file in output_files(output, &path)? {
            write_file(&file, response.body())?;
        }
    }

    let not_found = router.handle(Request::new(Method::Get, "/__rusty_not_found__"))?;
    if not_found.status() != StatusCode::NOT_FOUND {
        return Err(
            io::Error::other(format!("not-found route returned {}", not_found.status())).into(),
        );
    }
    write_file(&output.join("404.html"), not_found.body())?;
    fs::write(output.join(".nojekyll"), [])?;

    let origin = env::var("SITE_URL").unwrap_or_else(|_| DEFAULT_SITE_URL.to_owned());
    write_file(&output.join("sitemap.xml"), sitemap(&origin).as_bytes())?;
    write_file(&output.join("robots.txt"), robots(&origin).as_bytes())?;

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

fn output_files(output: &Path, path: &str) -> io::Result<Vec<PathBuf>> {
    let route = path.trim_matches('/');
    if route.is_empty() {
        return Ok(vec![output.join("index.html")]);
    }
    if route
        .split('/')
        .any(|segment| segment.is_empty() || matches!(segment, "." | ".."))
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("invalid route path `{path}`"),
        ));
    }
    Ok(vec![
        output.join(format!("{route}.html")),
        output.join(route).join("index.html"),
    ])
}

fn public_url(origin: &str, path: &str) -> String {
    let origin = origin.trim_end_matches('/');
    let base = crate::content::base_path();
    if path == "/" {
        format!("{origin}{base}")
    } else {
        format!("{origin}{base}{}", path.trim_start_matches('/'))
    }
}

fn sitemap(origin: &str) -> String {
    let mut xml = String::from(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
         <urlset xmlns=\"http://www.sitemaps.org/schemas/sitemap/0.9\">\n",
    );
    for path in crate::content::paths() {
        xml.push_str(&format!(
            "  <url><loc>{}</loc></url>\n",
            xml_escape(&public_url(origin, &path))
        ));
    }
    xml.push_str("</urlset>\n");
    xml
}

fn robots(origin: &str) -> String {
    format!(
        "User-agent: *\nAllow: /\n\nSitemap: {}\n",
        public_url(origin, "sitemap.xml")
    )
}

fn xml_escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
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
        assert!(home.contains("<title>Rusty — the Rust-native web framework</title>"));
        assert_eq!(home.matches("<title>").count(), 1);
        assert!(home.contains("content=\"Rusty is a Rust-native full-stack framework"));
        assert!(home.contains("__rusty_navigation_data"));
        assert!(home.contains("<meta name=\"rusty-base\" content=\"/\">"));
        assert!(home.contains("<style>"));
        assert!(home.contains("--bg:"));
        assert!(!home.contains("rel=\"stylesheet\""));
        assert!(output.join("docs.html").is_file());
        assert!(output.join("docs/getting-started.html").is_file());
        assert!(output.join("docs/getting-started/index.html").is_file());
        assert!(output.join("about.html").is_file());
        assert!(output.join("404.html").is_file());
        assert!(output.join("sitemap.xml").is_file());
        assert!(output.join("robots.txt").is_file());
        assert!(output.join("styles.css").is_file());
        assert!(output.join(".nojekyll").is_file());

        let sitemap = fs::read_to_string(output.join("sitemap.xml")).expect("sitemap");
        assert!(sitemap.contains("<loc>https://hussain-ahmed2.github.io/docs</loc>"));
        assert!(
            sitemap.contains("<loc>https://hussain-ahmed2.github.io/docs/getting-started</loc>")
        );
        let robots = fs::read_to_string(output.join("robots.txt")).expect("robots");
        assert!(robots.contains("Sitemap: https://hussain-ahmed2.github.io/sitemap.xml"));

        let docs = fs::read_to_string(output.join("docs/routing.html")).expect("docs");
        assert!(docs.contains("File-based routing"));
        assert!(docs.contains("<title>File-based routing — Rusty</title>"));
        assert_eq!(docs.matches("<title>").count(), 1);
        assert!(docs.contains(
            "content=\"Layouts, dynamic segments, loading boundaries, and API routes from the file tree.\""
        ));
        assert!(docs.contains("href=\"/docs/getting-started\""));
        assert!(!docs.contains("href=\"/docs/getting-started/\""));
        assert!(!home.contains("href=\"/docs/\""));
    }

    #[test]
    fn rejects_paths_that_escape_the_output() {
        let temp = TempDir::new("paths");
        assert!(output_files(&temp.path, "/../secret").is_err());
        assert!(output_files(&temp.path, "/docs//page").is_err());
    }
}
