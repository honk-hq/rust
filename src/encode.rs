use std::collections::BTreeMap;

use serde::Serialize;

use crate::error::{Error, FieldError, Result};
use crate::message::{
    Action, Category, Defaults, EventType, Message, MetadataValue, Priority, Severity,
};

/// The exact JSON of `contracts/openapi.yaml` `MessageRequest`.
#[derive(Serialize)]
struct Wire<'a> {
    #[serde(skip_serializing_if = "Option::is_none")]
    title: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    message: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    severity: Option<Severity>,
    #[serde(skip_serializing_if = "Option::is_none")]
    priority: Option<Priority>,
    #[serde(skip_serializing_if = "Option::is_none")]
    category: Option<Category>,
    #[serde(skip_serializing_if = "Option::is_none")]
    source: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    environment: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    channel: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    group_key: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    event_type: Option<EventType>,
    #[serde(skip_serializing_if = "Option::is_none")]
    occurred_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    url: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    image_url: Option<&'a str>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    actions: &'a Vec<Action>,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    metadata: &'a BTreeMap<String, MetadataValue>,
    #[serde(skip_serializing_if = "Option::is_none")]
    ttl_seconds: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    source_sequence: Option<u64>,
}

/// An empty string counts as unset (like `omitempty` in the Go SDK).
fn set(v: &Option<String>) -> Option<&str> {
    v.as_deref().filter(|s| !s.is_empty())
}

fn with_default<'a>(v: &'a Option<String>, d: &'a Option<String>) -> Option<&'a str> {
    set(v).or_else(|| set(d))
}

/// Returns the exact JSON body that [`Honk::send`](crate::Honk::send) posts for `message`, with
/// `defaults` applied. Useful for logging, dry runs and tests.
///
/// The only local check is that the message has a text or a title; everything else is the
/// server's to correct (see [`Accepted::warnings`](crate::Accepted::warnings)).
///
/// ```
/// use honk::{encode_message, Defaults, Message, Severity};
///
/// let body = encode_message(&Message::new("Backup finished").severity(Severity::BEEP), &Defaults::new().source("nightly"))?;
/// assert_eq!(body, r#"{"message":"Backup finished","severity":"success","source":"nightly"}"#);
/// # Ok::<(), honk::Error>(())
/// ```
pub fn encode_message(message: &Message, defaults: &Defaults) -> Result<String> {
    let blank = |s: Option<&str>| s.is_none_or(|s| s.trim().is_empty());
    if blank(Some(&message.message)) && blank(message.title.as_deref()) {
        return Err(Error::local_validation(vec![FieldError::new(
            "message",
            "required",
            "a message or a title is required",
        )]));
    }
    let wire = Wire {
        title: set(&message.title),
        message: Some(message.message.as_str()).filter(|s| !s.is_empty()),
        severity: message.severity,
        priority: message.priority,
        category: message.category,
        source: with_default(&message.source, &defaults.source),
        environment: with_default(&message.environment, &defaults.environment),
        channel: with_default(&message.channel, &defaults.channel),
        group_key: set(&message.group_key),
        event_type: message.event_type,
        occurred_at: message.occurred_at.map(crate::time::format_rfc3339),
        url: set(&message.url),
        image_url: set(&message.image_url),
        actions: &message.actions,
        metadata: &message.metadata,
        ttl_seconds: message.ttl_seconds,
        source_sequence: message.source_sequence,
    };
    serde_json::to_string(&wire).map_err(|e| {
        Error::local_validation(vec![FieldError::new(
            "body",
            "invalid_format",
            e.to_string(),
        )])
    })
}

/// Whether `k` can be sent as a header value at all: no control characters. The server's rule
/// (1–128 printable ASCII characters) is the server's to enforce (`422`, field
/// `Idempotency-Key`).
pub(crate) fn sendable_idempotency_key(k: &str) -> bool {
    !k.chars().any(char::is_control)
}
