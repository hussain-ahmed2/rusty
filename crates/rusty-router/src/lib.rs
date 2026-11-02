//! File-based routing and route manifests for Rusty.
//!
//! [`RouteManifest::discover`] scans a Rusty `src/app` directory and builds a
//! typed routing table. Each page route records its page, nested layout chain,
//! nearest loading/error/not-found boundaries, and URL pattern; `route.rs` files
//! become typed API endpoints.
//!
//! ```no_run
//! use rusty_router::RouteManifest;
//!
//! let manifest = RouteManifest::discover("src/app").expect("discover routes");
//! if let Some(found) = manifest.match_path("/blog/hello") {
//!     println!("page: {}", found.entry().page().display());
//!     println!("slug: {:?}", found.params().get("slug"));
//! }
//! ```
//!
//! The manifest serializes to JSON with [`RouteManifest::to_json_pretty`] and
//! validates when restored with [`RouteManifest::from_json`]. This makes it
//! suitable as a build-time artifact for a later compiler or server runtime.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

mod error;
mod manifest;
mod pattern;

pub use error::RouterError;
pub use manifest::{ApiRoute, ApiRouteMatch, RouteEntry, RouteManifest, RouteMatch};
pub use pattern::{RouteParams, RoutePattern, RouteSegment};
