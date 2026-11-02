//! Route patterns, segments, and captured parameters.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

use crate::RouterError;

/// One segment of a route pattern.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum RouteSegment {
    /// A literal path segment such as `about`.
    Static {
        /// The literal segment value.
        value: String,
    },
    /// A single dynamic segment such as `[slug]`.
    Dynamic {
        /// The parameter name captured by this segment.
        name: String,
    },
    /// A trailing segment that captures the rest of the path, such as `[...path]`.
    CatchAll {
        /// The parameter name captured by this segment.
        name: String,
    },
}

impl RouteSegment {
    /// Creates a validated static segment.
    ///
    /// # Errors
    ///
    /// Returns [`RouterError::InvalidPattern`] when the value is empty or
    /// contains route delimiters.
    pub fn static_segment(value: impl Into<String>) -> Result<Self, RouterError> {
        let value = value.into();
        validate_static(&value)?;
        Ok(Self::Static { value })
    }

    /// Creates a validated dynamic segment.
    ///
    /// # Errors
    ///
    /// Returns [`RouterError::InvalidPattern`] when the parameter name is not a
    /// valid identifier.
    pub fn dynamic(name: impl Into<String>) -> Result<Self, RouterError> {
        let name = name.into();
        validate_name(&name)?;
        Ok(Self::Dynamic { name })
    }

    /// Creates a validated catch-all segment.
    ///
    /// # Errors
    ///
    /// Returns [`RouterError::InvalidPattern`] when the parameter name is not a
    /// valid identifier.
    pub fn catch_all(name: impl Into<String>) -> Result<Self, RouterError> {
        let name = name.into();
        validate_name(&name)?;
        Ok(Self::CatchAll { name })
    }

    /// Returns the literal value for a static segment.
    #[must_use]
    pub fn static_value(&self) -> Option<&str> {
        match self {
            Self::Static { value } => Some(value),
            Self::Dynamic { .. } | Self::CatchAll { .. } => None,
        }
    }

    /// Returns the parameter name for a dynamic or catch-all segment.
    #[must_use]
    pub fn parameter_name(&self) -> Option<&str> {
        match self {
            Self::Static { .. } => None,
            Self::Dynamic { name } | Self::CatchAll { name } => Some(name),
        }
    }

    /// Returns `true` when this segment captures the rest of the path.
    #[must_use]
    pub const fn is_catch_all(&self) -> bool {
        matches!(self, Self::CatchAll { .. })
    }

    fn precedence(&self) -> u8 {
        match self {
            Self::Static { .. } => 2,
            Self::Dynamic { .. } => 1,
            Self::CatchAll { .. } => 0,
        }
    }
}

impl fmt::Display for RouteSegment {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Static { value } => formatter.write_str(value),
            Self::Dynamic { name } => write!(formatter, "[{name}]"),
            Self::CatchAll { name } => write!(formatter, "[...{name}]"),
        }
    }
}

/// A validated route pattern made of zero or more segments.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RoutePattern {
    segments: Vec<RouteSegment>,
}

impl RoutePattern {
    /// Creates a route pattern and validates its segments.
    ///
    /// # Errors
    ///
    /// Returns [`RouterError::InvalidPattern`] when a catch-all is not last, two
    /// parameters share a name, or a segment is malformed.
    pub fn new(segments: Vec<RouteSegment>) -> Result<Self, RouterError> {
        let pattern = Self { segments };
        pattern.validate()?;
        Ok(pattern)
    }

    /// Parses a route pattern such as `/blog/[slug]`.
    ///
    /// # Errors
    ///
    /// Returns [`RouterError::InvalidPattern`] when the path is not absolute or
    /// a segment is malformed.
    pub fn parse(value: &str) -> Result<Self, RouterError> {
        if !value.starts_with('/') {
            return Err(RouterError::invalid_pattern(
                value,
                "route patterns must start with `/`",
            ));
        }
        if value.contains(['?', '#']) {
            return Err(RouterError::invalid_pattern(
                value,
                "route patterns cannot contain a query string or fragment",
            ));
        }

        let body = &value[1..];
        let mut segments = Vec::new();
        if !body.is_empty() {
            for segment in body.split('/') {
                if segment.is_empty() {
                    return Err(RouterError::invalid_pattern(
                        value,
                        "route patterns cannot contain empty segments",
                    ));
                }

                if let Some(name) = segment
                    .strip_prefix("[...")
                    .and_then(|name| name.strip_suffix(']'))
                {
                    segments.push(RouteSegment::catch_all(name)?);
                } else if let Some(name) = segment
                    .strip_prefix('[')
                    .and_then(|name| name.strip_suffix(']'))
                {
                    segments.push(RouteSegment::dynamic(name)?);
                } else {
                    segments.push(RouteSegment::static_segment(segment)?);
                }
            }
        }

        Self::new(segments)
    }

    /// Returns `true` when this pattern is more specific than `other`.
    ///
    /// Specificity is compared left to right: static segments beat dynamic
    /// segments, which beat catch-all segments.
    #[must_use]
    pub fn is_more_specific(&self, other: &Self) -> bool {
        self.precedence() > other.precedence()
    }

    /// Returns `true` when both patterns match the same URL shape.
    ///
    /// Parameter names are ignored, so `/blog/[slug]` and `/blog/[id]` have the
    /// same shape.
    #[must_use]
    pub fn same_shape(&self, other: &Self) -> bool {
        self.signature() == other.signature()
    }

    /// Creates the root route pattern (`/`).
    #[must_use]
    pub fn root() -> Self {
        Self {
            segments: Vec::new(),
        }
    }

    /// Returns the pattern segments.
    #[must_use]
    pub fn segments(&self) -> &[RouteSegment] {
        &self.segments
    }

    /// Returns `true` when this pattern matches only `/`.
    #[must_use]
    pub fn is_root(&self) -> bool {
        self.segments.is_empty()
    }

    /// Returns `true` when the pattern ends in a catch-all segment.
    #[must_use]
    pub fn has_catch_all(&self) -> bool {
        self.segments.last().is_some_and(RouteSegment::is_catch_all)
    }

    /// Matches a request path against this pattern.
    ///
    /// Returns captured parameters on success. Query strings and fragments are
    /// ignored. Percent-encoded segments are captured without decoding.
    #[must_use]
    pub fn match_path(&self, path: &str) -> Option<RouteParams> {
        let request = split_path(path)?;
        self.match_segments(&request)
    }

    pub(crate) fn match_segments(&self, request: &[&str]) -> Option<RouteParams> {
        let mut params = RouteParams::default();
        let mut request_index = 0;

        for segment in &self.segments {
            match segment {
                RouteSegment::Static { value } => {
                    if request.get(request_index).copied() != Some(value.as_str()) {
                        return None;
                    }
                    request_index += 1;
                }
                RouteSegment::Dynamic { name } => {
                    let value = request.get(request_index)?;
                    if value.is_empty() {
                        return None;
                    }
                    params.insert(name.clone(), (*value).to_owned());
                    request_index += 1;
                }
                RouteSegment::CatchAll { name } => {
                    let rest = request.get(request_index..)?;
                    if rest.is_empty() {
                        return None;
                    }
                    params.insert(name.clone(), rest.join("/"));
                    request_index = request.len();
                }
            }
        }

        (request_index == request.len()).then_some(params)
    }

    pub(crate) fn precedence(&self) -> Vec<u8> {
        self.segments.iter().map(RouteSegment::precedence).collect()
    }

    pub(crate) fn signature(&self) -> String {
        let mut signature = String::new();
        for segment in &self.segments {
            match segment {
                RouteSegment::Static { value } => {
                    signature.push('s');
                    signature.push_str(value);
                    signature.push('\0');
                }
                RouteSegment::Dynamic { .. } => signature.push_str("d\0"),
                RouteSegment::CatchAll { .. } => signature.push_str("c\0"),
            }
        }
        signature
    }

    pub(crate) fn validate(&self) -> Result<(), RouterError> {
        let mut names = BTreeSet::new();

        for (index, segment) in self.segments.iter().enumerate() {
            match segment {
                RouteSegment::Static { value } => validate_static(value)?,
                RouteSegment::Dynamic { name } | RouteSegment::CatchAll { name } => {
                    validate_name(name)?;
                    if !names.insert(name.clone()) {
                        return Err(RouterError::invalid_pattern(
                            self.to_string(),
                            format!("parameter `{name}` is used more than once"),
                        ));
                    }
                }
            }

            if segment.is_catch_all() && index + 1 != self.segments.len() {
                return Err(RouterError::invalid_pattern(
                    self.to_string(),
                    "a catch-all segment must be the last segment",
                ));
            }
        }

        Ok(())
    }
}

impl FromStr for RoutePattern {
    type Err = RouterError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::parse(value)
    }
}

impl fmt::Display for RoutePattern {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("/")?;
        for (index, segment) in self.segments.iter().enumerate() {
            if index > 0 {
                formatter.write_str("/")?;
            }
            segment.fmt(formatter)?;
        }
        Ok(())
    }
}

/// Parameters captured from a request path.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RouteParams {
    values: BTreeMap<String, String>,
}

impl RouteParams {
    /// Returns a captured value by parameter name.
    #[must_use]
    pub fn get(&self, name: &str) -> Option<&str> {
        self.values.get(name).map(String::as_str)
    }

    /// Iterates over captured values in parameter-name order.
    pub fn iter(&self) -> impl Iterator<Item = (&str, &str)> {
        self.values
            .iter()
            .map(|(name, value)| (name.as_str(), value.as_str()))
    }

    /// Returns the number of captured parameters.
    #[must_use]
    pub fn len(&self) -> usize {
        self.values.len()
    }

    /// Returns `true` when no parameters were captured.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.values.is_empty()
    }

    fn insert(&mut self, name: String, value: String) {
        self.values.insert(name, value);
    }
}

pub(crate) fn split_path(path: &str) -> Option<Vec<&str>> {
    let path = path.split(['?', '#']).next()?;
    if !path.starts_with('/') {
        return None;
    }

    Some(
        path.split('/')
            .filter(|segment| !segment.is_empty())
            .collect(),
    )
}

fn validate_static(value: &str) -> Result<(), RouterError> {
    if value.is_empty() {
        return Err(RouterError::invalid_pattern(
            value,
            "a static segment cannot be empty",
        ));
    }
    if value.contains(['/', '[', ']', '(', ')']) {
        return Err(RouterError::invalid_pattern(
            value,
            "a static segment cannot contain route delimiters",
        ));
    }
    Ok(())
}

fn validate_name(name: &str) -> Result<(), RouterError> {
    let mut characters = name.chars();
    let Some(first) = characters.next() else {
        return Err(RouterError::invalid_pattern(
            name,
            "a route parameter needs a name",
        ));
    };

    if !(first.is_ascii_alphabetic() || first == '_') {
        return Err(RouterError::invalid_pattern(
            name,
            "route parameter names must start with an ASCII letter or underscore",
        ));
    }
    if !characters.all(|character| character.is_ascii_alphanumeric() || character == '_') {
        return Err(RouterError::invalid_pattern(
            name,
            "route parameter names may only contain ASCII letters, digits, and underscores",
        ));
    }

    Ok(())
}
