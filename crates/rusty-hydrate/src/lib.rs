//! Hydration state serialization and restoration.
//!
//! A [`HydrationSession`] owns the state shared by a server render and its
//! browser hydration. Signals created through [`HydrationSession::signal`] are
//! recorded on the server and restored on the client using stable keys.
//!
//! Server rendering embeds the serialized values in a
//! `<script id="__RUSTY_STATE__" type="application/json">` element. The script
//! must be placed outside the hydrated container.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};
use std::error::Error;
use std::fmt;
use std::rc::Rc;

use rusty_reactivity::Signal;
use serde::de::DeserializeOwned;
use serde::Serialize;
use serde_json::Value;

/// The DOM id used for the serialized hydration state script.
pub const HYDRATION_STATE_ID: &str = "__RUSTY_STATE__";

type StateReader = Rc<dyn Fn() -> Result<Value, HydrationError>>;

/// An error from hydration state setup or serialization.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum HydrationError {
    /// The serialized state was not valid JSON.
    InvalidJson(String),
    /// The document did not contain a hydration state script.
    StateScriptMissing,
    /// A restored value could not be decoded for its signal.
    InvalidValue {
        /// The signal key.
        key: String,
        /// The decoder error.
        message: String,
    },
    /// The same signal key was used more than once.
    DuplicateKey(String),
    /// A signal value could not be serialized.
    Serialization {
        /// The signal key, or `<state>` for the whole document.
        key: String,
        /// The serializer error.
        message: String,
    },
    /// State was requested from a client session for server rendering.
    NotServer,
}

impl fmt::Display for HydrationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidJson(message) => write!(formatter, "invalid hydration JSON: {message}"),
            Self::StateScriptMissing => {
                formatter.write_str("the hydration state script is missing")
            }
            Self::InvalidValue { key, message } => {
                write!(formatter, "invalid hydrated value for `{key}`: {message}")
            }
            Self::DuplicateKey(key) => {
                write!(
                    formatter,
                    "hydration signal `{key}` was created more than once"
                )
            }
            Self::Serialization { key, message } => {
                write!(
                    formatter,
                    "failed to serialize hydration state `{key}`: {message}"
                )
            }
            Self::NotServer => {
                formatter.write_str("hydration state can only be serialized by a server session")
            }
        }
    }
}

impl Error for HydrationError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Mode {
    Server,
    Client,
}

/// State shared between a server render and its browser hydration.
///
/// ```
/// use rusty_hydrate::HydrationSession;
///
/// let server = HydrationSession::server();
/// let count = server.signal("count", 1_u32).expect("server state");
/// count.set(7);
/// let state = server.state_json().expect("serialize");
///
/// let client = HydrationSession::from_json(&state).expect("restore");
/// let restored = client.signal("count", 0_u32).expect("client state");
/// assert_eq!(restored.get(), 7);
/// ```
pub struct HydrationSession {
    mode: Mode,
    restored: BTreeMap<String, Value>,
    entries: RefCell<BTreeMap<String, StateReader>>,
    used_keys: RefCell<BTreeSet<String>>,
}

impl HydrationSession {
    /// Creates an empty server session.
    #[must_use]
    pub fn server() -> Self {
        Self::new(Mode::Server, BTreeMap::new())
    }

    /// Creates an empty client session.
    ///
    /// Signals use their default values until state is supplied with
    /// [`HydrationSession::from_json`].
    #[must_use]
    pub fn client() -> Self {
        Self::new(Mode::Client, BTreeMap::new())
    }

    /// Restores a client session from serialized JSON.
    ///
    /// # Errors
    ///
    /// Returns [`HydrationError::InvalidJson`] when `json` is not an object.
    pub fn from_json(json: &str) -> Result<Self, HydrationError> {
        let restored: BTreeMap<String, Value> = serde_json::from_str(json)
            .map_err(|error| HydrationError::InvalidJson(error.to_string()))?;
        Ok(Self::new(Mode::Client, restored))
    }

    /// Runs `f` with the signal's value and records it for serialization.
    ///
    /// On the client, a restored value replaces `default`. Missing keys use the
    /// default value. Each key may be used once.
    ///
    /// # Errors
    ///
    /// Returns [`HydrationError`] for duplicate keys or an invalid restored
    /// value.
    pub fn signal<T>(&self, key: &str, default: T) -> Result<Signal<T>, HydrationError>
    where
        T: Serialize + DeserializeOwned + 'static,
    {
        if !self.used_keys.borrow_mut().insert(key.to_owned()) {
            return Err(HydrationError::DuplicateKey(key.to_owned()));
        }

        let value = match self.restored.get(key) {
            Some(value) => serde_json::from_value(value.clone()).map_err(|error| {
                HydrationError::InvalidValue {
                    key: key.to_owned(),
                    message: error.to_string(),
                }
            })?,
            None => default,
        };
        let signal = Signal::new(value);

        if self.mode == Mode::Server {
            let reader_signal = signal.clone();
            let reader_key = key.to_owned();
            let entry: StateReader = Rc::new(move || {
                reader_signal.with_untracked(|value| {
                    serde_json::to_value(value).map_err(|error| HydrationError::Serialization {
                        key: reader_key.clone(),
                        message: error.to_string(),
                    })
                })
            });
            self.entries.borrow_mut().insert(key.to_owned(), entry);
        }

        Ok(signal)
    }

    /// Returns `true` for a server session.
    #[must_use]
    pub const fn is_server(&self) -> bool {
        matches!(self.mode, Mode::Server)
    }

    /// Serializes the current server signal values as JSON.
    ///
    /// `<` is escaped so the result is safe inside a script element.
    ///
    /// # Errors
    ///
    /// Returns [`HydrationError::NotServer`] for a client session or
    /// [`HydrationError::Serialization`] when a value cannot be encoded.
    pub fn state_json(&self) -> Result<String, HydrationError> {
        if self.mode != Mode::Server {
            return Err(HydrationError::NotServer);
        }

        let entries = self.entries.borrow();
        let mut values = serde_json::Map::new();

        for (key, reader) in entries.iter() {
            values.insert(key.clone(), reader()?);
        }

        serde_json::to_string(&Value::Object(values))
            .map(|json| json.replace('<', "\\u003c"))
            .map_err(|error| HydrationError::Serialization {
                key: String::from("<state>"),
                message: error.to_string(),
            })
    }

    /// Returns a complete state script for embedding in a document.
    ///
    /// # Errors
    ///
    /// Returns the same errors as [`HydrationSession::state_json`].
    pub fn state_script(&self) -> Result<String, HydrationError> {
        let json = self.state_json()?;
        Ok(format!(
            "<script id=\"{HYDRATION_STATE_ID}\" type=\"application/json\">{json}</script>"
        ))
    }

    fn new(mode: Mode, restored: BTreeMap<String, Value>) -> Self {
        Self {
            mode,
            restored,
            entries: RefCell::new(BTreeMap::new()),
            used_keys: RefCell::new(BTreeSet::new()),
        }
    }
}

#[cfg(target_arch = "wasm32")]
impl HydrationSession {
    /// Restores a session from the current browser document.
    ///
    /// # Errors
    ///
    /// Returns [`HydrationError::StateScriptMissing`] when the state script is
    /// not present, or [`HydrationError::InvalidJson`] when its contents are
    /// invalid.
    pub fn from_document() -> Result<Self, HydrationError> {
        from_document()
    }
}

/// Restores the hydration session from the current browser document.
///
/// # Errors
///
/// Returns [`HydrationError::StateScriptMissing`] when the state script is not
/// present, or [`HydrationError::InvalidJson`] when its contents are invalid.
#[cfg(target_arch = "wasm32")]
pub fn from_document() -> Result<HydrationSession, HydrationError> {
    let window = web_sys::window().ok_or(HydrationError::StateScriptMissing)?;
    let document = window
        .document()
        .ok_or(HydrationError::StateScriptMissing)?;
    let script = document
        .get_element_by_id(HYDRATION_STATE_ID)
        .ok_or(HydrationError::StateScriptMissing)?;
    let json = script.text_content().unwrap_or_default();
    HydrationSession::from_json(&json)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serializes_current_server_values_and_restores_them() {
        let session = HydrationSession::server();
        let count = session.signal("count", 1_u32).expect("signal");
        let name = session
            .signal("name", String::from("server"))
            .expect("signal");

        count.set(9);
        name.set(String::from("Rusty"));

        let json = session.state_json().expect("serialize");
        assert_eq!(
            serde_json::from_str::<Value>(&json).expect("json"),
            serde_json::json!({ "count": 9, "name": "Rusty" })
        );

        let client = HydrationSession::from_json(&json).expect("restore");
        let restored_count = client.signal("count", 0_u32).expect("count");
        let restored_name = client
            .signal("name", String::from("default"))
            .expect("name");
        assert_eq!(restored_count.get(), 9);
        assert_eq!(restored_name.get(), "Rusty");
    }

    #[test]
    fn missing_client_values_use_defaults() {
        let client = HydrationSession::from_json("{}").expect("restore");
        let count = client.signal("count", 3_u32).expect("signal");
        assert_eq!(count.get(), 3);
    }

    #[test]
    fn rejects_duplicate_keys_and_invalid_values() {
        let session = HydrationSession::server();
        let _ = session.signal("count", 1_u32).expect("signal");
        assert!(matches!(
            session.signal("count", 2_u32),
            Err(HydrationError::DuplicateKey(key)) if key == "count"
        ));

        let client = HydrationSession::from_json(r#"{"count":"wrong"}"#).expect("restore");
        assert!(matches!(
            client.signal("count", 1_u32),
            Err(HydrationError::InvalidValue { key, .. }) if key == "count"
        ));
    }

    #[test]
    fn state_script_escapes_script_terminators() {
        let session = HydrationSession::server();
        session
            .signal("html", String::from("</script><script>alert(1)</script>"))
            .expect("signal");
        let script = session.state_script().expect("script");

        assert!(script.starts_with("<script id=\"__RUSTY_STATE__\""));
        assert!(!script.contains("</script><script>alert(1)"));
        assert!(script.contains("\\u003c"));
    }

    #[test]
    fn client_sessions_cannot_serialize() {
        assert_eq!(
            HydrationSession::client().state_json(),
            Err(HydrationError::NotServer)
        );
    }

    #[test]
    fn invalid_json_is_reported() {
        assert!(matches!(
            HydrationSession::from_json("["),
            Err(HydrationError::InvalidJson(_))
        ));
    }
}
