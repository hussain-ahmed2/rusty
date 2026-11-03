//! Transport-neutral HTTP runtime for Rusty.
//!
//! `rusty-server` provides typed HTTP methods, status codes, headers, requests,
//! and responses, plus method-aware routing, middleware, bearer
//! authentication, and double-submit-cookie CSRF protection. The optional
//! [`HttpServer`] transport adapter parses HTTP/1.0 and HTTP/1.1 requests over
//! TCP and writes [`Response`] values back to the socket.
//!
//! ```
//! use rusty_server::{HttpRouter, Method, Request, Response, StatusCode};
//!
//! let app = HttpRouter::new()
//!     .get("/hello/[name]", |context| {
//!         let name = context.param("name").unwrap_or("world");
//!         Ok(Response::text(StatusCode::OK, format!("hello {name}")))
//!     })
//!     .expect("valid route");
//!
//! let request = Request::new(Method::Get, "/hello/rusty");
//! let response = app.handle(request).expect("handle");
//! assert_eq!(response.body(), b"hello rusty");
//! ```

#![forbid(unsafe_code)]
#![deny(missing_docs)]

mod auth;
mod context;
mod csrf;
mod error;
mod http;
mod middleware;
mod router;
mod transport;

pub use auth::{authenticate, require_auth, AuthIdentity, Authenticator, BearerAuthenticator};
pub use context::{Extensions, RequestContext};
pub use csrf::{csrf, CsrfMiddleware, CsrfToken, CSRF_COOKIE, CSRF_HEADER};
pub use error::ServerError;
pub use http::{Headers, Method, Request, Response, StatusCode};
pub use middleware::{Handler, Middleware, Next};
pub use router::HttpRouter;
pub use transport::{serve_connection, HttpServer, ServerConfig, ShutdownHandle};
