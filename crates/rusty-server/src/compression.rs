//! Automatic gzip compression for complete responses.

use std::io::Write;

use flate2::write::GzEncoder;
use flate2::Compression as GzipLevel;

use crate::{Headers, Middleware, Response, StatusCode};

/// The minimum body size, in bytes, before compression is attempted.
pub const MIN_COMPRESSED_SIZE: usize = 1024;

/// Returns middleware that gzip-compresses eligible complete responses.
///
/// Compression is negotiated with `Accept-Encoding`, skipped for streaming,
/// already encoded, too small, or non-compressible responses, and adds
/// `Vary: Accept-Encoding`. Strong ETags become weak because the encoded
/// representation differs.
#[must_use]
pub fn compression() -> Middleware {
    std::sync::Arc::new(|context, next| {
        let accepted = accepts_gzip(context.request().headers());
        let mut response = next.run(context)?;
        if accepted {
            compress(&mut response);
        }
        Ok(response)
    })
}

fn compress(response: &mut Response) {
    if response.headers().contains("content-encoding")
        || response
            .headers()
            .get("cache-control")
            .is_some_and(|value| value.split(',').any(|part| part.trim() == "no-transform"))
    {
        return;
    }
    if response.status() == StatusCode::NOT_MODIFIED {
        add_vary(response.headers_mut());
        weaken_etag(response.headers_mut());
        return;
    }
    if !response.is_streaming() && is_compressible(response.headers().get("content-type")) {
        add_vary(response.headers_mut());
    }
    if !eligible(response) {
        return;
    }
    let raw = response.body();
    let Some(compressed) = gzip(raw) else {
        return;
    };
    if compressed.len() >= raw.len() {
        return;
    }

    let old = std::mem::replace(response, Response::new(StatusCode::OK));
    let (status, mut headers, _) = old.into_parts();
    let _ = headers.remove("content-length");
    weaken_etag(&mut headers);
    let _ = headers.insert("content-encoding", "gzip");
    *response = Response::new(status).with_body(compressed);
    *response.headers_mut() = headers;
}

fn eligible(response: &Response) -> bool {
    !response.is_streaming()
        && response.status().as_u16() >= 200
        && response.body().len() >= MIN_COMPRESSED_SIZE
        && response.status() != StatusCode::NO_CONTENT
        && response.status() != StatusCode::NOT_MODIFIED
        && !response.headers().contains("content-encoding")
        && !response
            .headers()
            .get("cache-control")
            .is_some_and(|value| value.split(',').any(|part| part.trim() == "no-transform"))
        && is_compressible(response.headers().get("content-type"))
}

fn is_compressible(content_type: Option<&str>) -> bool {
    let Some(content_type) = content_type else {
        return false;
    };
    let mime = content_type
        .split(';')
        .next()
        .unwrap_or_default()
        .trim()
        .to_ascii_lowercase();
    mime.starts_with("text/")
        || matches!(
            mime.as_str(),
            "application/json"
                | "application/javascript"
                | "application/xml"
                | "image/svg+xml"
                | "application/wasm"
        )
}

fn gzip(bytes: &[u8]) -> Option<Vec<u8>> {
    let mut encoder = GzEncoder::new(Vec::new(), GzipLevel::default());
    encoder.write_all(bytes).ok()?;
    encoder.finish().ok()
}

fn weaken_etag(headers: &mut Headers) {
    if let Some(etag) = headers.get("etag").map(str::to_owned) {
        if etag.starts_with('"') {
            let _ = headers.insert("etag", format!("W/{etag}"));
        }
    }
}

fn add_vary(headers: &mut Headers) {
    let varies = headers.get_all("vary").join(",").to_ascii_lowercase();
    if varies
        .split(',')
        .any(|value| matches!(value.trim(), "*" | "accept-encoding"))
    {
        return;
    }
    let _ = headers.append("vary", "Accept-Encoding");
}

fn accepts_gzip(headers: &Headers) -> bool {
    let mut gzip = None;
    let mut wildcard = None;
    for value in headers.get_all("accept-encoding") {
        for entry in value.split(',') {
            let mut parts = entry.trim().split(';');
            let coding = parts.next().unwrap_or_default().trim();
            let quality = parts
                .find_map(|parameter| {
                    parameter
                        .trim()
                        .strip_prefix("q=")
                        .or_else(|| parameter.trim().strip_prefix("Q="))
                })
                .and_then(|quality| quality.parse::<f32>().ok())
                .unwrap_or(1.0);
            if coding.eq_ignore_ascii_case("gzip") {
                gzip = Some(quality);
            } else if coding == "*" {
                wildcard = Some(quality);
            }
        }
    }
    gzip.or(wildcard).unwrap_or(0.0) > 0.0
}

#[cfg(test)]
mod tests {
    use std::io::Read;

    use flate2::read::GzDecoder;

    use crate::{compression, HttpRouter, Method, Request, Response, StatusCode};

    fn large_text() -> String {
        "Rusty compresses complete responses.\n".repeat(64)
    }

    fn route(router: HttpRouter) -> HttpRouter {
        router
            .get("/", |_| Ok(Response::text(StatusCode::OK, large_text())))
            .expect("text route")
            .get("/stream", |_| {
                Ok(Response::stream_html(
                    StatusCode::OK,
                    vec!["<p>".to_owned(), "stream".to_owned(), "</p>".to_owned()],
                ))
            })
            .expect("stream route")
    }

    fn app() -> HttpRouter {
        route(HttpRouter::new().middleware(compression()))
    }

    fn gunzip(bytes: &[u8]) -> String {
        let mut decoder = GzDecoder::new(bytes);
        let mut output = String::new();
        decoder.read_to_string(&mut output).expect("gunzip");
        output
    }

    #[test]
    fn compresses_when_the_client_accepts_gzip() {
        let response = app()
            .handle(
                Request::new(Method::Get, "/")
                    .with_header("accept-encoding", "br, gzip;q=0.8")
                    .expect("header"),
            )
            .expect("response");

        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(response.headers().get("content-encoding"), Some("gzip"));
        assert!(response.body().len() < large_text().len());
        assert_eq!(gunzip(response.body()), large_text());
        assert!(response
            .headers()
            .get_all("vary")
            .iter()
            .any(|value| value.eq_ignore_ascii_case("Accept-Encoding")));
    }

    #[test]
    fn skips_unaccepted_and_excluded_responses() {
        let plain = app().handle(Request::new(Method::Get, "/")).expect("plain");
        assert_eq!(plain.headers().get("content-encoding"), None);

        let rejected = app()
            .handle(
                Request::new(Method::Get, "/")
                    .with_header("accept-encoding", "gzip;q=0")
                    .expect("header"),
            )
            .expect("rejected");
        assert_eq!(rejected.headers().get("content-encoding"), None);

        let stream = app()
            .handle(
                Request::new(Method::Get, "/stream")
                    .with_header("accept-encoding", "gzip")
                    .expect("header"),
            )
            .expect("stream");
        assert_eq!(stream.headers().get("content-encoding"), None);
    }

    #[test]
    fn skips_small_and_already_encoded_bodies() {
        let small = HttpRouter::new()
            .middleware(compression())
            .get("/", |_| Ok(Response::text(StatusCode::OK, "small")))
            .expect("route")
            .handle(
                Request::new(Method::Get, "/")
                    .with_header("accept-encoding", "gzip")
                    .expect("header"),
            )
            .expect("small");
        assert_eq!(small.headers().get("content-encoding"), None);

        let encoded = HttpRouter::new()
            .middleware(compression())
            .get("/", |_| {
                Ok(Response::text(StatusCode::OK, large_text())
                    .with_header("content-encoding", "br")
                    .expect("header"))
            })
            .expect("route")
            .handle(
                Request::new(Method::Get, "/")
                    .with_header("accept-encoding", "gzip")
                    .expect("header"),
            )
            .expect("encoded");
        assert_eq!(encoded.headers().get("content-encoding"), Some("br"));
    }
}
