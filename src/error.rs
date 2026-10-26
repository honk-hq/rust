use std::fmt;
use std::time::Duration;

/// A `Result` whose error is [`honk::Error`](Error).
pub type Result<T, E = Error> = std::result::Result<T, E>;

/// Everything that can go wrong. Every variant except
/// [`InvalidConfiguration`](Error::InvalidConfiguration) carries a [`Failure`] with the details
/// (status, API error code, field errors, the idempotency key that was used, attempts, …).
///
/// ```
/// # async fn demo(honk: honk::Honk, msg: honk::Message) {
/// use honk::Error;
///
/// match honk.send(&msg, "order-1042-failed").await {
///     Ok(accepted) => println!("stored as {}", accepted.id),
///     Err(Error::Validation(f)) => eprintln!("a bug, don't retry: {:?}", f.fields),
///     Err(e) if e.is_retryable() => {
///         // Queue it and retry later with the same key: Honk deduplicates.
///         let (key, after) = (e.idempotency_key(), e.retry_after());
///         # let _ = (key, after);
///     }
///     Err(e) => eprintln!("{e}"),
/// }
/// # }
/// ```
#[derive(Debug)]
#[non_exhaustive]
pub enum Error {
    /// The client is misconfigured: a missing or malformed URL or key (from
    /// [`Honk::new`](crate::Honk::new), the builder or `from_env`).
    InvalidConfiguration(String),
    /// The server refused the request (400, 413, 415, 422): malformed JSON, a body over
    /// 16 KiB, neither message nor title, an unknown field or an invalid `event_type`.
    /// [`Failure::fields`] lists every problem. Also returned without sending anything
    /// (`failure.local`) for a message without text or title. Fix it; don't retry.
    Validation(Box<Failure>),
    /// 401 or 403: `invalid_key`, `project_suspended`, `workspace_suspended`, or `plan_limit`
    /// (a check-in that would create a heartbeat beyond the plan's limit).
    Auth(Box<Failure>),
    /// 429 after retries: `quota_exceeded` (daily, resets at UTC midnight), `rate_limited` or
    /// `overloaded`. See [`Failure::retry_after`].
    Quota(Box<Failure>),
    /// 409 `idempotency_conflict`: the key was already used with a different payload in the
    /// last 24 hours. Use a new key, or send the original payload.
    Conflict(Box<Failure>),
    /// Honk could not be reached on any attempt before the deadline.
    Network(Box<Failure>),
    /// The attempts timed out. The event may or may not have been stored; retrying later with
    /// the same idempotency key is safe.
    Timeout(Box<Failure>),
    /// 5xx on every attempt before the deadline.
    Server(Box<Failure>),
    /// Any other unexpected answer: 404 (wrong URL, or a check-in without an interval for a
    /// heartbeat Honk doesn't know), a redirect (never followed), a 2xx without an id.
    Http(Box<Failure>),
}

/// The details of a failed send.
#[derive(Debug, Default)]
#[non_exhaustive]
pub struct Failure {
    /// The HTTP status, or `None` when no answer was received.
    pub status: Option<u16>,
    /// The API error code (`invalid_key`, `quota_exceeded`, `validation_failed`, …), or
    /// `network_error` / `timeout`.
    pub code: String,
    /// A human-readable explanation (English).
    pub message: String,
    /// Every invalid field, for [`Error::Validation`].
    pub fields: Vec<FieldError>,
    /// `true` when the SDK rejected the message before sending anything.
    pub local: bool,
    /// The server's request id (`req_…`), when it answered.
    pub request_id: Option<String>,
    /// The `Idempotency-Key` that was used. Retry later with the same key to stay
    /// duplicate-free.
    pub idempotency_key: Option<String>,
    /// The number of HTTP attempts made (0 when rejected locally).
    pub attempts: u32,
    /// The server's `Retry-After`, when present.
    pub retry_after: Option<Duration>,
    pub(crate) source: Option<Box<dyn std::error::Error + Send + Sync + 'static>>,
}

/// One invalid field, from the server (`error.fields[]`) or the local "message or title" check.
/// Displays as `field code: message`.
#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize)]
#[non_exhaustive]
pub struct FieldError {
    /// The wire name: `message`, `group_key`, `metadata.region`, `Idempotency-Key`, `body`, …
    pub field: String,
    /// `required`, `too_long`, `too_short`, `invalid_enum`, `invalid_format`, `out_of_range`,
    /// `not_allowed` or `invalid_utf8`.
    pub code: String,
    /// A human-readable explanation.
    #[serde(default)]
    pub message: String,
}

impl FieldError {
    pub(crate) fn new(field: impl Into<String>, code: &str, message: impl Into<String>) -> Self {
        FieldError {
            field: field.into(),
            code: code.to_owned(),
            message: message.into(),
        }
    }
}

impl Error {
    /// The details of the failure (`None` for [`Error::InvalidConfiguration`]).
    pub fn failure(&self) -> Option<&Failure> {
        match self {
            Error::InvalidConfiguration(_) => None,
            Error::Validation(f)
            | Error::Auth(f)
            | Error::Quota(f)
            | Error::Conflict(f)
            | Error::Network(f)
            | Error::Timeout(f)
            | Error::Server(f)
            | Error::Http(f) => Some(&**f),
        }
    }

    /// Whether sending the same event again later (with the same idempotency key) may succeed:
    /// network errors, timeouts, 429 and 5xx.
    pub fn is_retryable(&self) -> bool {
        matches!(
            self,
            Error::Network(_) | Error::Timeout(_) | Error::Server(_) | Error::Quota(_)
        )
    }

    /// The HTTP status, when the server answered.
    pub fn status(&self) -> Option<u16> {
        self.failure().and_then(|f| f.status)
    }

    /// The API error code (`invalid_key`, `quota_exceeded`, …), when there is one.
    pub fn code(&self) -> Option<&str> {
        self.failure()
            .map(|f| f.code.as_str())
            .filter(|c| !c.is_empty())
    }

    /// Every invalid field (empty unless [`Error::Validation`]).
    pub fn fields(&self) -> &[FieldError] {
        self.failure().map_or(&[], |f| f.fields.as_slice())
    }

    /// The server's request id (`req_…`), when it answered: quote it when reporting a problem.
    pub fn request_id(&self) -> Option<&str> {
        self.failure().and_then(|f| f.request_id.as_deref())
    }

    /// The idempotency key that was used, to retry the same event later.
    pub fn idempotency_key(&self) -> Option<&str> {
        self.failure().and_then(|f| f.idempotency_key.as_deref())
    }

    /// The server's `Retry-After`, when present.
    pub fn retry_after(&self) -> Option<Duration> {
        self.failure().and_then(|f| f.retry_after)
    }

    pub(crate) fn local_validation(fields: Vec<FieldError>) -> Error {
        Error::Validation(Box::new(Failure {
            code: "validation_failed".into(),
            message: "invalid request".into(),
            fields,
            local: true,
            ..Failure::default()
        }))
    }
}

impl fmt::Display for FieldError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} {}", self.field, self.code)?;
        if !self.message.is_empty() {
            write!(f, ": {}", self.message)?;
        }
        Ok(())
    }
}

/// `honk: <status> <code>: <message> (<field> <code>: <message>; …) [request_id req_…]`
impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let failure = match self {
            Error::InvalidConfiguration(msg) => return write!(f, "honk: {msg}"),
            _ => self.failure().expect("every other variant has a failure"),
        };
        f.write_str("honk: ")?;
        if let Some(status) = failure.status {
            write!(f, "{status} ")?;
            if !failure.code.is_empty() {
                write!(f, "{}: ", failure.code)?;
            }
        }
        f.write_str(&failure.message)?;
        if !failure.fields.is_empty() {
            let parts: Vec<String> = failure.fields.iter().map(ToString::to_string).collect();
            write!(f, " ({})", parts.join("; "))?;
        }
        if let Some(id) = &failure.request_id {
            write!(f, " [request_id {id}]")?;
        }
        Ok(())
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.failure()
            .and_then(|f| f.source.as_deref())
            .map(|e| e as &(dyn std::error::Error + 'static))
    }
}
