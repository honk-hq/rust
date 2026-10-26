use std::collections::BTreeMap;
use std::fmt;
use std::str::FromStr;
use std::time::SystemTime;

use serde::{Deserialize, Serialize};

macro_rules! wire_enum {
    ($(#[$meta:meta])* $name:ident { $($(#[$vmeta:meta])* $variant:ident = $wire:literal),+ $(,)? }) => {
        $(#[$meta])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
        #[serde(rename_all = "lowercase")]
        pub enum $name {
            $($(#[$vmeta])* $variant),+
        }

        impl $name {
            /// Every value, in the API's order.
            pub const ALL: &'static [$name] = &[$($name::$variant),+];

            /// The value as sent on the wire.
            pub fn as_str(self) -> &'static str {
                match self {
                    $($name::$variant => $wire),+
                }
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(self.as_str())
            }
        }
    };
}

wire_enum! {
    /// Severity (the level), lowest to highest; it alone decides how a push interrupts. Every
    /// severity has a horn name on the Honk scale: [`Severity::LIGHT`],
    /// [`Severity::BEEP`], [`Severity::LOUD`], [`Severity::LONG`], [`Severity::BLAST`].
    ///
    /// ```
    /// use honk::Severity;
    /// assert_eq!(Severity::LOUD, Severity::Warning);
    /// assert_eq!("Loud".parse::<Severity>().unwrap(), Severity::Warning);
    /// assert_eq!(Severity::BLAST.to_string(), "critical"); // always sent canonical
    /// ```
    Severity {
        /// `info` (Light). The default.
        Info = "info",
        /// `success` (Beep).
        Success = "success",
        /// `warning` (Loud).
        Warning = "warning",
        /// `error` (Long).
        Error = "error",
        /// `critical` (Blast).
        Critical = "critical",
    }
}

impl Severity {
    /// Light: an alias of [`Severity::Info`].
    pub const LIGHT: Severity = Severity::Info;
    /// Beep: an alias of [`Severity::Success`].
    pub const BEEP: Severity = Severity::Success;
    /// Loud: an alias of [`Severity::Warning`].
    pub const LOUD: Severity = Severity::Warning;
    /// Long: an alias of [`Severity::Error`].
    pub const LONG: Severity = Severity::Error;
    /// Blast: an alias of [`Severity::Critical`].
    pub const BLAST: Severity = Severity::Critical;

    /// The horn name: `light`, `beep`, `loud`, `long` or `blast`.
    pub fn horn(self) -> &'static str {
        match self {
            Severity::Info => "light",
            Severity::Success => "beep",
            Severity::Warning => "loud",
            Severity::Error => "long",
            Severity::Critical => "blast",
        }
    }

    /// Parses a canonical value or horn name, case-insensitively (`"LOUD"` and `"warning"` are
    /// [`Severity::Warning`]).
    pub fn parse(value: &str) -> Option<Severity> {
        let v = value.trim().to_ascii_lowercase();
        Severity::ALL
            .iter()
            .copied()
            .find(|s| s.as_str() == v || s.horn() == v)
    }
}

/// The error of [`Severity::from_str`] and the other enums' `from_str`: names the valid values.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseEnumError(pub(crate) String);

impl fmt::Display for ParseEnumError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for ParseEnumError {}

impl FromStr for Severity {
    type Err = ParseEnumError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Severity::parse(s).ok_or_else(|| {
            ParseEnumError(format!(
                "unknown severity {s:?}: use light (info), beep (success), loud (warning), long (error) or blast (critical)"
            ))
        })
    }
}

wire_enum! {
    /// The priority declared by the source. The level ([`Severity`]) decides how a push
    /// interrupts. `Urgent` is kept only from an ingestion key with `allow_urgent`; from any
    /// other key the server lowers it to `High` with a `downgraded` warning.
    Priority {
        /// `low`.
        Low = "low",
        /// `normal`. The default.
        Normal = "normal",
        /// `high`.
        High = "high",
        /// `urgent`: may break through Focus on the phone.
        Urgent = "urgent",
    }
}

wire_enum! {
    /// A problem opens an incident for its group, a recovery closes it. Without a group key a
    /// message groups by its title.
    EventType {
        /// `event`. The default.
        Event = "event",
        /// `problem`: opens or continues the group's incident.
        Problem = "problem",
        /// `recovery`: closes the group's open incident.
        Recovery = "recovery",
    }
}

wire_enum! {
    /// The category (taxonomy v1).
    Category {
        /// `infrastructure`.
        Infrastructure = "infrastructure",
        /// `security`.
        Security = "security",
        /// `backups`.
        Backups = "backups",
        /// `deployments`.
        Deployments = "deployments",
        /// `payments`.
        Payments = "payments",
        /// `customers`.
        Customers = "customers",
        /// `sales`.
        Sales = "sales",
        /// `automation`.
        Automation = "automation",
        /// `personal`.
        Personal = "personal",
        /// `other`.
        Other = "other",
    }
}

macro_rules! from_str_enum {
    ($name:ident, $what:literal) => {
        impl FromStr for $name {
            type Err = ParseEnumError;

            fn from_str(s: &str) -> Result<Self, Self::Err> {
                let v = s.trim().to_ascii_lowercase();
                $name::ALL
                    .iter()
                    .copied()
                    .find(|x| x.as_str() == v)
                    .ok_or_else(|| {
                        let names: Vec<&str> = $name::ALL.iter().map(|x| x.as_str()).collect();
                        ParseEnumError(format!(
                            "unknown {} {s:?}: use one of {}",
                            $what,
                            names.join(", ")
                        ))
                    })
            }
        }
    };
}

from_str_enum!(Priority, "priority");
from_str_enum!(EventType, "event type");
from_str_enum!(Category, "category");

/// A metadata value: a string, a number or a boolean.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(untagged)]
pub enum MetadataValue {
    /// A string.
    String(String),
    /// An integer.
    Integer(i64),
    /// A finite number.
    Float(f64),
    /// A boolean.
    Bool(bool),
}

macro_rules! metadata_from {
    ($($t:ty => $variant:ident $(as $cast:ty)?),+ $(,)?) => {
        $(impl From<$t> for MetadataValue {
            fn from(v: $t) -> Self {
                MetadataValue::$variant(v $(as $cast)?)
            }
        })+
    };
}

metadata_from! {
    i64 => Integer, i32 => Integer as i64, i16 => Integer as i64, i8 => Integer as i64,
    u32 => Integer as i64, u16 => Integer as i64, u8 => Integer as i64,
    f64 => Float, f32 => Float as f64, bool => Bool, String => String,
}

impl From<&str> for MetadataValue {
    fn from(v: &str) -> Self {
        MetadataValue::String(v.to_owned())
    }
}

impl From<&String> for MetadataValue {
    fn from(v: &String) -> Self {
        MetadataValue::String(v.clone())
    }
}

/// A button on a message. Honk never opens the URL; the phone does, when you tap the button.
///
/// ```
/// use honk::{Action, Message};
///
/// let msg = Message::new("Maria Tudor asked for a quote")
///     .action("Reply", "mailto:maria@example.com?subject=Your%20quote")
///     .action("Call", "tel:+15550134");
/// assert_eq!(msg.actions[1], Action::new("Call", "tel:+15550134"));
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize)]
#[non_exhaustive]
pub struct Action {
    /// Up to 40 characters, one line, shown as sent.
    pub title: String,
    /// `https://`, `mailto:`, `tel:` or `sms:`. An action the server can't use is left out
    /// with an `ignored` warning; the others are kept.
    pub url: String,
}

impl Action {
    /// A button with this title and URL.
    pub fn new(title: impl Into<String>, url: impl Into<String>) -> Self {
        Action {
            title: title.into(),
            url: url.into(),
        }
    }
}

/// One event for `POST /v1/messages`. A message text or a title is required; unset fields are
/// omitted so the server defaults apply (severity info, priority normal, source `api`,
/// environment `default`, channel `general`, event type event, TTL 3600 s). Empty strings
/// count as unset.
///
/// Honk delivers first: values it can't use as sent (too long, a wrong type, an invalid URL or
/// action, …) are corrected or left out, and the answer lists each correction in
/// [`Accepted::warnings`].
///
/// ```
/// use honk::{Category, Message, Priority, Severity};
///
/// let msg = Message::new("db-1 /var is at 91%")
///     .title("Disk 91% full")
///     .severity(Severity::LOUD)
///     .priority(Priority::High)
///     .category(Category::Infrastructure)
///     .group_key("disk/db-1/var")
///     .metadata("host", "db-1")
///     .metadata("used_percent", 91);
/// assert_eq!(msg.severity, Some(Severity::Warning));
/// ```
#[derive(Debug, Clone, PartialEq, Default)]
#[non_exhaustive]
pub struct Message {
    /// The text: plain, up to 8192 bytes of UTF-8. Line breaks and tabs are allowed. May be
    /// empty when a title is set.
    pub message: String,
    /// One line, up to 160 characters. Defaults to the first line of the message.
    pub title: Option<String>,
    /// Default info (light).
    pub severity: Option<Severity>,
    /// Default normal.
    pub priority: Option<Priority>,
    /// The category.
    pub category: Option<Category>,
    /// Defaults to the client's default, then `api`.
    pub source: Option<String>,
    /// Defaults to the client's default, then `default`.
    pub environment: Option<String>,
    /// Defaults to the client's default, then `general`.
    pub channel: Option<String>,
    /// Optional. Messages with the same key (per environment, source and channel) form one
    /// group: the first one pushes, repeats update it calmly. Without one a message groups by
    /// its title. Use one key per customer request (`requests/<id>`), a shared key only for
    /// repeats of the same problem.
    pub group_key: Option<String>,
    /// Default event.
    pub event_type: Option<EventType>,
    /// When it happened at the source (informational).
    pub occurred_at: Option<SystemTime>,
    /// An https link shown as "Open link".
    pub url: Option<String>,
    /// An https image the server fetches after ingestion.
    pub image_url: Option<String>,
    /// Up to 3 buttons, in display order (the first is the primary). Empty: none.
    pub actions: Vec<Action>,
    /// Strings, numbers or booleans, up to 16 keys.
    pub metadata: BTreeMap<String, MetadataValue>,
    /// The push lifetime, 60–86400 seconds. Default 3600.
    pub ttl_seconds: Option<u32>,
    /// A monotonic counter per source stream (0 … 2^53-1), so a delayed recovery can never
    /// close a newer problem.
    pub source_sequence: Option<u64>,
}

impl Message {
    /// A message with this text and nothing else set. For a title-only message use
    /// `Message::default().title("…")`.
    pub fn new(message: impl Into<String>) -> Self {
        Message {
            message: message.into(),
            ..Message::default()
        }
    }

    /// Sets the title.
    pub fn title(mut self, title: impl Into<String>) -> Self {
        self.title = Some(title.into());
        self
    }

    /// Sets the severity (a horn alias like [`Severity::LOUD`] or a canonical value).
    pub fn severity(mut self, severity: Severity) -> Self {
        self.severity = Some(severity);
        self
    }

    /// Sets the priority.
    pub fn priority(mut self, priority: Priority) -> Self {
        self.priority = Some(priority);
        self
    }

    /// Sets the category.
    pub fn category(mut self, category: Category) -> Self {
        self.category = Some(category);
        self
    }

    /// Sets the source.
    pub fn source(mut self, source: impl Into<String>) -> Self {
        self.source = Some(source.into());
        self
    }

    /// Sets the environment.
    pub fn environment(mut self, environment: impl Into<String>) -> Self {
        self.environment = Some(environment.into());
        self
    }

    /// Sets the channel.
    pub fn channel(mut self, channel: impl Into<String>) -> Self {
        self.channel = Some(channel.into());
        self
    }

    /// Sets the group key, e.g. `requests/4812`.
    pub fn group_key(mut self, group_key: impl Into<String>) -> Self {
        self.group_key = Some(group_key.into());
        self
    }

    /// Sets the event type.
    pub fn event_type(mut self, event_type: EventType) -> Self {
        self.event_type = Some(event_type);
        self
    }

    /// Sets when the event happened at the source.
    pub fn occurred_at(mut self, occurred_at: SystemTime) -> Self {
        self.occurred_at = Some(occurred_at);
        self
    }

    /// Sets the https link shown as "Open link".
    pub fn url(mut self, url: impl Into<String>) -> Self {
        self.url = Some(url.into());
        self
    }

    /// Sets an https image the server fetches after ingestion.
    pub fn image_url(mut self, image_url: impl Into<String>) -> Self {
        self.image_url = Some(image_url.into());
        self
    }

    /// Adds a button (up to 3, in display order), e.g. `.action("Call", "tel:+15550134")`.
    pub fn action(mut self, title: impl Into<String>, url: impl Into<String>) -> Self {
        self.actions.push(Action::new(title, url));
        self
    }

    /// Adds one metadata entry (a string, number or boolean).
    pub fn metadata(mut self, key: impl Into<String>, value: impl Into<MetadataValue>) -> Self {
        self.metadata.insert(key.into(), value.into());
        self
    }

    /// Sets the push lifetime in seconds (60–86400; the server clamps other values).
    pub fn ttl_seconds(mut self, ttl_seconds: u32) -> Self {
        self.ttl_seconds = Some(ttl_seconds);
        self
    }

    /// Sets the monotonic source sequence.
    pub fn source_sequence(mut self, source_sequence: u64) -> Self {
        self.source_sequence = Some(source_sequence);
        self
    }
}

impl From<&str> for Message {
    fn from(message: &str) -> Self {
        Message::new(message)
    }
}

impl From<String> for Message {
    fn from(message: String) -> Self {
        Message::new(message)
    }
}

/// Values applied to every message that leaves these fields unset.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
#[non_exhaustive]
pub struct Defaults {
    /// The default source.
    pub source: Option<String>,
    /// The default environment.
    pub environment: Option<String>,
    /// The default channel.
    pub channel: Option<String>,
}

impl Defaults {
    /// No defaults.
    pub fn new() -> Self {
        Defaults::default()
    }

    /// Sets the default source.
    pub fn source(mut self, source: impl Into<String>) -> Self {
        self.source = Some(source.into());
        self
    }

    /// Sets the default environment.
    pub fn environment(mut self, environment: impl Into<String>) -> Self {
        self.environment = Some(environment.into());
        self
    }

    /// Sets the default channel.
    pub fn channel(mut self, channel: impl Into<String>) -> Self {
        self.channel = Some(channel.into());
        self
    }
}

/// The `202` answer: the message is durably stored. It does not mean pushed, delivered or read.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[non_exhaustive]
pub struct Accepted {
    /// The message id (`msg_…`); the original id when `duplicate` is true.
    pub id: String,
    /// True when this idempotency key was already accepted with the same payload in the last
    /// 24 hours (nothing new was stored).
    #[serde(default)]
    pub duplicate: bool,
    /// When the server accepted it (the first time, for a duplicate), as RFC 3339.
    #[serde(default)]
    pub received_at: String,
    /// What the server corrected to accept the message (a title shortened, an invalid URL left
    /// out, `urgent` lowered to `high`, …); empty when nothing. Not errors: the message was
    /// stored. Log them, or set [`HonkBuilder::on_warnings`](crate::HonkBuilder::on_warnings).
    #[serde(default, deserialize_with = "lenient_warnings")]
    pub warnings: Vec<Warning>,
}

/// One correction the server made to accept a message (`202 warnings[]`). Displays as
/// `field code: message`, e.g. `title truncated: shortened to 160 characters`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Deserialize)]
#[non_exhaustive]
pub struct Warning {
    /// The field it concerns: `title`, `metadata.order`, `actions[1].url`, …
    #[serde(default)]
    pub field: String,
    /// `truncated`, `normalized`, `ignored`, `clamped`, `downgraded` or `hashed` (kept as sent,
    /// so codes added later come through too).
    #[serde(default)]
    pub code: String,
    /// What was wrong and what was done (English, for logs).
    #[serde(default)]
    pub message: String,
}

impl fmt::Display for Warning {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} {}", self.field, self.code)?;
        if !self.message.is_empty() {
            write!(f, ": {}", self.message)?;
        }
        Ok(())
    }
}

/// `warnings` that are missing, `null` or unreadable are no warnings: they never fail a send
/// that the server accepted.
fn lenient_warnings<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Vec<Warning>, D::Error> {
    let value = serde_json::Value::deserialize(d)?;
    Ok(serde_json::from_value(value).unwrap_or_default())
}

impl Accepted {
    /// Whether the server corrected anything ([`warnings`](Accepted::warnings) is not empty).
    pub fn has_warnings(&self) -> bool {
        !self.warnings.is_empty()
    }

    /// [`received_at`](Accepted::received_at) as a [`SystemTime`], when it parses.
    pub fn received_at_time(&self) -> Option<SystemTime> {
        crate::time::parse_rfc3339(&self.received_at)
    }
}
