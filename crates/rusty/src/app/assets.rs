//! Static asset serving from the public directory.

use std::fs;
use std::path::{Path, PathBuf};

use rusty_server::{Method, Request, Response, StatusCode};

#[derive(Clone, Debug)]
pub(crate) struct StaticFiles {
    root: PathBuf,
    prefix: &'static str,
    cache_control: &'static str,
}

impl StaticFiles {
    pub(crate) fn load(path: &Path, cache_control: &'static str) -> Option<Self> {
        Self::load_with_prefix(path, "", cache_control)
    }

    pub(crate) fn load_with_prefix(
        path: &Path,
        prefix: &'static str,
        cache_control: &'static str,
    ) -> Option<Self> {
        let root = fs::canonicalize(path).ok()?;
        if !root.is_dir() {
            return None;
        }
        Some(Self {
            root,
            prefix,
            cache_control,
        })
    }

    pub(crate) fn serve(&self, request: &Request) -> Option<Response> {
        if !matches!(request.method(), Method::Get | Method::Head) {
            return None;
        }
        let path = decode_path(request.path())?;
        let path = if self.prefix.is_empty() {
            path.as_str()
        } else {
            let path = path.strip_prefix(self.prefix)?;
            if !path.starts_with('/') {
                return None;
            }
            path
        };
        let relative = relative_path(path)?;
        let candidate = self.root.join(relative);
        let canonical = fs::canonicalize(candidate).ok()?;
        if !canonical.starts_with(&self.root) {
            return None;
        }
        let metadata = fs::metadata(&canonical).ok()?;
        if !metadata.is_file() {
            return None;
        }
        let body = fs::read(&canonical).ok()?;

        let mut response = Response::new(StatusCode::OK);
        let _ = response
            .headers_mut()
            .insert("content-type", content_type(&canonical));
        let _ = response
            .headers_mut()
            .insert("cache-control", self.cache_control);
        let _ = response
            .headers_mut()
            .insert("x-content-type-options", "nosniff");
        Some(response.with_body(body))
    }
}

fn relative_path(path: &str) -> Option<String> {
    let path = path.trim_start_matches('/');
    if path.is_empty() || path.ends_with('/') {
        return Some(format!("{path}index.html"));
    }
    for segment in path.split('/') {
        if segment.is_empty() || matches!(segment, "." | "..") || segment.contains(['\\', ':']) {
            return None;
        }
    }
    Some(path.to_owned())
}

fn decode_path(path: &str) -> Option<String> {
    if !path.starts_with('/') {
        return None;
    }
    let bytes = path.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        match bytes[index] {
            b'%' => {
                let high = hex(*bytes.get(index + 1)?)?;
                let low = hex(*bytes.get(index + 2)?)?;
                let byte = (high << 4) | low;
                if byte == 0 || byte == b'\\' {
                    return None;
                }
                decoded.push(byte);
                index += 3;
            }
            byte => {
                if byte == b'\\' {
                    return None;
                }
                decoded.push(byte);
                index += 1;
            }
        }
    }
    String::from_utf8(decoded).ok()
}

fn hex(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

fn content_type(path: &Path) -> &'static str {
    match path
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or("")
        .to_ascii_lowercase()
        .as_str()
    {
        "html" | "htm" => "text/html; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "js" | "mjs" => "text/javascript; charset=utf-8",
        "json" | "map" => "application/json; charset=utf-8",
        "xml" => "application/xml; charset=utf-8",
        "txt" => "text/plain; charset=utf-8",
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "avif" => "image/avif",
        "ico" => "image/x-icon",
        "wasm" => "application/wasm",
        "woff" => "font/woff",
        "woff2" => "font/woff2",
        "ttf" => "font/ttf",
        "otf" => "font/otf",
        "pdf" => "application/pdf",
        "mp4" => "video/mp4",
        "webm" => "video/webm",
        "mp3" => "audio/mpeg",
        "wav" => "audio/wav",
        "ogg" => "audio/ogg",
        _ => "application/octet-stream",
    }
}

#[cfg(test)]
mod tests {
    use super::{decode_path, relative_path};

    #[test]
    fn decodes_percent_escapes_and_rejects_traversal() {
        assert_eq!(
            decode_path("/hello%20world.txt").as_deref(),
            Some("/hello world.txt")
        );
        assert_eq!(decode_path("/%2e%2e/secret"), Some("/../secret".to_owned()));
        assert_eq!(decode_path("/%00"), None);
        assert_eq!(decode_path("/a%5cb"), None);
        assert_eq!(decode_path("/a%zz"), None);
    }

    #[test]
    fn builds_safe_relative_paths() {
        assert_eq!(relative_path("/").as_deref(), Some("index.html"));
        assert_eq!(relative_path("/blog/").as_deref(), Some("blog/index.html"));
        assert_eq!(relative_path("/a//b"), None);
        assert_eq!(relative_path("/../secret"), None);
        assert_eq!(relative_path("/a/./b"), None);
        assert_eq!(relative_path("/a:b"), None);
    }
}
