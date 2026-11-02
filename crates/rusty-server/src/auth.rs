//! Authentication identities and bearer-token authentication.

use std::collections::BTreeMap;
use std::sync::Arc;

use crate::{Middleware, Request, Response, ServerError, StatusCode};

/// An authenticated principal attached to a request context.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthIdentity {
    subject: String,
    roles: Vec<String>,
}

impl AuthIdentity {
    /// Creates an identity for a subject with no roles.
    #[must_use]
    pub fn new(subject: impl Into<String>) -> Self {
        Self {
            subject: subject.into(),
            roles: Vec::new(),
        }
    }

    /// Adds a role to the identity.
    #[must_use]
    pub fn with_role(mut self, role: impl Into<String>) -> Self {
        self.roles.push(role.into());
        self
    }

    /// Returns the subject identifier.
    #[must_use]
    pub fn subject(&self) -> &str {
        &self.subject
    }

    /// Returns the identity's roles.
    #[must_use]
    pub fn roles(&self) -> &[String] {
        &self.roles
    }

    /// Returns `true` when the identity has the given role.
    #[must_use]
    pub fn has_role(&self, role: &str) -> bool {
        self.roles.iter().any(|candidate| candidate == role)
    }
}

/// Resolves an identity from a request.
///
/// Returning `Ok(None)` means the request is anonymous. Returning an error
/// aborts request handling.
pub trait Authenticator: Send + Sync {
    /// Authenticates the request.
    ///
    /// # Errors
    ///
    /// Returns [`ServerError::Authentication`] when credentials are present but
    /// invalid.
    fn authenticate(&self, request: &Request) -> Result<Option<AuthIdentity>, ServerError>;
}

/// A static bearer-token authenticator.
///
/// Tokens are compared in constant time. This is suitable for service tokens
/// and tests; applications should replace it with a database-backed
/// authenticator.
#[derive(Debug, Clone, Default)]
pub struct BearerAuthenticator {
    tokens: BTreeMap<String, AuthIdentity>,
}

impl BearerAuthenticator {
    /// Creates an authenticator with no accepted tokens.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Registers a token for an identity.
    pub fn allow(&mut self, token: impl Into<String>, identity: AuthIdentity) {
        self.tokens.insert(token.into(), identity);
    }

    /// Registers a token and returns the authenticator.
    #[must_use]
    pub fn with_token(mut self, token: impl Into<String>, identity: AuthIdentity) -> Self {
        self.allow(token, identity);
        self
    }
}

impl Authenticator for BearerAuthenticator {
    fn authenticate(&self, request: &Request) -> Result<Option<AuthIdentity>, ServerError> {
        let Some(header) = request.headers().get("authorization") else {
            return Ok(None);
        };
        let Some((scheme, token)) = header.split_once(' ') else {
            return Ok(None);
        };
        if !scheme.eq_ignore_ascii_case("bearer") || token.is_empty() {
            return Ok(None);
        }

        Ok(self
            .tokens
            .iter()
            .find(|(candidate, _)| constant_time_eq(candidate.as_bytes(), token.as_bytes()))
            .map(|(_, identity)| identity.clone()))
    }
}

pub(crate) fn constant_time_eq(left: &[u8], right: &[u8]) -> bool {
    if left.len() != right.len() {
        return false;
    }
    let mut difference = 0_u8;
    for (left, right) in left.iter().zip(right) {
        difference |= left ^ right;
    }
    difference == 0
}

/// Creates middleware that authenticates requests when credentials are present.
///
/// Anonymous requests continue without an identity.
#[must_use]
pub fn authenticate<A>(authenticator: A) -> Middleware
where
    A: Authenticator + 'static,
{
    let authenticator = Arc::new(authenticator);
    Arc::new(move |context, next| {
        if let Some(identity) = authenticator.authenticate(context.request())? {
            context.set_identity(Some(identity));
        }
        next.run(context)
    })
}

/// Creates middleware that requires a valid authenticated identity.
///
/// Missing or invalid credentials produce a `401 Unauthorized` response.
#[must_use]
pub fn require_auth<A>(authenticator: A) -> Middleware
where
    A: Authenticator + 'static,
{
    let authenticator = Arc::new(authenticator);
    Arc::new(
        move |context, next| match authenticator.authenticate(context.request())? {
            Some(identity) => {
                context.set_identity(Some(identity));
                next.run(context)
            }
            None => Ok(
                Response::text(StatusCode::UNAUTHORIZED, "authentication required")
                    .with_header("www-authenticate", "Bearer")
                    .expect("static header is valid"),
            ),
        },
    )
}
