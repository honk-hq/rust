# Changelog

All notable changes to the `honk-honk` crate are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and the project uses
[Semantic Versioning](https://semver.org/).

## [Unreleased]

## [0.5.0] - 2026-10-26

Honk! Honk! now delivers first (contracts/API.md §5.1): the server refuses only what it cannot
deliver and corrects everything else, listing each correction in the `202` answer. The crate
becomes a thin wrapper around that.

### Added
- `Accepted::warnings: Vec<Warning>` and `Accepted::has_warnings()`: what the server corrected
  to accept the message. `Warning { field, code, message }` (codes `truncated`, `normalized`,
  `ignored`, `clamped`, `downgraded`, `hashed`; unknown codes are kept as sent) displays as
  `title truncated: …`. A missing, `null` or unreadable `warnings` is an empty list, never an
  error.
- `HonkBuilder::on_warnings(|accepted| …)`: called once per accepted send that has warnings
  (never for errors, not per retry), for the async and the blocking client.
- `Error::request_id()`. `Display` of an error now shows each field's code as well as its
  message (`event_type invalid_enum: must be one of …`) and ends with `[request_id req_…]`;
  `FieldError` implements `Display`.
- A title-only message: `Message::default().title("…")` (or a helper with an empty message)
  is sent without `message`.
- Heartbeat check-ins (`POST /v1/heartbeats/check-in`): `honk.check_in(slug)` returns a
  `PendingCheckIn` with `.name(…)`, `.interval(Duration)`, `.grace(Duration)`, `.fail()`,
  `.failed(bool)` and `.message(…)`; `.await` it for a `CheckedIn { heartbeat_id, status }`.
  With an interval, the first check-in creates the heartbeat. Retried like messages (no
  idempotency key needed). Only the slug is required locally; the server checks the values.
  Also on `blocking::Honk`, finished with `.send()`. `HeartbeatStatus`.

### Changed
- No more local validation of field values (lengths, URLs, actions, metadata, TTL, the 16 KiB
  body, group-key rules): the values are sent as given and the server corrects them with a
  warning or refuses the request. The only local checks left: a message or a title (non-blank;
  `Error::Validation`, field `message`, code `required`), a check-in's slug, a URL and a key on
  the client, and an `Idempotency-Key` without control characters (which could not be sent as a
  header). The server's other rules for the key (1–128 printable ASCII) come back as its `422`.
- A recovery and a `source_sequence` no longer need a group key, and `uhoh`/`phew`/`problem`/
  `recovery` accept an empty group key (it is left out; the message groups by its title).
- The key is no longer required to start with `honk_` locally; a wrong key is the server's
  `401 invalid_key`.
- `encode_message(message, defaults)` lost its `validate` argument.
- `Priority::Urgent` from a key without `allow_urgent` is accepted as `high` with a
  `downgraded` warning (the server no longer answers `403 priority_not_allowed`).

### Deprecated
- `HonkBuilder::validate(bool)`: does nothing now; remove the call.

### Removed
- The limit constants `MAX_BODY_BYTES`, `MAX_MESSAGE_BYTES`, `MAX_TITLE`, `MAX_SOURCE`,
  `MAX_ENVIRONMENT`, `MAX_CHANNEL`, `MAX_GROUP_KEY`, `MAX_URL_BYTES`, `MAX_ACTIONS`,
  `MAX_ACTION_TITLE`, `MAX_METADATA_KEYS`, `MAX_METADATA_STRING`, `MIN_TTL_SECONDS` and
  `MAX_TTL_SECONDS`: the server enforces (and corrects to) them.
- The retired error codes `priority_not_allowed` and `requires_group_key` from the docs.

## [0.4.1] - 2026-10-18

### Changed
- The README and the crate description call the product Honk! Honk!; the crate, the library
  and the API are unchanged.

## [0.4.0] - 2026-10-15

### Changed
- The package is now `honk-honk` on crates.io (the library is still `honk`).

## [0.3.0] - 2026-10-14

### Changed
- **Breaking:** the library is now `honk` (it was `honk_me`): `use honk::Honk;`. The package on
  crates.io is still `honk-me`, so `Cargo.toml` stays `honk-me = "0.3"`.

  Upgrading from 0.2: bump the version and replace `honk_me::` with `honk::` in your code.
  Nothing else changes.

### Added
- `honk(&message, idempotency_key)`, `uhoh(group_key, title, message)` and
  `phew(group_key, title, message)` on `Honk` and on `blocking::Honk`.
- `send`, `problem` and `recovery` stay, without deprecation warnings, as plain aliases of
  `honk`, `uhoh` and `phew`: same arguments, same results.

## [0.2.0] - 2026-10-07

### Added
- `Message::actions` and `Action`: up to 3 buttons (`https://`, `mailto:`, `tel:` or `sms:`),
  added with `.action(title, url)` on `Message` and on both `PendingSend`s, sent as `actions` and
  omitted when empty. Validated locally like the server, with errors on `actions`,
  `actions[i].title` and `actions[i].url`; `MAX_ACTIONS` and `MAX_ACTION_TITLE`.

## [0.1.0] - 2026-10-04

### Added
- Async `Honk` client for `POST /v1/messages` on Tokio and `reqwest` with rustls (no
  OpenSSL), with every field of the v1 ingestion API (including `image_url`):
  `Honk::new(url, key)`, `Honk::builder()`, `Honk::from_env()`.
- `send(&message, idempotency_key)` returning `Accepted { id, duplicate, received_at }`.
- Automatic UUIDv7 `Idempotency-Key` (or your own), reused on every retry.
- Retries for network errors, timeouts, 429 and 5xx with exponential backoff, full jitter,
  `Retry-After` and a total deadline; redirects are reported, never followed.
- The `Error` enum: `Validation`, `Auth`, `Quota`, `Conflict`, `Network`, `Timeout`, `Server`,
  `Http` (each with a `Failure`), `InvalidConfiguration`; `is_retryable()`.
- Local validation of limits, https-only URLs, metadata and the 16 KiB body, with every invalid
  field reported at once; `validate(false)` leaves it to the server.
- The Honk scale: `Severity::LIGHT`, `BEEP`, `LOUD`, `LONG`, `BLAST` (aliases of the canonical
  severities), case-insensitive parsing, always sent canonical; the `light`, `beep`, `loud`,
  `long`, `blast` helpers and the `info`, `success`, `warning`, `error`, `critical` synonyms;
  `problem` and `recovery`. Helpers return a `PendingSend` to chain fields and `.await`.
- `blocking` feature: `honk_me::blocking::Honk`, a synchronous client.
- `encode_message` (the exact JSON body, for dry runs) and `new_idempotency_key`.
