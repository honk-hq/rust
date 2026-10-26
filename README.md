# honk-honk (Rust)

[![CI](https://github.com/honk-hq/rust/actions/workflows/ci.yml/badge.svg)](https://github.com/honk-hq/rust/actions/workflows/ci.yml)
[![crates.io](https://img.shields.io/crates/v/honk-honk)](https://crates.io/crates/honk-honk)
[![docs.rs](https://img.shields.io/docsrs/honk-honk)](https://docs.rs/honk-honk)

Official Rust client for [Honk! Honk!](https://honk-honk.app), the inbox that turns events from your
apps, jobs, scripts and CI into calm, grouped push notifications on your phone.

```rust
use honk::Honk;

#[tokio::main]
async fn main() -> honk::Result<()> {
    let honk = Honk::from_env()?;

    honk.uhoh("kitchen/oven", "The cake is burning", "Oven at 260 °C and climbing").await?;
    // someone opens a window, someone else finds the oven gloves
    honk.phew("kitchen/oven", "Cake saved", "Nobody needs to know.").await?;
    Ok(())
}
```

`uhoh` opens an incident for `kitchen/oven` and `phew` closes it: one group with a happy ending
on your phone, not two unrelated alarms.

- Async-first on Tokio, `reqwest` with rustls (no OpenSSL). A synchronous client with the
  `blocking` feature.
- Retries with backoff, `Retry-After`, a total deadline and an idempotency key on every send,
  so a retry never creates a duplicate.
- Heartbeats: `check_in` from cron jobs, timers and daemons, and Honk! Honk! tells you when they
  go quiet.
- A thin wrapper: Honk! Honk! delivers first. The server corrects what it can't use as sent and
  says so in `accepted.warnings`; the client only checks that there is a message or a title.
- Rust 1.85+. `#![forbid(unsafe_code)]`.

> **Keep the key on the server.** An ingestion key (`honk_…`) lets anyone post into your
> project. Use this crate in services, jobs, CLIs and CI only; never ship it inside a desktop,
> mobile or WebAssembly front end. Read it from the environment.

## Install

```sh
cargo add honk-honk
cargo add tokio --features macros,rt-multi-thread   # if you don't have a runtime yet
```

or, for synchronous code, `cargo add honk-honk --features blocking`. The package is `honk-honk`
on crates.io (`honk` was taken); in code the library is plain `honk`: `use honk::Honk;`.

Create a project and an ingestion key at [honk-honk.app](https://honk-honk.app). Its
*Integrations* page generates ready-to-paste code.

## Quick start

```rust
use honk::Honk;

#[tokio::main]
async fn main() -> honk::Result<()> {
    let honk = Honk::from_env()?; // HONK_URL, HONK_KEY (+ HONK_SOURCE, HONK_ENVIRONMENT, HONK_CHANNEL)
    honk.beep("Backup finished", "nightly pg_dump took 42 s").await?;
    Ok(())
}
```

Or explicitly: `Honk::new("https://honk-honk.app", key)?`. A missing or malformed URL or key is
`Error::InvalidConfiguration`. `Honk` is cheap to clone: create one per application and share
it, so connections are kept alive.

## The Honk scale

Five levels. The Honk app shows them by their standard names (Info, Success, Warning, Error,
Critical); people who prefer horn sounds can switch on the Honk names (Light, Beep, Loud, Long,
Blast) with the "Honk names for levels" switch in Settings. The SDK takes both: the canonical values and the
Honk names as input aliases, in any case, and always sends the canonical value.

| Level | Honk name | Severity | Helper | Constant |
|---|---|---|---|---|
| Info | Light | `light` (info) | `honk.light(title, message)` | `Severity::LIGHT` (= `Severity::Info`) |
| Success | Beep | `beep` (success) | `honk.beep(…)` | `Severity::BEEP` |
| Warning | Loud | `loud` (warning) | `honk.loud(…)` | `Severity::LOUD` |
| Error | Long | `long` (error) | `honk.long(…)` | `Severity::LONG` |
| Critical | Blast | `blast` (critical) | `honk.blast(…)` | `Severity::BLAST` |

`"loud".parse::<Severity>()`, `"LOUD"` and `"warning"` are the same event (also for
idempotency). `long` and `blast` push at least as `high` priority. `info()`, `success()`,
`warning()`, `error()` and `critical()` remain as synonyms.

Every helper returns a `PendingSend`: chain more fields, then `.await` it.

```rust
use honk::Priority;

honk.loud("Disk 91% full", "db-1 /var is at 91%")
    .group_key("disk/db-1/var")
    .priority(Priority::High)
    .metadata("used_percent", 91)
    .await?;
```

## Recipe: notify me when a customer asks for something

Give each request its own group (`requests/<id>`) and its own idempotency key. Two different
customers never fold into one notification, and a retried webhook or job never buzzes twice.

```rust
use honk::{Category, Message, Priority, Severity};

let msg = Message::new(format!("{} asked for a quote: {}", req.name, req.summary))
    .title("New quote request")
    .severity(Severity::LOUD)
    .priority(Priority::High)
    .category(Category::Customers)
    .group_key(format!("requests/{}", req.id))
    .url(format!("https://shop.example.com/admin/requests/{}", req.id)); // https only, "Open link"
honk.honk(&msg, format!("request-{}", req.id).as_str()).await?;
```

## Uh-oh and phew: problems and recoveries

`uhoh` reports a problem and opens an incident for its group key; `phew` reports the recovery
and closes it.

```rust
honk.uhoh("db/backup", "Backup failed", "pg_dump exited with 1").await?;
honk.phew("db/backup", "Backup OK", "pg_dump finished").await?;
```

`uhoh` is a Long message (error) by default and `phew` a Beep message (success); chain `.severity(…)` to
change it. The group key is optional: pass `""` and the message groups by its title. `send`, `problem` and `recovery` still work: they are `honk`, `uhoh` and `phew` under
their 0.2 names, with no deprecation warning.

## Heartbeats

Some failures are silence: the backup that never ran, the host that stopped. A heartbeat expects
a check-in at least every interval; when one is later than the interval plus the grace period,
Honk! Honk! opens a problem in the key's project (a normal message: groups, quiet hours and
pushes apply), and the next check-in closes it.

```rust
use std::time::Duration;

// The first check-in with an interval creates the heartbeat "nightly-backup".
let c = honk.check_in("nightly-backup")
    .name("Nightly backup")                  // optional, shown in the app (default: the slug)
    .interval(Duration::from_secs(24 * 3600)) // 60 s – 7 days
    .grace(Duration::from_secs(3600))         // 0 – 1 day, default 5 min
    .await?;
println!("{} is {}", c.heartbeat_id, c.status); // hbt_… is up

// Report a failure now instead of waiting for the silence:
honk.check_in("nightly-backup").fail().message("pg_dump exited with 1").await?;
```

The slug is `[a-z0-9-]{1,64}`, unique in the project (the server checks the values; the client
only requires a slug). `interval`, `grace` and `name` are sent on
every check-in, so the code stays the source of truth; leave them off to keep what was set in the
app (the heartbeat must then exist, or the check-in is `Error::Http` with code `not_found`).
Severity and pausing are set in the app. Check-ins are retried like messages and need no
idempotency key: Honk! Honk! ignores check-ins less than 10 s apart, and a failure while a problem
is already open adds nothing.

A job run by cron or a systemd timer checks in when it is done:

```rust
// src/bin/backup.rs
use std::time::Duration;

#[tokio::main]
async fn main() -> honk::Result<()> {
    let honk = honk::Honk::from_env()?; // HONK_URL, HONK_KEY
    let check_in = honk.check_in("nightly-backup").interval(Duration::from_secs(24 * 3600));
    match run_backup() {
        Ok(()) => check_in.await?,
        Err(e) => check_in.fail().message(e.to_string()).await?,
    };
    Ok(())
}
```

```sh
# crontab -e
HONK_URL=https://honk-honk.app
HONK_KEY=honk_…
15 3 * * * /usr/local/bin/backup
```

```ini
# /etc/systemd/system/backup.service (started daily by backup.timer, OnCalendar=03:15)
[Service]
Type=oneshot
EnvironmentFile=/etc/honk.env
ExecStart=/usr/local/bin/backup
```

A long-running service checks in from a loop; the grace period (5 minutes by default) absorbs
a slow tick:

```rust
let mut tick = tokio::time::interval(Duration::from_secs(300));
loop {
    tick.tick().await;
    if let Err(e) = honk.check_in("mailman-host").interval(Duration::from_secs(300)).await {
        tracing::warn!(%e, "heartbeat check-in failed"); // the next tick tries again
    }
}
```

## Sending a message

`honk(&message, idempotency_key)` returns `Accepted { id, duplicate, received_at, warnings }` once
Honk! Honk! has durably stored the event (`202`); that does not mean a push was delivered.

The essentials:

| Field | Notes |
|---|---|
| `message` | the text; required unless there is a `title` |
| `title` | one line; defaults to the first line of the message. `Message::default().title("…")` sends a title alone |
| `severity` | the level, which alone decides how loud the push is: `LIGHT`, `BEEP`, `LOUD`, `LONG`, `BLAST` (default light) |
| `group_key` | optional; same key, same group (`requests/4812`). Without one a message groups by its title |
| `event_type` | `Problem` / `Recovery` (or `uhoh` / `phew`); no group key needed |

Also `priority`, `category`, `source`, `environment`, `channel` (the last three fall back to the
client's defaults), `occurred_at`, `url`, `image_url`, `actions`, `metadata`, `ttl_seconds` and
`source_sequence`; see the [API reference](https://docs.rs/honk-honk). The server applies the
limits and says what it corrected (see Warnings).

`idempotency_key` is your stable key (`"order-42-failed"`, 1–128 printable ASCII characters)
or `None` for a new UUIDv7. A replay within 24 hours returns the original id with
`duplicate: true`; the same key with a different payload is `Error::Conflict`.

### Warnings

Honk! Honk! never loses an alert to an optional field. A title that is too long is shortened, an
`http://` link or a broken action is left out, a `ttl_seconds` out of range is clamped, `urgent`
from a key without `allow_urgent` is lowered to `high`; the message is stored and each correction
comes back in `accepted.warnings` (`field`, `code`: `truncated`, `normalized`, `ignored`,
`clamped`, `downgraded` or `hashed`, and an English `message`). They are not errors.

```rust
let accepted = honk.honk(&msg, None).await?;
for w in &accepted.warnings {
    tracing::warn!(id = %accepted.id, "honk corrected {w}"); // "title truncated: …"
}

// Or once for the whole client (async and blocking):
let honk = Honk::builder()
    .url(url)
    .key(key)
    .on_warnings(|a| for w in &a.warnings { tracing::warn!(id = %a.id, "honk: {w}") })
    .build()?;
```

The hook runs once per accepted send that has warnings, never for errors.

## Actions

Up to three buttons on a message, in display order (the first is the primary). Honk! Honk! never opens
them; the phone does when you tap one: Mail for `mailto:`, the Phone app for `tel:`, Messages for
`sms:`, Safari for `https://`.

```rust
honk.light("New quote request", "Maria Tudor asked for a quote: 3 rooms, 2 bathrooms")
    .group_key("requests/4812")
    .action("Reply", "mailto:maria@example.com?subject=Your%20quote")
    .action("Call", "tel:+15550134")
    .await?;
```

`Message::action` works the same way, and `message.actions` is a plain `Vec<Action>`. URLs are
`https://`, `mailto:`, `tel:` or `sms:`. An action the server can't use (`http:`, `javascript:`,
an app scheme) is left out with an `ignored` warning on `actions[i].url`; the others are kept.

## Configuration

```rust
use std::time::Duration;

let honk = Honk::builder()
    .url("https://honk-honk.app")
    .key(std::env::var("HONK_KEY").unwrap_or_default())
    .timeout(Duration::from_secs(5))   // per attempt
    .retries(4)                        // after the first attempt; 0 disables
    .deadline(Duration::from_secs(30)) // total, waits included
    .source("billing")                 // defaults for source, environment, channel
    .environment("production")
    .on_warnings(|a| eprintln!("honk {}: {:?}", a.id, a.warnings))
    .user_agent("my-app/1.2")
    .build()?;
```

Retries happen only on network errors, timeouts, `429` and `5xx`: exponential backoff with full
jitter (`random(0, min(8 s, 0.5 s·2ⁿ))`), never sooner than `Retry-After`, all under the
deadline. A wait that would cross the deadline (a daily quota) fails fast with `retry_after`.
Redirects are reported, never followed.

## Errors

The server refuses only what it cannot deliver; everything else is accepted with
[warnings](#warnings).

| Variant | When | What to do |
|---|---|---|
| `Error::Validation` | `400 invalid_json`, `413` (body over 16 KiB), `415`, `422` (no message and no title, an unknown field, an invalid `event_type`); also locally (`local == true`, nothing sent) for a message without text or title | fix the request |
| `Error::Auth` | `401 invalid_key`, `403 project_suspended`, `workspace_suspended`, `plan_limit` (a check-in creating one heartbeat too many) | fix the key or the plan |
| `Error::Quota` | `429 quota_exceeded` (daily, resets at UTC midnight), `rate_limited` or `overloaded`, after retries | retry after `retry_after` |
| `Error::Conflict` | `409 idempotency_conflict`: same key, different payload | use a new key, or send the original payload |
| `Error::Network` | unreachable on every attempt | retry later, same key |
| `Error::Timeout` | attempts timed out; the message may or may not be stored | retry later, same key |
| `Error::Server` | `5xx` on every attempt | retry later, same key |
| `Error::Http` | `404` (wrong URL, or a check-in without an interval for an unknown heartbeat), a redirect, an answer without an id | fix the URL, or give the check-in an interval |
| `Error::InvalidConfiguration` | a missing or malformed URL, or no key | fix the configuration |

Each variant except the last carries a `Failure` with `status`, `code`, `message`, `fields`,
`local`, `request_id`, `idempotency_key`, `attempts` and `retry_after` (also `e.status()`,
`e.code()`, `e.fields()`, `e.request_id()`). `Display` shows them all:

```text
honk: 422 validation_failed: Request validation failed (event_type invalid_enum: must be one of event, problem, recovery) [request_id req_01k6…]
```

```rust
use honk::Error;

match honk.honk(&msg, "order-1042-failed").await {
    Ok(_) => {}
    Err(Error::Validation(f)) => tracing::error!(?f.fields, "a bug: don't retry"),
    Err(e) if e.is_retryable() => queue.retry_later(e.idempotency_key(), e.retry_after()),
    Err(e) => return Err(e.into()),
}
```

## Blocking client

```rust
// Cargo.toml: honk-honk = { version = "0.4", features = ["blocking"] }
use std::time::Duration;
use honk::Message;
use honk::blocking::Honk;

let honk = Honk::from_env()?;
honk.beep("Backup finished", "nightly pg_dump took 42 s").send()?;
honk.uhoh("db/backup", "Backup failed", "pg_dump exited with 1").send()?;
honk.honk(&Message::new("Imported 1 204 rows"), None)?;
honk.check_in("nightly-backup").interval(Duration::from_secs(24 * 3600)).send()?;
```

It runs the async client on a private single-threaded Tokio runtime, with the same retries and
guarantees. Don't call it from inside an async runtime; use the async client there.

## Development

```sh
cargo test                                   # unit tests + mock server
cargo test --all-features                    # also the blocking client
cargo clippy --all-targets --all-features -- -D warnings
HONK_URL=… HONK_KEY=… cargo test --test integration   # against a real server (use a test project's key)
```

The version lives in `Cargo.toml` (`VERSION` and the User-Agent `honk-rust/<version>` come
from it). Releases: push a tag `vX.Y.Z` matching it and the release workflow publishes to
crates.io with trusted publishing (see `CHANGELOG.md`).

## Links

- [honk-honk.app](https://honk-honk.app): the Honk! Honk! inbox (web, iPhone).
- [docs.rs/honk-honk](https://docs.rs/honk-honk): the API reference.
- Other SDKs: [Node.js](https://github.com/honk-hq/node),
  [PHP / Laravel](https://github.com/honk-hq/php), [Go + CLI](https://github.com/honk-hq/go),
  [Swift](https://github.com/honk-hq/honk-swift), [Kotlin / Java](https://github.com/honk-hq/kotlin),
  [n8n](https://github.com/honk-hq/n8n).

MIT License.
