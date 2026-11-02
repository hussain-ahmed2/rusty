//! Method-aware HTTP routing with middleware.

use std::sync::Arc;

use rusty_router::{RouteParams, RoutePattern};

use crate::middleware::{run_chain, Handler, Middleware};
use crate::{Method, Request, RequestContext, Response, ServerError, StatusCode};

#[derive(Clone)]
struct HttpRoute {
    method: Option<Method>,
    pattern: RoutePattern,
    handler: Handler,
}

#[derive(Clone, Default)]
struct RouterInner {
    routes: Vec<HttpRoute>,
    fallback: Option<Handler>,
}

/// A method-aware HTTP router with composable middleware and a fallback.
///
/// ```
/// use rusty_server::{HttpRouter, Request, Response, StatusCode};
///
/// let app = HttpRouter::new()
///     .get("/health", |_| Ok(Response::text(StatusCode::OK, "ok")))
///     .expect("valid route");
///
/// let response = app.handle(Request::new(rusty_server::Method::Get, "/health"))
///     .expect("handle request");
/// assert_eq!(response.body(), b"ok");
/// ```
#[derive(Clone)]
pub struct HttpRouter {
    inner: Arc<RouterInner>,
    middlewares: Vec<Middleware>,
}

impl Default for HttpRouter {
    fn default() -> Self {
        Self::new()
    }
}

impl HttpRouter {
    /// Creates an empty router.
    #[must_use]
    pub fn new() -> Self {
        Self {
            inner: Arc::new(RouterInner::default()),
            middlewares: Vec::new(),
        }
    }

    /// Registers a handler for any method.
    ///
    /// # Errors
    ///
    /// Returns [`ServerError::Route`] when the pattern is invalid or overlaps an
    /// existing route for the same method.
    pub fn route<F>(mut self, pattern: &str, handler: F) -> Result<Self, ServerError>
    where
        F: Fn(&mut RequestContext) -> Result<Response, ServerError> + Send + Sync + 'static,
    {
        self.add(None, pattern, handler)?;
        Ok(self)
    }

    /// Registers a `GET` handler.
    ///
    /// # Errors
    ///
    /// Returns [`ServerError::Route`] when the pattern is invalid or overlaps an
    /// existing route for the same method.
    pub fn get<F>(self, pattern: &str, handler: F) -> Result<Self, ServerError>
    where
        F: Fn(&mut RequestContext) -> Result<Response, ServerError> + Send + Sync + 'static,
    {
        self.method(Method::Get, pattern, handler)
    }

    /// Registers a `POST` handler.
    ///
    /// # Errors
    ///
    /// Returns [`ServerError::Route`] when the pattern is invalid or overlaps an
    /// existing route for the same method.
    pub fn post<F>(self, pattern: &str, handler: F) -> Result<Self, ServerError>
    where
        F: Fn(&mut RequestContext) -> Result<Response, ServerError> + Send + Sync + 'static,
    {
        self.method(Method::Post, pattern, handler)
    }

    /// Registers a `PUT` handler.
    ///
    /// # Errors
    ///
    /// Returns [`ServerError::Route`] when the pattern is invalid or overlaps an
    /// existing route for the same method.
    pub fn put<F>(self, pattern: &str, handler: F) -> Result<Self, ServerError>
    where
        F: Fn(&mut RequestContext) -> Result<Response, ServerError> + Send + Sync + 'static,
    {
        self.method(Method::Put, pattern, handler)
    }

    /// Registers a `PATCH` handler.
    ///
    /// # Errors
    ///
    /// Returns [`ServerError::Route`] when the pattern is invalid or overlaps an
    /// existing route for the same method.
    pub fn patch<F>(self, pattern: &str, handler: F) -> Result<Self, ServerError>
    where
        F: Fn(&mut RequestContext) -> Result<Response, ServerError> + Send + Sync + 'static,
    {
        self.method(Method::Patch, pattern, handler)
    }

    /// Registers a `DELETE` handler.
    ///
    /// # Errors
    ///
    /// Returns [`ServerError::Route`] when the pattern is invalid or overlaps an
    /// existing route for the same method.
    pub fn delete<F>(self, pattern: &str, handler: F) -> Result<Self, ServerError>
    where
        F: Fn(&mut RequestContext) -> Result<Response, ServerError> + Send + Sync + 'static,
    {
        self.method(Method::Delete, pattern, handler)
    }

    /// Registers a handler for a specific method.
    ///
    /// # Errors
    ///
    /// Returns [`ServerError::Route`] when the pattern is invalid or overlaps an
    /// existing route for the same method.
    pub fn method<F>(
        mut self,
        method: Method,
        pattern: &str,
        handler: F,
    ) -> Result<Self, ServerError>
    where
        F: Fn(&mut RequestContext) -> Result<Response, ServerError> + Send + Sync + 'static,
    {
        self.add(Some(method), pattern, handler)?;
        Ok(self)
    }

    /// Adds middleware that runs before handlers in registration order.
    #[must_use]
    pub fn middleware(mut self, middleware: Middleware) -> Self {
        self.middlewares.push(middleware);
        self
    }

    /// Sets the handler used when no route matches instead of the default 404.
    #[must_use]
    pub fn fallback<F>(mut self, handler: F) -> Self
    where
        F: Fn(&mut RequestContext) -> Result<Response, ServerError> + Send + Sync + 'static,
    {
        Arc::make_mut(&mut self.inner).fallback = Some(Arc::new(handler));
        self
    }

    /// Dispatches a request through middleware and the matched route.
    ///
    /// # Errors
    ///
    /// Propagates errors from middleware or the matched handler.
    pub fn handle(&self, request: Request) -> Result<Response, ServerError> {
        let inner = Arc::clone(&self.inner);
        let handler: Handler = Arc::new(move |context| dispatch(&inner, context));
        let mut context = RequestContext::new(request);
        run_chain(&self.middlewares, &handler, &mut context)
    }

    fn add<F>(
        &mut self,
        method: Option<Method>,
        pattern: &str,
        handler: F,
    ) -> Result<(), ServerError>
    where
        F: Fn(&mut RequestContext) -> Result<Response, ServerError> + Send + Sync + 'static,
    {
        let pattern = RoutePattern::parse(pattern)?;
        let inner = Arc::make_mut(&mut self.inner);
        for existing in &inner.routes {
            let same_method = match (existing.method, method) {
                (Some(left), Some(right)) => left == right,
                _ => true,
            };
            if existing.pattern.same_shape(&pattern) && same_method {
                return Err(ServerError::Route(format!(
                    "route `{pattern}` overlaps an existing route for the same method"
                )));
            }
        }
        inner.routes.push(HttpRoute {
            method,
            pattern,
            handler: Arc::new(handler),
        });
        Ok(())
    }
}

fn dispatch(inner: &RouterInner, context: &mut RequestContext) -> Result<Response, ServerError> {
    let path = context.request().path().to_owned();
    let method = context.request().method();
    let mut best: Option<(&HttpRoute, RouteParams)> = None;
    let mut path_matched = false;

    for route in &inner.routes {
        let Some(params) = route.pattern.match_path(&path) else {
            continue;
        };
        path_matched = true;
        if route.method.map_or(true, |candidate| candidate == method) {
            let replace = best.as_ref().map_or(true, |(current, _)| {
                route.pattern.is_more_specific(&current.pattern)
            });
            if replace {
                best = Some((route, params));
            }
        }
    }

    if let Some((route, params)) = best {
        context.set_params(params);
        return (route.handler)(context);
    }
    if path_matched {
        return Ok(Response::text(
            StatusCode::METHOD_NOT_ALLOWED,
            "method not allowed",
        ));
    }
    if let Some(fallback) = &inner.fallback {
        return fallback(context);
    }
    Ok(Response::text(StatusCode::NOT_FOUND, "not found"))
}
