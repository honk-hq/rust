//! The official Rust client for [Honk](https://honk-honk.app): send events from your apps, jobs,
//! scripts and CI to your phone, as calm, grouped push notifications.
//!
//! - Async-first on Tokio and `reqwest` with rustls (no OpenSSL); a synchronous
//!   `blocking::Honk` with the feature `blocking`.
//! - Every send carries an `Idempotency-Key` (yours, or a UUIDv7) that is reused on every
//!   retry, so a retry never creates a duplicate.
//! - Retries only network errors, timeouts, 429 and 5xx, with exponential backoff and full
//!   jitter, never sooner than `Retry-After`, all under a total deadline (30 s). Redirects are
//!   reported, never followed.
//! - Heartbeats: [`Honk::check_in`] tells Honk a job or host is alive, and Honk tells you when
//!   it goes quiet.
//! - A thin wrapper: Honk delivers first. The server corrects what it can't use as sent (a
//!   title too long, an invalid URL, `urgent` from a key without it, …) and lists each
//!   correction in [`Accepted::warnings`]; [`HonkBuilder::on_warnings`] logs them. The client
//!   only checks that a message has a text or a title.
//!
//! Keep the ingestion key (`honk_…`) on the server: never ship it inside a client app.
//!
//! # Quick start
//!
//! ```no_run
//! use honk::{Honk, Message, Severity};
//!
//! #[tokio::main]
//! async fn main() -> honk::Result<()> {
//!     let honk = Honk::from_env()?; // HONK_URL, HONK_KEY
//!
//!     // The Honk scale: light, beep, loud, long, blast.
//!     honk.beep("Backup finished", "nightly pg_dump took 42 s").await?;
//!
//!     // Every field, and your own idempotency key.
//!     let msg = Message::new("Maria asked for a quote: 3 rooms, 2 bathrooms")
//!         .title("New quote request")
//!         .severity(Severity::LOUD)
//!         .group_key("requests/4812")
//!         .url("https://shop.example.com/admin/requests/4812")
//!         .metadata("rooms", 3);
//!     let accepted = honk.honk(&msg, "request-4812").await?;
//!     println!("stored as {}", accepted.id);
//!
//!     // Incidents: uh-oh opens one for its group, phew closes it.
//!     honk.uhoh("db/backup", "Backup failed", "pg_dump exited with 1").await?;
//!     honk.phew("db/backup", "Backup OK", "pg_dump finished").await?;
//!     Ok(())
//! }
//! ```
//!
//! # Heartbeats
//!
//! A heartbeat expects a check-in at least every interval. When one is late by more than the
//! grace period, Honk opens a problem in the project; the next check-in recovers it.
//!
//! ```no_run
//! # async fn demo(honk: honk::Honk) -> honk::Result<()> {
//! use std::time::Duration;
//!
//! // The first check-in with an interval creates the heartbeat "nightly-backup".
//! honk.check_in("nightly-backup")
//!     .name("Nightly backup")
//!     .interval(Duration::from_secs(24 * 3600))
//!     .grace(Duration::from_secs(3600))
//!     .await?;
//!
//! // Or report a failure right away.
//! honk.check_in("nightly-backup").fail().message("pg_dump exited with 1").await?;
//! # Ok(())
//! # }
//! ```
//!
//! [`Honk::send`], [`Honk::problem`] and [`Honk::recovery`] are the same methods as
//! [`Honk::honk`], [`Honk::uhoh`] and [`Honk::phew`], under their older names.
//!
//! # Warnings
//!
//! ```no_run
//! # async fn demo(honk: honk::Honk) -> honk::Result<()> {
//! let accepted = honk.loud("t".repeat(300), "the server shortens the title").await?;
//! for w in &accepted.warnings {
//!     eprintln!("{w}"); // "title truncated: …"
//! }
//! # Ok(())
//! # }
//! ```
//!
//! # Errors
//!
//! Every failure is an [`Error`]: `Validation` (fix the message), `Auth`, `Quota`, `Conflict`,
//! `Network`, `Timeout`, `Server` or `Http`, each with a [`Failure`] holding the details: the
//! HTTP status, the server's code and message, every invalid field and the request id, all in
//! its `Display`. [`Error::is_retryable`] tells whether sending again later (with the same
//! idempotency key) may succeed.
//!
//! # Features
//!
//! - `blocking`: `blocking::Honk`, a synchronous client on a private Tokio runtime.

#![forbid(unsafe_code)]
#![warn(missing_docs, rust_2018_idioms, unreachable_pub)]
#![cfg_attr(docsrs, feature(doc_cfg))]

#[cfg(feature = "blocking")]
#[cfg_attr(docsrs, doc(cfg(feature = "blocking")))]
pub mod blocking;
mod client;
mod encode;
mod error;
mod heartbeat;
mod message;
mod time;
mod uuid;

pub use client::{Honk, HonkBuilder, PendingCheckIn, PendingSend, VERSION};
pub use encode::encode_message;
pub use error::{Error, Failure, FieldError, Result};
pub use heartbeat::{CheckedIn, HeartbeatStatus};
pub use message::{
    Accepted, Action, Category, Defaults, EventType, Message, MetadataValue, ParseEnumError,
    Priority, Severity, Warning,
};
pub use uuid::new_idempotency_key;
