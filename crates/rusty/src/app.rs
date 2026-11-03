//! File-based application runtime.
//!
//! The `#[rusty::app]` macro generates a module tree and calls these types to
//! register page modules, API route modules, layouts, and error boundaries.
//! Static files from the configured public directory and generated WebAssembly
//! client artifacts are served as a fallback after application routes.

use std::env;
use std::error::Error;
use std::fmt;
use std::path::PathBuf;

pub use rusty_config::CLIENT_URL_PREFIX;
use rusty_config::{ConfigError, RustyConfig};
use rusty_core::{Children, Element};
use rusty_rpc::{RpcError, RpcRegistry, ServerInvoker};
use rusty_server::{
    HttpRouter, HttpServer, Method, Request, RequestContext, Response, ServerError, StatusCode,
};
use rusty_ssr::render_to_string;

mod assets;

/// Renders a page for a matched route.
pub type PageHandler = fn(&mut RequestContext) -> Result<Element, ServerError>;

/// Handles an API route method.
pub type ApiHandler = fn(&mut RequestContext) -> Result<Response, ServerError>;

/// Wraps page content in a layout.
pub type Layout = fn(Children) -> Element;

/// Renders an error boundary for a failed page.
pub type ErrorHandler = fn(&ServerError) -> Element;

/// Renders the application not-found page.
pub type NotFoundHandler = fn() -> Element;

#[derive(Clone)]
struct NotFoundRoute {
    handler: NotFoundHandler,
    layouts: Vec<Layout>,
}

/// The API handlers exported by a `route.rs` module.
#[derive(Clone, Copy, Default)]
pub struct ApiMethods {
    /// `GET` handler.
    pub get: Option<ApiHandler>,
    /// `POST` handler.
    pub post: Option<ApiHandler>,
    /// `PUT` handler.
    pub put: Option<ApiHandler>,
    /// `PATCH` handler.
    pub patch: Option<ApiHandler>,
    /// `DELETE` handler.
    pub delete: Option<ApiHandler>,
    /// `OPTIONS` handler.
    pub options: Option<ApiHandler>,
}

impl ApiMethods {
    /// Returns `true` when no method handlers are registered.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.get.is_none()
            && self.post.is_none()
            && self.put.is_none()
            && self.patch.is_none()
            && self.delete.is_none()
            && self.options.is_none()
    }
}

/// A page route and its rendering boundaries.
#[derive(Clone)]
pub struct PageRoute {
    pattern: String,
    page: PageHandler,
    layouts: Vec<Layout>,
    error: Option<ErrorHandler>,
}

impl PageRoute {
    /// Creates a page route without layouts or an error boundary.
    #[must_use]
    pub fn new(pattern: impl Into<String>, page: PageHandler) -> Self {
        Self {
            pattern: pattern.into(),
            page,
            layouts: Vec::new(),
            error: None,
        }
    }

    /// Sets layouts from the root layout to the nearest layout.
    #[must_use]
    pub fn layouts(mut self, layouts: &[Layout]) -> Self {
        self.layouts = layouts.to_vec();
        self
    }

    /// Sets the nearest error boundary.
    #[must_use]
    pub fn error(mut self, handler: ErrorHandler) -> Self {
        self.error = Some(handler);
        self
    }
}

type RpcRegistration = fn(&mut RpcRegistry) -> Result<(), RpcError>;

/// An application route table assembled by `#[rusty::app]`.
#[derive(Clone, Default)]
pub struct Router {
    pages: Vec<PageRoute>,
    apis: Vec<(String, ApiMethods)>,
    rpcs: Vec<RpcRegistration>,
    not_found: Option<NotFoundRoute>,
    public_dir: Option<PathBuf>,
    client_dir: Option<PathBuf>,
    diagnostics: bool,
}

impl Router {
    /// Creates an empty route table.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds a page route.
    #[must_use]
    pub fn page(mut self, route: PageRoute) -> Self {
        self.pages.push(route);
        self
    }

    /// Adds an API route.
    #[must_use]
    pub fn api(mut self, pattern: impl Into<String>, methods: ApiMethods) -> Self {
        self.apis.push((pattern.into(), methods));
        self
    }

    /// Registers a `#[server]` function marker at its declared path.
    #[must_use]
    pub fn rpc<F>(mut self) -> Self
    where
        F: ServerInvoker + Default + Send + Sync + 'static,
    {
        fn register<F>(registry: &mut RpcRegistry) -> Result<(), RpcError>
        where
            F: ServerInvoker + Default + Send + Sync + 'static,
        {
            registry.register::<F>()
        }
        self.rpcs.push(register::<F>);
        self
    }

    /// Sets the application not-found page.
    #[must_use]
    pub fn not_found(self, handler: NotFoundHandler) -> Self {
        self.not_found_with_layouts(handler, &[])
    }

    /// Sets the application not-found page and its layouts.
    #[must_use]
    pub fn not_found_with_layouts(mut self, handler: NotFoundHandler, layouts: &[Layout]) -> Self {
        self.not_found = Some(NotFoundRoute {
            handler,
            layouts: layouts.to_vec(),
        });
        self
    }

    /// Serves files from a public directory at the site root.
    ///
    /// A missing directory disables static serving. Relative paths resolve
    /// against the current working directory when the router is built.
    #[must_use]
    pub fn public_dir(mut self, directory: impl Into<PathBuf>) -> Self {
        self.public_dir = Some(directory.into());
        self
    }

    /// Serves generated WebAssembly client artifacts below
    /// [`CLIENT_URL_PREFIX`].
    ///
    /// A missing directory disables client serving. Relative paths resolve
    /// against the current working directory when the router is built.
    #[must_use]
    pub fn client_dir(mut self, directory: impl Into<PathBuf>) -> Self {
        self.client_dir = Some(directory.into());
        self
    }

    /// Enables or disables `__rusty` diagnostics endpoints.
    #[must_use]
    pub fn diagnostics(mut self, enabled: bool) -> Self {
        self.diagnostics = enabled;
        self
    }

    /// Builds the underlying HTTP router.
    ///
    /// # Errors
    ///
    /// Returns [`ServerError::Route`] when a pattern is invalid, two routes
    /// overlap, or an API route has no method handlers.
    pub fn build(&self) -> Result<HttpRouter, ServerError> {
        let mut router = HttpRouter::new();

        for page in &self.pages {
            let route = page.clone();
            router = router.get(&page.pattern, move |context| {
                Ok(render_page(&route, context))
            })?;
            let route = page.clone();
            router = router.method(Method::Head, &page.pattern, move |context| {
                Ok(render_page(&route, context))
            })?;
        }

        for (pattern, methods) in &self.apis {
            if methods.is_empty() {
                return Err(ServerError::Route(format!(
                    "API route `{pattern}` has no method handlers"
                )));
            }
            router = add_method(router, pattern, Method::Get, methods.get)?;
            router = add_method(router, pattern, Method::Head, methods.get)?;
            router = add_method(router, pattern, Method::Post, methods.post)?;
            router = add_method(router, pattern, Method::Put, methods.put)?;
            router = add_method(router, pattern, Method::Patch, methods.patch)?;
            router = add_method(router, pattern, Method::Delete, methods.delete)?;
            router = add_method(router, pattern, Method::Options, methods.options)?;
        }

        if !self.rpcs.is_empty() {
            let mut registry = RpcRegistry::new();
            for register in &self.rpcs {
                register(&mut registry).map_err(|error| ServerError::Handler(error.to_string()))?;
            }
            let registry = std::sync::Arc::new(registry);
            for path in registry.paths() {
                let registry = std::sync::Arc::clone(&registry);
                router = router.post(path, move |context| registry.handle(context.request()))?;
            }
        }

        if self.diagnostics {
            router = router
                .get("/__rusty/health", |_| {
                    Ok(Response::text(StatusCode::OK, "ok"))
                })?
                .get("/__rusty/status", {
                    let pages = self.pages.len();
                    let apis = self.apis.len();
                    let rpcs = self.rpcs.len();
                    move |_| {
                        Ok(Response::json(
                            StatusCode::OK,
                            format!(
                                "{{\"ok\":true,\"routes\":{pages},\"apiRoutes\":{apis},\"serverFunctions\":{rpcs}}}"
                            ),
                        ))
                    }
                })?;
        }

        let cache_control = if self.diagnostics {
            "no-cache"
        } else {
            "public, max-age=3600"
        };
        let static_files = self
            .public_dir
            .as_deref()
            .and_then(|path| assets::StaticFiles::load(path, cache_control));
        let client_files = self.client_dir.as_deref().and_then(|path| {
            assets::StaticFiles::load_with_prefix(path, CLIENT_URL_PREFIX, cache_control)
        });
        if self.not_found.is_some() || static_files.is_some() || client_files.is_some() {
            let not_found = self.not_found.clone();
            router = router.fallback(move |context| {
                if let Some(response) = client_files
                    .as_ref()
                    .and_then(|client_files| client_files.serve(context.request()))
                {
                    return Ok(response);
                }
                if let Some(response) = static_files
                    .as_ref()
                    .and_then(|static_files| static_files.serve(context.request()))
                {
                    return Ok(response);
                }
                Ok(not_found.as_ref().map_or_else(
                    || Response::text(StatusCode::NOT_FOUND, "not found"),
                    render_not_found,
                ))
            });
        }

        Ok(router)
    }

    /// Handles a request without a network transport.
    ///
    /// # Errors
    ///
    /// Returns route-building or handler errors.
    pub fn handle(&self, request: Request) -> Result<Response, ServerError> {
        self.build()?.handle(request)
    }
}

fn add_method(
    router: HttpRouter,
    pattern: &str,
    method: Method,
    handler: Option<ApiHandler>,
) -> Result<HttpRouter, ServerError> {
    match handler {
        Some(handler) => router.method(method, pattern, handler),
        None => Ok(router),
    }
}

fn render_page(route: &PageRoute, context: &mut RequestContext) -> Response {
    match (route.page)(context) {
        Ok(page) => render_success(page, &route.layouts),
        Err(error) => {
            let page = route
                .error
                .map_or_else(|| default_error(&error), |handler| handler(&error));
            render_failure(page, &route.layouts)
        }
    }
}

fn render_success(page: Element, layouts: &[Layout]) -> Response {
    render_document(page, layouts, StatusCode::OK)
}

fn render_failure(page: Element, layouts: &[Layout]) -> Response {
    render_document(page, layouts, StatusCode::INTERNAL_SERVER_ERROR)
}

fn render_document(page: Element, layouts: &[Layout], status: StatusCode) -> Response {
    let page = apply_layouts(page, layouts);
    let body = if layouts.is_empty() {
        match document(&page) {
            Ok(body) => body,
            Err(error) => return render_error_response(&error),
        }
    } else {
        match render_to_string(&page) {
            Ok(body) => body,
            Err(error) => return render_error_response(&error.to_string()),
        }
    };
    Response::html(status, body)
}

fn apply_layouts(mut page: Element, layouts: &[Layout]) -> Element {
    for layout in layouts.iter().rev() {
        page = layout(Children::new(page));
    }
    page
}

fn document(page: &Element) -> Result<String, String> {
    let body = render_to_string(page).map_err(|error| error.to_string())?;
    Ok(format!(
        "<!doctype html><html><head><meta charset=\"utf-8\"><title>Rusty</title></head><body>{body}</body></html>"
    ))
}

fn render_not_found(route: &NotFoundRoute) -> Response {
    let page = (route.handler)();
    if route.layouts.is_empty() {
        let body = match render_to_string(&page) {
            Ok(body) => body,
            Err(error) => return render_error_response(&error.to_string()),
        };
        Response::html(
            StatusCode::NOT_FOUND,
            format!(
                "<!doctype html><html><head><meta charset=\"utf-8\"><title>Not Found</title></head><body>{body}</body></html>"
            ),
        )
    } else {
        render_document(page, &route.layouts, StatusCode::NOT_FOUND)
    }
}

fn default_error(_error: &ServerError) -> Element {
    Element::text("Internal Server Error")
}

fn render_error_response(_message: &str) -> Response {
    Response::text(StatusCode::INTERNAL_SERVER_ERROR, "Internal Server Error")
}

/// An error from serving an application.
#[derive(Debug)]
#[non_exhaustive]
pub enum ServeError {
    /// Configuration could not be loaded or validated.
    Config(ConfigError),
    /// The HTTP server failed to bind or run.
    Server(ServerError),
    /// A network operation failed while binding or serving.
    Io(std::io::Error),
}

impl fmt::Display for ServeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Config(error) => error.fmt(formatter),
            Self::Server(error) => write!(formatter, "server error: {error}"),
            Self::Io(error) => write!(formatter, "server I/O failed: {error}"),
        }
    }
}

impl Error for ServeError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Config(error) => Some(error),
            Self::Server(error) => Some(error),
            Self::Io(error) => Some(error),
        }
    }
}

impl From<ConfigError> for ServeError {
    fn from(error: ConfigError) -> Self {
        Self::Config(error)
    }
}

impl From<ServerError> for ServeError {
    fn from(error: ServerError) -> Self {
        Self::Server(error)
    }
}

impl From<std::io::Error> for ServeError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error)
    }
}

/// Serves an application using `rusty.toml` from the current directory.
///
/// `RUSTY_DEV=1` enables the `__rusty` diagnostics endpoints.
///
/// # Errors
///
/// Returns a configuration or server error when the application cannot start.
pub fn serve(router: Router) -> Result<(), ServeError> {
    let mut router = router;
    if env::var("RUSTY_DEV").as_deref() == Ok("1") {
        router = router.diagnostics(true);
    }
    let config = config_from_env()?;
    serve_with(router, &config)
}

fn config_from_env() -> Result<RustyConfig, ServeError> {
    let mut config = RustyConfig::load(".")?;
    if let Ok(host) = env::var("RUSTY_HOST") {
        if !host.is_empty() {
            config.dev.host = host;
        }
    }
    if let Ok(port) = env::var("RUSTY_PORT") {
        if !port.is_empty() {
            config.dev.port = port.parse().map_err(|_| ConfigError::Invalid {
                field: "dev.port".to_owned(),
                message: format!("`{port}` is not a valid port"),
            })?;
        }
    }
    config.validate()?;
    Ok(config)
}

/// Serves an application with explicit configuration.
///
/// # Errors
///
/// Returns a server error when the listener cannot bind or the server fails.
pub fn serve_with(router: Router, config: &RustyConfig) -> Result<(), ServeError> {
    config.validate()?;
    let client_dir = config.build.out_dir.join("client");
    let http = router
        .public_dir(&config.app.public_dir)
        .client_dir(client_dir)
        .build()?;
    let server = HttpServer::bind((config.dev.host.as_str(), config.dev.port))?;
    let address = server.local_addr()?;
    println!("Rusty listening on http://{address}");
    server.run_router(http)?;
    Ok(())
}
