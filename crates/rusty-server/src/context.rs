//! Typed request context passed through handlers and middleware.

use std::any::{Any, TypeId};
use std::collections::BTreeMap;
use std::fmt;

use rusty_router::RouteParams;

use crate::{AuthIdentity, Request};

/// Type-keyed values shared between middleware and request handlers.
#[derive(Default)]
pub struct Extensions {
    values: BTreeMap<TypeId, Box<dyn Any + Send + Sync>>,
}

impl Extensions {
    /// Creates an empty extension map.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Inserts a value and returns any previous value of the same type.
    pub fn insert<T>(&mut self, value: T) -> Option<T>
    where
        T: Any + Send + Sync,
    {
        self.values
            .insert(TypeId::of::<T>(), Box::new(value))
            .and_then(|previous| previous.downcast::<T>().ok())
            .map(|previous| *previous)
    }

    /// Returns a reference to a value of the requested type.
    #[must_use]
    pub fn get<T>(&self) -> Option<&T>
    where
        T: Any + Send + Sync,
    {
        self.values
            .get(&TypeId::of::<T>())
            .and_then(|value| value.downcast_ref::<T>())
    }

    /// Returns a mutable reference to a value of the requested type.
    pub fn get_mut<T>(&mut self) -> Option<&mut T>
    where
        T: Any + Send + Sync,
    {
        self.values
            .get_mut(&TypeId::of::<T>())
            .and_then(|value| value.downcast_mut::<T>())
    }

    /// Removes and returns a value of the requested type.
    pub fn remove<T>(&mut self) -> Option<T>
    where
        T: Any + Send + Sync,
    {
        self.values
            .remove(&TypeId::of::<T>())
            .and_then(|value| value.downcast::<T>().ok())
            .map(|value| *value)
    }

    /// Returns `true` when a value of the requested type is present.
    #[must_use]
    pub fn contains<T>(&self) -> bool
    where
        T: Any + Send + Sync,
    {
        self.values.contains_key(&TypeId::of::<T>())
    }

    /// Returns the number of stored values.
    #[must_use]
    pub fn len(&self) -> usize {
        self.values.len()
    }

    /// Returns `true` when no values are stored.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.values.is_empty()
    }
}

impl fmt::Debug for Extensions {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Extensions")
            .field("len", &self.values.len())
            .finish()
    }
}

/// A request paired with captured route parameters, auth identity, and
/// middleware-owned extensions.
pub struct RequestContext {
    request: Request,
    params: RouteParams,
    extensions: Extensions,
    identity: Option<AuthIdentity>,
}

impl RequestContext {
    /// Creates a context with no route parameters or identity.
    #[must_use]
    pub fn new(request: Request) -> Self {
        Self {
            request,
            params: RouteParams::default(),
            extensions: Extensions::new(),
            identity: None,
        }
    }

    /// Creates a context with captured route parameters.
    #[must_use]
    pub fn with_params(request: Request, params: RouteParams) -> Self {
        Self {
            request,
            params,
            extensions: Extensions::new(),
            identity: None,
        }
    }

    /// Returns the underlying request.
    #[must_use]
    pub fn request(&self) -> &Request {
        &self.request
    }

    /// Returns the underlying request mutably.
    pub fn request_mut(&mut self) -> &mut Request {
        &mut self.request
    }

    /// Returns captured route parameters.
    #[must_use]
    pub fn params(&self) -> &RouteParams {
        &self.params
    }

    /// Returns a captured route parameter by name.
    #[must_use]
    pub fn param(&self, name: &str) -> Option<&str> {
        self.params.get(name)
    }

    /// Returns middleware-owned extensions.
    #[must_use]
    pub fn extensions(&self) -> &Extensions {
        &self.extensions
    }

    /// Returns middleware-owned extensions mutably.
    pub fn extensions_mut(&mut self) -> &mut Extensions {
        &mut self.extensions
    }

    /// Returns the authenticated identity, if any.
    #[must_use]
    pub fn identity(&self) -> Option<&AuthIdentity> {
        self.identity.as_ref()
    }

    /// Sets the authenticated identity.
    pub fn set_identity(&mut self, identity: Option<AuthIdentity>) {
        self.identity = identity;
    }

    /// Returns the authenticated identity or an authentication error.
    ///
    /// # Errors
    ///
    /// Returns [`crate::ServerError::Authentication`] when no identity is set.
    pub fn require_identity(&self) -> Result<&AuthIdentity, crate::ServerError> {
        self.identity.as_ref().ok_or_else(|| {
            crate::ServerError::Authentication("an authenticated identity is required".to_owned())
        })
    }

    pub(crate) fn set_params(&mut self, params: RouteParams) {
        self.params = params;
    }

    /// Consumes the context and returns its request, parameters, extensions,
    /// and identity.
    #[must_use]
    pub fn into_parts(self) -> (Request, RouteParams, Extensions, Option<AuthIdentity>) {
        (self.request, self.params, self.extensions, self.identity)
    }
}

impl fmt::Debug for RequestContext {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("RequestContext")
            .field("request", &self.request)
            .field("params", &self.params)
            .field("extensions", &self.extensions)
            .field("identity", &self.identity)
            .finish()
    }
}
