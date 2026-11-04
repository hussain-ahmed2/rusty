//! Static asset serving from the public directory.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

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
        let etag = format!("\"{}\"", fingerprint(&body));
        let modified = metadata.modified().ok();
        let last_modified = modified.and_then(http_date);
        let not_modified = if !request.headers().get_all("if-none-match").is_empty() {
            request
                .headers()
                .get_all("if-none-match")
                .iter()
                .any(|value| etag_matches(value, &etag))
        } else if let (Some(value), Some(modified)) =
            (request.headers().get("if-modified-since"), modified)
        {
            parse_http_date(value).is_some_and(|since| {
                modified_seconds(modified)
                    .zip(modified_seconds(since))
                    .is_some_and(|(modified, since)| modified <= since)
            })
        } else {
            false
        };

        if not_modified {
            let mut response = Response::new(StatusCode::NOT_MODIFIED);
            let headers = response.headers_mut();
            let _ = headers.insert("cache-control", self.cache_control);
            let _ = headers.insert("x-content-type-options", "nosniff");
            let _ = headers.insert("etag", etag);
            if let Some(last_modified) = last_modified {
                let _ = headers.insert("last-modified", last_modified);
            }
            return Some(response);
        }

        let mut response = Response::new(StatusCode::OK);
        let headers = response.headers_mut();
        let _ = headers.insert("content-type", content_type(&canonical));
        let _ = headers.insert("cache-control", self.cache_control);
        let _ = headers.insert("x-content-type-options", "nosniff");
        let _ = headers.insert("etag", etag);
        if let Some(last_modified) = last_modified {
            let _ = headers.insert("last-modified", last_modified);
        }
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

fn fingerprint(bytes: &[u8]) -> String {
    let mut hash = 0xcbf2_9ce4_8422_2325_u64;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("{hash:016x}")
}

fn etag_matches(header: &str, etag: &str) -> bool {
    header.split(',').map(str::trim).any(|candidate| {
        candidate == "*" || candidate.strip_prefix("W/").unwrap_or(candidate) == etag
    })
}

const WEEKDAYS: [&str; 7] = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];
const MONTHS: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];

fn http_date(time: SystemTime) -> Option<String> {
    let seconds = modified_seconds(time)?;
    let days = i64::try_from(seconds / 86_400).ok()?;
    let remainder = seconds % 86_400;
    let (year, month, day) = civil_from_days(days);
    let weekday = WEEKDAYS[usize::try_from((days + 4).rem_euclid(7)).ok()?];
    let month = MONTHS[usize::try_from(month - 1).ok()?];
    Some(format!(
        "{weekday}, {day:02} {month} {year:04} {:02}:{:02}:{:02} GMT",
        remainder / 3_600,
        (remainder % 3_600) / 60,
        remainder % 60
    ))
}

fn parse_http_date(value: &str) -> Option<SystemTime> {
    let parts = value.split_whitespace().collect::<Vec<_>>();
    let [_, day, month, year, time, "GMT"] = parts.as_slice() else {
        return None;
    };
    let day = day.parse::<u32>().ok()?;
    if !(1..=31).contains(&day) {
        return None;
    }
    let month = month_number(month)?;
    let year = year.parse::<i64>().ok()?;
    let mut time = time.split(':');
    let hour = time.next()?.parse::<u64>().ok()?;
    let minute = time.next()?.parse::<u64>().ok()?;
    let second = time.next()?.parse::<u64>().ok()?;
    if time.next().is_some() || hour > 23 || minute > 59 || second > 59 {
        return None;
    }
    let days = days_from_civil(year, month, day);
    let seconds = days
        .checked_mul(86_400)?
        .checked_add(i64::try_from(hour * 3_600 + minute * 60 + second).ok()?)?;
    let seconds = u64::try_from(seconds).ok()?;
    UNIX_EPOCH.checked_add(Duration::from_secs(seconds))
}

fn modified_seconds(time: SystemTime) -> Option<u64> {
    time.duration_since(UNIX_EPOCH)
        .ok()
        .map(|duration| duration.as_secs())
}

fn month_number(month: &str) -> Option<u32> {
    MONTHS
        .iter()
        .position(|candidate| *candidate == month)
        .and_then(|index| u32::try_from(index + 1).ok())
}

fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let year = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    (
        if month <= 2 { year + 1 } else { year },
        month as u32,
        day as u32,
    )
}

fn days_from_civil(year: i64, month: u32, day: u32) -> i64 {
    let year = if month <= 2 { year - 1 } else { year };
    let era = if year >= 0 { year } else { year - 399 } / 400;
    let yoe = year - era * 400;
    let month = i64::from(month);
    let mp = if month > 2 { month - 3 } else { month + 9 };
    let doy = (153 * mp + 2) / 5 + i64::from(day) - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

#[cfg(test)]
mod tests {
    use super::{
        decode_path, etag_matches, http_date, modified_seconds, parse_http_date, relative_path,
    };
    use std::time::{Duration, UNIX_EPOCH};

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

    #[test]
    fn formats_and_parses_http_dates() {
        let time = UNIX_EPOCH + Duration::from_secs(784_111_777);
        let formatted = http_date(time).expect("format");
        assert_eq!(formatted, "Sun, 06 Nov 1994 08:49:37 GMT");
        let parsed = parse_http_date(&formatted).expect("parse");
        assert_eq!(modified_seconds(parsed), Some(784_111_777));
        assert!(parse_http_date("not a date").is_none());
        assert!(parse_http_date("Sun, 06 Nov 1994 08:49:37 PST").is_none());
    }

    #[test]
    fn matches_strong_weak_and_list_etags() {
        assert!(etag_matches("\"abc\"", "\"abc\""));
        assert!(etag_matches("W/\"abc\"", "\"abc\""));
        assert!(etag_matches("\"other\", \"abc\"", "\"abc\""));
        assert!(etag_matches("*", "\"abc\""));
        assert!(!etag_matches("\"other\"", "\"abc\""));
    }
}
