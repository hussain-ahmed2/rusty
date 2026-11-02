//! Composable request middleware.

use std::sync::Arc;

use crate::{RequestContext, Response, ServerError};

/// A request handler.
pub type Handler = Arc<dyn Fn(&mut RequestContext) -> Result<Response, ServerError> + Send + Sync>;

/// Middleware that can inspect or replace a request, then optionally continue.
pub type Middleware =
    Arc<dyn Fn(&mut RequestContext, Next<'_>) -> Result<Response, ServerError> + Send + Sync>;

/// The remainder of a middleware chain.
pub struct Next<'a> {
    remaining: &'a [Middleware],
    handler: &'a Handler,
}

impl<'a> Next<'a> {
    /// Runs the remainder of the chain and the final handler.
    ///
    /// # Errors
    ///
    /// Propagates errors from later middleware or the handler.
    pub fn run(self, context: &mut RequestContext) -> Result<Response, ServerError> {
        run_chain(self.remaining, self.handler, context)
    }
}

pub(crate) fn run_chain(
    middlewares: &[Middleware],
    handler: &Handler,
    context: &mut RequestContext,
) -> Result<Response, ServerError> {
    let Some((middleware, remaining)) = middlewares.split_first() else {
        return handler(context);
    };
    middleware(context, Next { remaining, handler })
}
