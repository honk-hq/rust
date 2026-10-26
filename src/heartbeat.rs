//! Heartbeat check-ins (`POST /v1/heartbeats/check-in`, contracts/API.md §11).

use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::error::{Error, FieldError, Result};

/// The state of a heartbeat after a check-in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize)]
#[serde(rename_all = "lowercase")]
#[non_exhaustive]
pub enum HeartbeatStatus {
    /// Created, never checked in.
    New,
    /// On time.
    Up,
    /// Overdue by more than the grace period; a problem is open.
    Late,
    /// The last check-in reported a failure; a problem is open.
    Failing,
    /// Paused in the app: check-ins are noted, nothing is evaluated.
    Paused,
    /// A state this version of the crate doesn't know.
    #[serde(other)]
    Unknown,
}

impl HeartbeatStatus {
    /// The value as the server sends it (`"unknown"` for [`HeartbeatStatus::Unknown`]).
    pub fn as_str(self) -> &'static str {
        match self {
            HeartbeatStatus::New => "new",
            HeartbeatStatus::Up => "up",
            HeartbeatStatus::Late => "late",
            HeartbeatStatus::Failing => "failing",
            HeartbeatStatus::Paused => "paused",
            HeartbeatStatus::Unknown => "unknown",
        }
    }
}

impl std::fmt::Display for HeartbeatStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// The `202` answer to a check-in.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[non_exhaustive]
pub struct CheckedIn {
    /// The heartbeat's id (`hbt_…`).
    pub heartbeat_id: String,
    /// The heartbeat's state after this check-in (unchanged when the server ignored it because
    /// the previous one was less than 10 s ago).
    pub status: HeartbeatStatus,
}

/// What a check-in sends; shared by the async and the blocking builders.
#[derive(Debug, Clone, Default)]
pub(crate) struct CheckInRequest {
    pub(crate) slug: String,
    pub(crate) name: Option<String>,
    pub(crate) interval: Option<Duration>,
    pub(crate) grace: Option<Duration>,
    pub(crate) failed: bool,
    pub(crate) message: Option<String>,
}

#[derive(Serialize)]
struct Wire<'a> {
    slug: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    name: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    interval_seconds: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    grace_seconds: Option<u64>,
    status: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    message: Option<&'a str>,
}

impl CheckInRequest {
    pub(crate) fn new(slug: String) -> Self {
        CheckInRequest {
            slug,
            ..CheckInRequest::default()
        }
    }

    /// Returns the JSON body. The slug is the only local check; the server judges the values.
    pub(crate) fn encode(&self) -> Result<String> {
        let wire = Wire {
            slug: &self.slug,
            name: self.name.as_deref().filter(|s| !s.is_empty()),
            interval_seconds: self.interval.map(|d| d.as_secs()),
            grace_seconds: self.grace.map(|d| d.as_secs()),
            status: if self.failed { "fail" } else { "ok" },
            message: self.message.as_deref().filter(|s| !s.is_empty()),
        };
        if wire.slug.trim().is_empty() {
            return Err(Error::local_validation(vec![FieldError::new(
                "slug",
                "required",
                "slug is required",
            )]));
        }
        serde_json::to_string(&wire).map_err(|e| {
            Error::local_validation(vec![FieldError::new(
                "body",
                "invalid_format",
                e.to_string(),
            )])
        })
    }
}

/// The setters of both `PendingCheckIn`s, which keep their request in `self.request`.
macro_rules! check_in_setters {
    () => {
        /// The heartbeat's name, shown in the app (1–100 characters, one line). Without one, a
        /// heartbeat created by this check-in is named after its slug. Sent on every check-in,
        /// so a name changed in the app is changed back.
        pub fn name(mut self, name: impl Into<String>) -> Self {
            self.request.name = Some(name.into());
            self
        }

        /// How often a check-in is expected, in whole seconds (60 s – 7 days). With an
        /// interval, the first check-in creates the heartbeat; without one, a check-in for a
        /// slug Honk doesn't know fails with a `404`. Sent on every check-in, so the interval
        /// set in the app follows the code.
        pub fn interval(mut self, interval: std::time::Duration) -> Self {
            self.request.interval = Some(interval);
            self
        }

        /// How late a check-in may be before Honk opens a problem, in whole seconds
        /// (0 – 1 day). Default 5 minutes for a heartbeat created by a check-in.
        pub fn grace(mut self, grace: std::time::Duration) -> Self {
            self.request.grace = Some(grace);
            self
        }

        /// Reports an explicit failure: Honk opens a problem right away instead of waiting for
        /// the heartbeat to go late. The next ok check-in recovers it.
        pub fn fail(self) -> Self {
            self.failed(true)
        }

        /// Reports a failure when `failed` is true, an ok check-in otherwise (the default).
        pub fn failed(mut self, failed: bool) -> Self {
            self.request.failed = failed;
            self
        }

        /// A short text (at most 1 KiB) shown in the problem of a failed check-in, or in the
        /// recovery of an ok one, e.g. the last lines of a job's output.
        pub fn message(mut self, message: impl Into<String>) -> Self {
            self.request.message = Some(message.into());
            self
        }

        /// The heartbeat's slug.
        pub fn slug(&self) -> &str {
            &self.request.slug
        }
    };
}
pub(crate) use check_in_setters;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encodes_the_contract_fields() {
        let mut r = CheckInRequest::new("nightly-backup".into());
        assert_eq!(
            r.encode().unwrap(),
            r#"{"slug":"nightly-backup","status":"ok"}"#
        );
        r.name = Some("Nightly backup".into());
        r.interval = Some(Duration::from_millis(86_400_900)); // whole seconds
        r.grace = Some(Duration::ZERO);
        r.failed = true;
        r.message = Some("pg_dump exited with 1".into());
        assert_eq!(
            r.encode().unwrap(),
            r#"{"slug":"nightly-backup","name":"Nightly backup","interval_seconds":86400,"grace_seconds":0,"status":"fail","message":"pg_dump exited with 1"}"#
        );
    }

    #[test]
    fn only_the_slug_is_checked() {
        let r = CheckInRequest {
            slug: "Not A Slug".into(),
            interval: Some(Duration::from_secs(1)),
            message: Some("x".repeat(2000)),
            ..CheckInRequest::default()
        };
        assert!(r.encode().is_ok());
        for slug in ["", "  "] {
            let r = CheckInRequest::new(slug.into());
            let Err(Error::Validation(f)) = r.encode() else {
                panic!("{slug:?}")
            };
            assert_eq!((f.fields[0].field.as_str(), f.local), ("slug", true));
        }
    }

    #[test]
    fn statuses_parse_and_unknown_ones_are_kept_apart() {
        let c: CheckedIn =
            serde_json::from_str(r#"{"heartbeat_id":"hb_1","status":"late"}"#).unwrap();
        assert_eq!(c.status, HeartbeatStatus::Late);
        assert_eq!(c.status.to_string(), "late");
        let c: CheckedIn =
            serde_json::from_str(r#"{"heartbeat_id":"hb_1","status":"sleeping"}"#).unwrap();
        assert_eq!(c.status, HeartbeatStatus::Unknown);
    }
}
