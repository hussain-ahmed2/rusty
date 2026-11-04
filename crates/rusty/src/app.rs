//! File-based application runtime.
//!
//! The `#[rusty::app]` macro generates a module tree and calls these types to
//! register page modules, API route modules, layouts, loading boundaries, and
//! error boundaries. Pages with loading boundaries enable same-origin client
//! navigation by default. Static files from the configured public directory and
//! generated WebAssembly client artifacts are served as a fallback after
//! application routes.

use std::env;
use std::error::Error;
use std::fmt;
use std::future::Future;
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::Arc;

pub use rusty_config::CLIENT_URL_PREFIX;
use rusty_config::{ConfigError, RustyConfig};
use rusty_core::{Children, Element};
use rusty_rpc::{RpcError, RpcRegistry, ServerInvoker};
use rusty_server::{
    block_on, HttpRouter, HttpServer, Method, Request, RequestContext, Response, ServerError,
    StatusCode,
};
use rusty_ssr::render_to_chunks;

mod assets;
pub use metadata::Metadata;
mod metadata;
mod navigation;

/// Renders a page for a matched route.
pub type PageHandler = fn(&mut RequestContext) -> Result<Element, ServerError>;

/// Runs before application routes and static files.
///
/// Returning `Some(response)` short-circuits the request; returning `None`
/// continues through the route table and static asset fallbacks.
pub type MiddlewareHandler =
    Arc<dyn Fn(&mut RequestContext) -> Result<Option<Response>, ServerError> + Send + Sync>;

/// A boxed future returned by an asynchronous page handler.
pub type AsyncPageFuture<'a> = Pin<Box<dyn Future<Output = Result<Element, ServerError>> + 'a>>;

/// Renders a page asynchronously for a matched route.
pub type AsyncPageHandler =
    Arc<dyn for<'a> Fn(&'a mut RequestContext) -> AsyncPageFuture<'a> + Send + Sync>;

/// Handles an API route method synchronously.
pub type SyncApiHandler = fn(&mut RequestContext) -> Result<Response, ServerError>;

/// A boxed future returned by an asynchronous API handler.
pub type AsyncApiFuture<'a> = Pin<Box<dyn Future<Output = Result<Response, ServerError>> + 'a>>;

/// Handles an API route method asynchronously.
pub type AsyncApiHandler =
    Arc<dyn for<'a> Fn(&'a mut RequestContext) -> AsyncApiFuture<'a> + Send + Sync>;

/// A synchronous or asynchronous API method handler.
#[derive(Clone)]
pub enum ApiHandler {
    /// A synchronous handler.
    Sync(SyncApiHandler),
    /// An asynchronous handler.
    Async(AsyncApiHandler),
}

impl From<SyncApiHandler> for ApiHandler {
    fn from(handler: SyncApiHandler) -> Self {
        Self::Sync(handler)
    }
}

/// Wraps page content in a layout.
pub type Layout = fn(Children) -> Element;

/// Produces document metadata for a matched route.
pub type MetadataHandler = fn(&mut RequestContext) -> Metadata;

/// Renders an error boundary for a failed page.
pub type ErrorHandler = fn(&ServerError) -> Element;

/// Renders the application not-found page.
pub type NotFoundHandler = fn() -> Element;

/// Renders a route's loading boundary.
pub type LoadingHandler = fn() -> Element;

#[derive(Clone)]
struct NotFoundRoute {
    handler: NotFoundHandler,
    layouts: Vec<Layout>,
}

/// The API handlers exported by a `route.rs` module.
#[derive(Clone, Default)]
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

#[derive(Clone)]
enum PageEndpoint {
    Sync(PageHandler),
    Async(AsyncPageHandler),
}

impl PageEndpoint {
    fn render(&self, context: &mut RequestContext) -> Result<Element, ServerError> {
        match self {
            Self::Sync(handler) => handler(context),
            Self::Async(handler) => block_on(handler(context)),
        }
    }
}

/// A page route and its rendering boundaries.
#[derive(Clone)]
pub struct PageRoute {
    pattern: String,
    page: PageEndpoint,
    layouts: Vec<Layout>,
    metadata: Vec<MetadataHandler>,
    loading: Option<LoadingHandler>,
    error: Option<ErrorHandler>,
}

impl PageRoute {
    /// Creates a page route without layouts or an error boundary.
    #[must_use]
    pub fn new(pattern: impl Into<String>, page: PageHandler) -> Self {
        Self {
            pattern: pattern.into(),
            page: PageEndpoint::Sync(page),
            layouts: Vec::new(),
            metadata: Vec::new(),
            loading: None,
            error: None,
        }
    }

    /// Creates a page route backed by an asynchronous handler.
    #[must_use]
    pub fn async_new(pattern: impl Into<String>, page: AsyncPageHandler) -> Self {
        Self {
            pattern: pattern.into(),
            page: PageEndpoint::Async(page),
            layouts: Vec::new(),
            metadata: Vec::new(),
            loading: None,
            error: None,
        }
    }

    /// Sets layouts from the root layout to the nearest layout.
    #[must_use]
    pub fn layouts(mut self, layouts: &[Layout]) -> Self {
        self.layouts = layouts.to_vec();
        self
    }

    /// Sets metadata handlers from the root to the nearest metadata module.
    #[must_use]
    pub fn metadata(mut self, handlers: &[MetadataHandler]) -> Self {
        self.metadata = handlers.to_vec();
        self
    }

    /// Sets the nearest loading boundary shown during client navigation.
    #[must_use]
    pub fn loading(mut self, handler: LoadingHandler) -> Self {
        self.loading = Some(handler);
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
    middleware: Option<MiddlewareHandler>,
    compression: Option<bool>,
    public_dir: Option<PathBuf>,
    client_dir: Option<PathBuf>,
    diagnostics: bool,
    navigation: Option<bool>,
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

    /// Adds application middleware that runs before routes and static files.
    ///
    /// The middleware may return a response to short-circuit the request, or
    /// `None` to continue. `#[rusty::app]` registers `src/middleware.rs`
    /// automatically when that file exports `pub fn middleware`.
    #[must_use]
    pub fn middleware<F>(mut self, middleware: F) -> Self
    where
        F: Fn(&mut RequestContext) -> Result<Option<Response>, ServerError> + Send + Sync + 'static,
    {
        self.middleware = Some(Arc::new(middleware));
        self
    }

    /// Enables or disables gzip compression for complete responses.
    ///
    /// Compression is enabled by default and applies to complete and streaming
    /// responses.
    #[must_use]
    pub fn compression(mut self, enabled: bool) -> Self {
        self.compression = Some(enabled);
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

    /// Enables or disables same-origin client navigation.
    ///
    /// Navigation is enabled automatically when any page has a loading boundary.
    /// Enabling it without boundaries still installs the runtime with a generic
    /// loading indicator. Disabling it makes every link use the browser's normal
    /// document request.
    #[must_use]
    pub fn navigation(mut self, enabled: bool) -> Self {
        self.navigation = Some(enabled);
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
        let navigation_enabled = self
            .navigation
            .unwrap_or_else(|| self.pages.iter().any(|page| page.loading.is_some()));
        let navigation = if navigation_enabled {
            Arc::new(navigation::build(&self.pages)?)
        } else {
            Arc::new(String::new())
        };

        if self.compression.unwrap_or(true) {
            router = router.middleware(rusty_server::compression());
        }
        if let Some(middleware) = &self.middleware {
            let middleware = Arc::clone(middleware);
            router = router.middleware(Arc::new(move |context, next| match middleware(context)? {
                Some(response) => Ok(response),
                None => next.run(context),
            }));
        }

        for page in &self.pages {
            let route = page.clone();
            let nav_get = Arc::clone(&navigation);
            router = router.get(&page.pattern, move |context| {
                Ok(render_page(&route, context, &nav_get))
            })?;
            let route = page.clone();
            let nav_head = Arc::clone(&navigation);
            router = router.method(Method::Head, &page.pattern, move |context| {
                Ok(render_page(&route, context, &nav_head))
            })?;
        }

        for (pattern, methods) in &self.apis {
            if methods.is_empty() {
                return Err(ServerError::Route(format!(
                    "API route `{pattern}` has no method handlers"
                )));
            }
            router = add_method(router, pattern, Method::Get, methods.get.as_ref())?;
            router = add_method(router, pattern, Method::Head, methods.get.as_ref())?;
            router = add_method(router, pattern, Method::Post, methods.post.as_ref())?;
            router = add_method(router, pattern, Method::Put, methods.put.as_ref())?;
            router = add_method(router, pattern, Method::Patch, methods.patch.as_ref())?;
            router = add_method(router, pattern, Method::Delete, methods.delete.as_ref())?;
            router = add_method(router, pattern, Method::Options, methods.options.as_ref())?;
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
            let navigation = Arc::clone(&navigation);
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
                    |route| render_not_found(route, &navigation),
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
    handler: Option<&ApiHandler>,
) -> Result<HttpRouter, ServerError> {
    match handler {
        Some(ApiHandler::Sync(handler)) => router.method(method, pattern, *handler),
        Some(ApiHandler::Async(handler)) => {
            let handler = Arc::clone(handler);
            router.method(method, pattern, move |context| block_on(handler(context)))
        }
        None => Ok(router),
    }
}

fn render_page(route: &PageRoute, context: &mut RequestContext, navigation: &str) -> Response {
    match route.page.render(context) {
        Ok(page) => {
            let metadata = resolve_metadata(&route.metadata, context);
            render_success(page, &route.layouts, navigation, &metadata)
        }
        Err(error) => {
            let page = route
                .error
                .map_or_else(|| default_error(&error), |handler| handler(&error));
            render_failure(page, &route.layouts, navigation)
        }
    }
}

fn resolve_metadata(handlers: &[MetadataHandler], context: &mut RequestContext) -> Metadata {
    let mut metadata = Metadata::new();
    for handler in handlers {
        metadata = metadata.merge(handler(context));
    }
    metadata
}

fn render_success(
    page: Element,
    layouts: &[Layout],
    navigation: &str,
    metadata: &Metadata,
) -> Response {
    render_document(page, layouts, StatusCode::OK, "Rusty", navigation, metadata)
}

fn render_failure(page: Element, layouts: &[Layout], navigation: &str) -> Response {
    render_document(
        page,
        layouts,
        StatusCode::INTERNAL_SERVER_ERROR,
        "Rusty",
        navigation,
        &Metadata::new(),
    )
}

fn render_document(
    page: Element,
    layouts: &[Layout],
    status: StatusCode,
    title: &str,
    navigation: &str,
    metadata: &Metadata,
) -> Response {
    let page = apply_layouts(page, layouts);
    let navigation = navigation::markup(navigation);
    match render_to_chunks(&page) {
        Ok(chunks) if layouts.is_empty() => {
            let mut stream = Vec::with_capacity(chunks.len() + 3);
            let default_title = if metadata.has_title() {
                String::new()
            } else {
                format!("<title>{title}</title>")
            };
            stream.push(format!(
                "<!doctype html><html><head><meta charset=\"utf-8\">{default_title}{}</head><body>",
                metadata.markup()
            ));
            stream.extend(chunks);
            stream.push(navigation);
            stream.push("</body></html>".to_owned());
            Response::stream_html(status, stream)
        }
        Ok(mut chunks) => {
            insert_metadata(&mut chunks, metadata);
            insert_navigation(&mut chunks, navigation);
            Response::stream_html(status, chunks)
        }
        Err(error) => render_error_response(&error.to_string()),
    }
}

fn insert_metadata(chunks: &mut Vec<String>, metadata: &Metadata) {
    if metadata.is_empty() {
        return;
    }
    let mut document = chunks.concat();
    if find_ascii_case_insensitive(&document, "</head>").is_none() {
        return;
    }
    if metadata.has_title() {
        remove_title_tags(&mut document);
    }
    remove_meta_tags(&mut document, metadata);
    if metadata.canonical().is_some() {
        remove_canonical_links(&mut document);
    }
    if let Some(index) = find_ascii_case_insensitive(&document, "</head>") {
        document.insert_str(index, &metadata.markup());
        *chunks = vec![document];
    }
}

fn remove_title_tags(document: &mut String) {
    loop {
        let Some(start) = find_tag_start(document, 0, "title") else {
            return;
        };
        let Some(open_end) = document[start..].find('>') else {
            return;
        };
        let content_start = start + open_end + 1;
        let Some(close) = find_ascii_case_insensitive_from(document, content_start, "</title>")
        else {
            return;
        };
        document.replace_range(start..close + "</title>".len(), "");
    }
}

fn remove_meta_tags(document: &mut String, metadata: &Metadata) {
    let mut search = 0;
    while let Some(start) = find_tag_start(document, search, "meta") {
        let Some(open_end) = document[start..].find('>') else {
            return;
        };
        let end = start + open_end + 1;
        let name = attribute_value(&document[start..end], "name");
        let property = attribute_value(&document[start..end], "property");
        if metadata.should_replace_meta(name.as_deref(), property.as_deref()) {
            document.replace_range(start..end, "");
            search = start;
        } else {
            search = end;
        }
    }
}

fn remove_canonical_links(document: &mut String) {
    let mut search = 0;
    while let Some(start) = find_tag_start(document, search, "link") {
        let Some(open_end) = document[start..].find('>') else {
            return;
        };
        let end = start + open_end + 1;
        if attribute_value(&document[start..end], "rel")
            .is_some_and(|value| value.eq_ignore_ascii_case("canonical"))
        {
            document.replace_range(start..end, "");
            search = start;
        } else {
            search = end;
        }
    }
}

fn find_tag_start(document: &str, from: usize, tag: &str) -> Option<usize> {
    let bytes = document.as_bytes();
    let needle = format!("<{tag}");
    let needle = needle.as_bytes();
    let mut index = from;
    while index + needle.len() <= bytes.len() {
        if bytes[index..index + needle.len()].eq_ignore_ascii_case(needle) {
            let next = index + needle.len();
            if next == bytes.len()
                || bytes[next].is_ascii_whitespace()
                || matches!(bytes[next], b'/' | b'>')
            {
                return Some(index);
            }
        }
        index += 1;
    }
    None
}

fn find_ascii_case_insensitive(document: &str, needle: &str) -> Option<usize> {
    find_ascii_case_insensitive_from(document, 0, needle)
}

fn find_ascii_case_insensitive_from(document: &str, from: usize, needle: &str) -> Option<usize> {
    let bytes = document.as_bytes();
    let needle = needle.as_bytes();
    if needle.is_empty() {
        return Some(from.min(bytes.len()));
    }
    let mut index = from;
    while index + needle.len() <= bytes.len() {
        if bytes[index..index + needle.len()].eq_ignore_ascii_case(needle) {
            return Some(index);
        }
        index += 1;
    }
    None
}

fn attribute_value(tag: &str, name: &str) -> Option<String> {
    let bytes = tag.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        while index < bytes.len() && !is_attribute_name(bytes[index]) {
            index += 1;
        }
        let start = index;
        while index < bytes.len() && is_attribute_name(bytes[index]) {
            index += 1;
        }
        if start == index {
            index += 1;
            continue;
        }
        let key = &tag[start..index];
        while index < bytes.len() && bytes[index].is_ascii_whitespace() {
            index += 1;
        }
        if index >= bytes.len() || bytes[index] != b'=' {
            continue;
        }
        index += 1;
        while index < bytes.len() && bytes[index].is_ascii_whitespace() {
            index += 1;
        }
        if index >= bytes.len() {
            return None;
        }
        let (value_start, value_end) = if matches!(bytes[index], b'"' | b'\'') {
            let quote = bytes[index];
            let value_start = index + 1;
            index = value_start;
            while index < bytes.len() && bytes[index] != quote {
                index += 1;
            }
            let value_end = index;
            index = (index + 1).min(bytes.len());
            (value_start, value_end)
        } else {
            let value_start = index;
            while index < bytes.len() && !bytes[index].is_ascii_whitespace() && bytes[index] != b'>'
            {
                index += 1;
            }
            (value_start, index)
        };
        if key.eq_ignore_ascii_case(name) {
            return Some(tag[value_start..value_end].to_owned());
        }
    }
    None
}

fn is_attribute_name(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b':')
}

fn insert_navigation(chunks: &mut Vec<String>, navigation: String) {
    if navigation.is_empty() {
        return;
    }
    for chunk in chunks.iter_mut().rev() {
        if let Some(index) = chunk.rfind("</body>") {
            chunk.insert_str(index, &navigation);
            return;
        }
    }
    chunks.push(navigation);
}

fn apply_layouts(mut page: Element, layouts: &[Layout]) -> Element {
    for layout in layouts.iter().rev() {
        page = layout(Children::new(page));
    }
    page
}

fn render_not_found(route: &NotFoundRoute, navigation: &str) -> Response {
    render_document(
        (route.handler)(),
        &route.layouts,
        StatusCode::NOT_FOUND,
        "Not Found",
        navigation,
        &Metadata::new(),
    )
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
