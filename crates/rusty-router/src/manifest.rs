//! Route discovery, matching, and serialized manifests.

use std::cmp::Ordering;
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::pattern::{split_path, RouteParams, RoutePattern, RouteSegment};
use crate::RouterError;

/// A discovered page route and its rendering boundaries.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RouteEntry {
    pattern: RoutePattern,
    page: PathBuf,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    layouts: Vec<PathBuf>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    loading: Option<PathBuf>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    error: Option<PathBuf>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    not_found: Option<PathBuf>,
}

impl RouteEntry {
    /// Returns the URL pattern rendered by this route.
    #[must_use]
    pub fn pattern(&self) -> &RoutePattern {
        &self.pattern
    }

    /// Returns the page module path, relative to the application root.
    #[must_use]
    pub fn page(&self) -> &Path {
        &self.page
    }

    /// Returns layout module paths from the root layout to the nearest layout.
    #[must_use]
    pub fn layouts(&self) -> &[PathBuf] {
        &self.layouts
    }

    /// Returns the nearest loading boundary module.
    #[must_use]
    pub fn loading(&self) -> Option<&Path> {
        self.loading.as_deref()
    }

    /// Returns the nearest error boundary module.
    #[must_use]
    pub fn error(&self) -> Option<&Path> {
        self.error.as_deref()
    }

    /// Returns the nearest not-found boundary module.
    #[must_use]
    pub fn not_found(&self) -> Option<&Path> {
        self.not_found.as_deref()
    }
}

/// A discovered API endpoint and its route pattern.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ApiRoute {
    pattern: RoutePattern,
    route: PathBuf,
}

impl ApiRoute {
    /// Returns the URL pattern handled by this endpoint.
    #[must_use]
    pub fn pattern(&self) -> &RoutePattern {
        &self.pattern
    }

    /// Returns the `route.rs` module path, relative to the application root.
    #[must_use]
    pub fn route(&self) -> &Path {
        &self.route
    }
}

/// A serializable manifest of file-based routes.
///
/// Paths are relative to the application root passed to
/// [`RouteManifest::discover`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RouteManifest {
    routes: Vec<RouteEntry>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    api_routes: Vec<ApiRoute>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    not_found: Option<PathBuf>,
}

impl RouteManifest {
    /// Discovers routes under an application directory.
    ///
    /// The directory follows the Rusty `src/app` conventions:
    ///
    /// - `page.rs` creates a rendered route and `route.rs` creates an API route.
    /// - `layout.rs` wraps that route and its descendants.
    /// - `loading.rs`, `error.rs`, and `not_found.rs` apply to descendants.
    /// - `[name]` captures one segment and `[...name]` captures the rest.
    /// - `(group)` directories organize files without adding a URL segment.
    /// - Directories beginning with `_` or `.` are ignored.
    ///
    /// # Errors
    ///
    /// Returns [`RouterError`] when the filesystem cannot be read, a directory
    /// name is malformed, or two pages resolve to the same URL shape.
    pub fn discover(app_root: impl AsRef<Path>) -> Result<Self, RouterError> {
        let app_root = app_root.as_ref();
        let metadata = fs::metadata(app_root).map_err(|error| RouterError::io(app_root, &error))?;
        if !metadata.is_dir() {
            return Err(RouterError::invalid_route(
                app_root,
                "the application root must be a directory",
            ));
        }

        let mut routes = Vec::new();
        let mut api_routes = Vec::new();
        let mut state = WalkState::default();
        visit(app_root, app_root, &mut state, &mut routes, &mut api_routes)?;
        let mut manifest = Self {
            routes,
            api_routes,
            not_found: state.not_found,
        };
        manifest.sort();
        manifest.validate()?;
        Ok(manifest)
    }

    /// Returns the discovered routes.
    #[must_use]
    pub fn routes(&self) -> &[RouteEntry] {
        &self.routes
    }

    /// Returns the discovered API endpoints.
    #[must_use]
    pub fn api_routes(&self) -> &[ApiRoute] {
        &self.api_routes
    }

    /// Returns the root not-found boundary, used when no route matches.
    #[must_use]
    pub fn not_found(&self) -> Option<&Path> {
        self.not_found.as_deref()
    }

    /// Returns the number of routes.
    #[must_use]
    pub fn len(&self) -> usize {
        self.routes.len()
    }

    /// Returns the number of API endpoints.
    #[must_use]
    pub fn api_len(&self) -> usize {
        self.api_routes.len()
    }

    /// Returns `true` when no pages or API endpoints were discovered.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.routes.is_empty() && self.api_routes.is_empty()
    }

    /// Matches a request path against the most specific route.
    ///
    /// Static segments take precedence over dynamic segments, which take
    /// precedence over catch-all segments. Query strings and fragments are
    /// ignored, and percent-encoded segments are captured without decoding.
    #[must_use]
    pub fn match_path<'a>(&'a self, path: &str) -> Option<RouteMatch<'a>> {
        let (entry, params) = best_match(
            self.routes.iter().map(|entry| (&entry.pattern, entry)),
            path,
        )?;
        Some(RouteMatch { entry, params })
    }

    /// Matches a request path against the most specific API endpoint.
    #[must_use]
    pub fn match_api_path<'a>(&'a self, path: &str) -> Option<ApiRouteMatch<'a>> {
        let (route, params) = best_match(
            self.api_routes.iter().map(|route| (&route.pattern, route)),
            path,
        )?;
        Some(ApiRouteMatch { route, params })
    }

    /// Serializes the manifest as compact JSON.
    ///
    /// # Errors
    ///
    /// Returns [`RouterError::Manifest`] when serialization fails.
    pub fn to_json(&self) -> Result<String, RouterError> {
        serde_json::to_string(self).map_err(|error| RouterError::manifest(error.to_string()))
    }

    /// Serializes the manifest as pretty-printed JSON.
    ///
    /// # Errors
    ///
    /// Returns [`RouterError::Manifest`] when serialization fails.
    pub fn to_json_pretty(&self) -> Result<String, RouterError> {
        serde_json::to_string_pretty(self).map_err(|error| RouterError::manifest(error.to_string()))
    }

    /// Restores and validates a serialized manifest.
    ///
    /// # Errors
    ///
    /// Returns [`RouterError::Manifest`] when JSON decoding fails and
    /// [`RouterError`] when the decoded routes are invalid or duplicated.
    pub fn from_json(json: &str) -> Result<Self, RouterError> {
        let manifest: Self =
            serde_json::from_str(json).map_err(|error| RouterError::manifest(error.to_string()))?;
        manifest.validate()?;
        Ok(manifest)
    }

    fn sort(&mut self) {
        self.routes.sort_by(|left, right| {
            left.pattern
                .to_string()
                .cmp(&right.pattern.to_string())
                .then_with(|| left.page.cmp(&right.page))
        });
        self.api_routes.sort_by(|left, right| {
            left.pattern
                .to_string()
                .cmp(&right.pattern.to_string())
                .then_with(|| left.route.cmp(&right.route))
        });
    }

    fn validate(&self) -> Result<(), RouterError> {
        let mut seen = BTreeMap::<String, (String, PathBuf)>::new();

        for entry in &self.routes {
            entry.pattern.validate()?;
            if entry.page.as_os_str().is_empty() {
                return Err(RouterError::manifest(
                    "a route entry is missing its page path",
                ));
            }

            let signature = entry.pattern.signature();
            if let Some((pattern, first)) = seen.get(&signature) {
                return Err(RouterError::DuplicateRoute {
                    pattern: pattern.clone(),
                    first: first.clone(),
                    second: entry.page.clone(),
                });
            }
            seen.insert(signature, (entry.pattern.to_string(), entry.page.clone()));
        }

        for route in &self.api_routes {
            route.pattern.validate()?;
            if route.route.as_os_str().is_empty() {
                return Err(RouterError::manifest(
                    "an API route is missing its route path",
                ));
            }

            let signature = route.pattern.signature();
            if let Some((pattern, first)) = seen.get(&signature) {
                return Err(RouterError::DuplicateRoute {
                    pattern: pattern.clone(),
                    first: first.clone(),
                    second: route.route.clone(),
                });
            }
            seen.insert(signature, (route.pattern.to_string(), route.route.clone()));
        }

        if self
            .not_found
            .as_ref()
            .is_some_and(|path| path.as_os_str().is_empty())
        {
            return Err(RouterError::manifest(
                "the not-found boundary path cannot be empty",
            ));
        }

        Ok(())
    }
}

/// A matched route and its captured parameters.
#[derive(Debug)]
pub struct RouteMatch<'a> {
    entry: &'a RouteEntry,
    params: RouteParams,
}

impl<'a> RouteMatch<'a> {
    /// Returns the matched route entry.
    #[must_use]
    pub fn entry(&self) -> &'a RouteEntry {
        self.entry
    }

    /// Returns the captured route parameters.
    #[must_use]
    pub fn params(&self) -> &RouteParams {
        &self.params
    }

    /// Consumes the match and returns its route entry and parameters.
    #[must_use]
    pub fn into_parts(self) -> (&'a RouteEntry, RouteParams) {
        (self.entry, self.params)
    }
}

/// A matched API endpoint and its captured parameters.
#[derive(Debug)]
pub struct ApiRouteMatch<'a> {
    route: &'a ApiRoute,
    params: RouteParams,
}

impl<'a> ApiRouteMatch<'a> {
    /// Returns the matched API route.
    #[must_use]
    pub fn route(&self) -> &'a ApiRoute {
        self.route
    }

    /// Returns the captured route parameters.
    #[must_use]
    pub fn params(&self) -> &RouteParams {
        &self.params
    }

    /// Consumes the match and returns its route and parameters.
    #[must_use]
    pub fn into_parts(self) -> (&'a ApiRoute, RouteParams) {
        (self.route, self.params)
    }
}

fn best_match<'a, T>(
    items: impl IntoIterator<Item = (&'a RoutePattern, &'a T)>,
    path: &str,
) -> Option<(&'a T, RouteParams)> {
    let request = split_path(path)?;
    let mut best: Option<(&'a T, RouteParams, Vec<u8>)> = None;

    for (pattern, item) in items {
        let Some(params) = pattern.match_segments(&request) else {
            continue;
        };
        let precedence = pattern.precedence();
        let replace = best.as_ref().map_or(true, |(_, _, current)| {
            precedence.cmp(current) == Ordering::Greater
        });
        if replace {
            best = Some((item, params, precedence));
        }
    }

    best.map(|(item, params, _)| (item, params))
}

#[derive(Clone, Default)]
struct WalkState {
    segments: Vec<RouteSegment>,
    layouts: Vec<PathBuf>,
    loading: Option<PathBuf>,
    error: Option<PathBuf>,
    not_found: Option<PathBuf>,
}

#[derive(Default)]
struct DirectoryScan {
    page: Option<PathBuf>,
    route: Option<PathBuf>,
    layout: Option<PathBuf>,
    loading: Option<PathBuf>,
    error: Option<PathBuf>,
    not_found: Option<PathBuf>,
    directories: Vec<PathBuf>,
}

fn visit(
    app_root: &Path,
    directory: &Path,
    state: &mut WalkState,
    routes: &mut Vec<RouteEntry>,
    api_routes: &mut Vec<ApiRoute>,
) -> Result<(), RouterError> {
    let scan = scan_directory(directory)?;

    if let Some(layout) = scan.layout {
        state.layouts.push(relative_path(app_root, &layout));
    }
    if let Some(loading) = scan.loading {
        state.loading = Some(relative_path(app_root, &loading));
    }
    if let Some(error) = scan.error {
        state.error = Some(relative_path(app_root, &error));
    }
    if let Some(not_found) = scan.not_found {
        state.not_found = Some(relative_path(app_root, &not_found));
    }

    let pattern = RoutePattern::new(state.segments.clone())?;

    if let Some(page) = scan.page {
        routes.push(RouteEntry {
            pattern: pattern.clone(),
            page: relative_path(app_root, &page),
            layouts: state.layouts.clone(),
            loading: state.loading.clone(),
            error: state.error.clone(),
            not_found: state.not_found.clone(),
        });
    }

    if let Some(route) = scan.route {
        api_routes.push(ApiRoute {
            pattern: pattern.clone(),
            route: relative_path(app_root, &route),
        });
    }

    if state
        .segments
        .last()
        .is_some_and(RouteSegment::is_catch_all)
    {
        if scan
            .directories
            .iter()
            .any(|directory| !is_ignored_directory(directory))
        {
            return Err(RouterError::invalid_route(
                directory,
                "a catch-all route cannot contain child route directories",
            ));
        }
        return Ok(());
    }

    for child in scan.directories {
        let Some(name) = child.file_name().and_then(|name| name.to_str()) else {
            return Err(RouterError::invalid_route(
                &child,
                "route directory names must be valid UTF-8",
            ));
        };

        let mut child_state = state.clone();
        match directory_segment(name, &child)? {
            DirectoryKind::Ignore => continue,
            DirectoryKind::Group => {}
            DirectoryKind::Segment(segment) => child_state.segments.push(segment),
        }
        visit(app_root, &child, &mut child_state, routes, api_routes)?;
    }

    Ok(())
}

fn scan_directory(directory: &Path) -> Result<DirectoryScan, RouterError> {
    let mut scan = DirectoryScan::default();
    let mut entries = fs::read_dir(directory)
        .map_err(|error| RouterError::io(directory, &error))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| RouterError::io(directory, &error))?;
    entries.sort_by_key(|entry| entry.file_name());

    for entry in entries {
        let path = entry.path();
        let file_type = entry
            .file_type()
            .map_err(|error| RouterError::io(&path, &error))?;

        if file_type.is_dir() {
            scan.directories.push(path);
            continue;
        }
        if !file_type.is_file() || path.extension().and_then(|value| value.to_str()) != Some("rs") {
            continue;
        }

        match path.file_stem().and_then(|value| value.to_str()) {
            Some("page") => scan.page = Some(path),
            Some("route") => scan.route = Some(path),
            Some("layout") => scan.layout = Some(path),
            Some("loading") => scan.loading = Some(path),
            Some("error") => scan.error = Some(path),
            Some("not_found") => scan.not_found = Some(path),
            _ => {}
        }
    }

    Ok(scan)
}

enum DirectoryKind {
    Ignore,
    Group,
    Segment(RouteSegment),
}

fn directory_segment(name: &str, path: &Path) -> Result<DirectoryKind, RouterError> {
    if name.starts_with('_') || name.starts_with('.') {
        return Ok(DirectoryKind::Ignore);
    }

    if name.starts_with('(') {
        if !name.ends_with(')') || name.len() <= 2 {
            return Err(RouterError::invalid_route(
                path,
                "a route group must be written as `(name)`",
            ));
        }
        let group = &name[1..name.len() - 1];
        if group.contains(['(', ')', '/', '[', ']']) {
            return Err(RouterError::invalid_route(
                path,
                "a route group name cannot contain route delimiters",
            ));
        }
        return Ok(DirectoryKind::Group);
    }

    if name.starts_with("[[") {
        return Err(RouterError::invalid_route(
            path,
            "optional catch-all routes are not supported",
        ));
    }

    if let Some(inner) = name
        .strip_prefix("[...")
        .and_then(|value| value.strip_suffix(']'))
    {
        let segment = RouteSegment::catch_all(inner)
            .map_err(|error| RouterError::invalid_route(path, error.to_string()))?;
        return Ok(DirectoryKind::Segment(segment));
    }

    if let Some(inner) = name
        .strip_prefix('[')
        .and_then(|value| value.strip_suffix(']'))
    {
        let segment = RouteSegment::dynamic(inner)
            .map_err(|error| RouterError::invalid_route(path, error.to_string()))?;
        return Ok(DirectoryKind::Segment(segment));
    }

    if name.contains(['[', ']', '(', ')']) {
        return Err(RouterError::invalid_route(
            path,
            "dynamic segments must be written as `[name]` or `[...name]`",
        ));
    }

    let segment = RouteSegment::static_segment(name)
        .map_err(|error| RouterError::invalid_route(path, error.to_string()))?;
    Ok(DirectoryKind::Segment(segment))
}

fn is_ignored_directory(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.starts_with('_') || name.starts_with('.'))
}

fn relative_path(app_root: &Path, path: &Path) -> PathBuf {
    path.strip_prefix(app_root).unwrap_or(path).to_path_buf()
}
