//! Heartbeat check-ins: `POST /v1/heartbeats/check-in` (contracts/API.md §11).

mod common;

use std::time::Duration;

use common::{KEY, Mock, Reply, client};
use honk::{Error, HeartbeatStatus};
use serde_json::json;

#[tokio::test]
async fn checks_in_with_every_field_the_key_and_no_idempotency_key() {
    let mock = Mock::start(vec![Reply::checked_in("hbt_1", "up")]).await;
    let honk = client(&mock).user_agent("cron/1").build().unwrap();
    let checked = honk
        .check_in("nightly-backup")
        .name("Nightly backup")
        .interval(Duration::from_secs(86_400))
        .grace(Duration::from_millis(3_600_700)) // whole seconds
        .message("dumped 42 tables")
        .await
        .unwrap();
    assert_eq!(checked.heartbeat_id, "hbt_1");
    assert_eq!(checked.status, HeartbeatStatus::Up);

    let reqs = mock.requests();
    assert_eq!(reqs.len(), 1);
    let r = &reqs[0];
    assert_eq!(
        (r.method.as_str(), r.path.as_str()),
        ("POST", "/v1/heartbeats/check-in")
    );
    assert_eq!(r.header("authorization"), format!("Bearer {KEY}"));
    assert_eq!(r.header("content-type"), "application/json");
    assert_eq!(r.header("accept"), "application/json");
    assert_eq!(
        r.header("user-agent"),
        format!("honk-rust/{} cron/1", honk::VERSION)
    );
    assert_eq!(r.header("idempotency-key"), "", "check-ins carry none");
    assert_eq!(
        r.json(),
        json!({
            "slug": "nightly-backup",
            "name": "Nightly backup",
            "interval_seconds": 86400,
            "grace_seconds": 3600,
            "status": "ok",
            "message": "dumped 42 tables"
        })
    );
}

#[tokio::test]
async fn minimal_and_failed_check_ins() {
    let mock = Mock::start(vec![
        Reply::checked_in("hbt_1", "up"),
        Reply::checked_in("hbt_1", "failing"),
        Reply::checked_in("hbt_1", "up"),
    ])
    .await;
    // A URL ending in /v1/messages is the base address too.
    let honk = honk::Honk::builder()
        .url(format!("{}/v1/messages/", mock.url))
        .key(KEY)
        .build()
        .unwrap();
    honk.check_in("mailman-host").send().await.unwrap();
    let c = honk
        .check_in("mailman-host")
        .fail()
        .message("queue runner exited with 1")
        .await
        .unwrap();
    assert_eq!(c.status, HeartbeatStatus::Failing);
    let pending = honk.check_in("mailman-host").fail().failed(false);
    assert_eq!(pending.slug(), "mailman-host");
    pending.await.unwrap();

    let bodies: Vec<_> = mock.requests().iter().map(|r| r.json()).collect();
    assert_eq!(
        bodies,
        [
            json!({ "slug": "mailman-host", "status": "ok" }),
            json!({ "slug": "mailman-host", "status": "fail", "message": "queue runner exited with 1" }),
            json!({ "slug": "mailman-host", "status": "ok" }),
        ]
    );
    assert!(
        mock.requests()
            .iter()
            .all(|r| r.path == "/v1/heartbeats/check-in")
    );
}

#[tokio::test]
async fn retries_like_messages() {
    let mock = Mock::start(vec![
        Reply::error(503, "unavailable", "busy").header("Retry-After", "0"),
        Reply::error(429, "rate_limited", "slow down").header("Retry-After", "0"),
        Reply::Drop,
        Reply::checked_in("hbt_1", "up"),
    ])
    .await;
    let honk = client(&mock).build().unwrap();
    let c = honk
        .check_in("backup")
        .interval(Duration::from_secs(300))
        .await
        .unwrap();
    assert_eq!(c.heartbeat_id, "hbt_1");
    let reqs = mock.requests();
    assert_eq!(reqs.len(), 4);
    assert!(reqs.iter().all(|r| r.body == reqs[0].body), "same body");
}

#[tokio::test]
async fn an_unknown_heartbeat_without_an_interval_is_a_404_with_a_hint() {
    let mock = Mock::start(vec![
        Reply::error(404, "not_found", "Not found"),
        Reply::error(404, "not_found", "Not found"),
    ])
    .await;
    let honk = client(&mock).build().unwrap();
    let err = honk.check_in("backup").await.unwrap_err();
    let Error::Http(f) = &err else {
        panic!("{err:?}")
    };
    assert_eq!(
        (f.status, f.code.as_str(), f.attempts),
        (Some(404), "not_found", 1)
    );
    assert!(err.to_string().contains("set an interval"), "{err}");
    assert_eq!(err.idempotency_key(), None);
    assert!(!err.is_retryable());

    // With an interval, a 404 means the server has no heartbeats (or the URL is wrong).
    let err = honk
        .check_in("backup")
        .interval(Duration::from_secs(60))
        .await
        .unwrap_err();
    assert!(err.to_string().contains("base address"), "{err}");
    assert_eq!(mock.count(), 2, "404 is not retried");
}

#[tokio::test]
async fn errors_that_are_never_retried() {
    for (status, code, want) in [
        (401, "invalid_key", "auth"),
        (403, "plan_limit", "auth"),
        (403, "project_suspended", "auth"),
        (422, "validation_failed", "validation"),
        (422, "unknown_field", "validation"),
    ] {
        let mock = Mock::start(vec![Reply::error(status, code, "nope")]).await;
        let err = client(&mock)
            .build()
            .unwrap()
            .check_in("backup")
            .interval(Duration::from_secs(300))
            .await
            .unwrap_err();
        let kind = match &err {
            Error::Validation(_) => "validation",
            Error::Auth(_) => "auth",
            other => panic!("{other:?}"),
        };
        assert_eq!(
            (kind, err.status(), err.code()),
            (want, Some(status), Some(code))
        );
        assert!(!err.is_retryable());
        assert_eq!(mock.count(), 1, "{status} {code} is not retried");
        assert_eq!(
            err.failure().unwrap().request_id.as_deref(),
            Some("req_test")
        );
    }
}

#[tokio::test]
async fn only_the_slug_is_checked_before_sending() {
    let mock = Mock::start(vec![Reply::checked_in("hbt_1", "new")]).await;
    let honk = client(&mock).build().unwrap();
    let err = honk.check_in("").await.unwrap_err();
    let Error::Validation(f) = &err else {
        panic!("{err:?}")
    };
    assert!(f.local);
    assert_eq!((f.attempts, err.fields()[0].field.as_str()), (0, "slug"));
    assert_eq!(mock.count(), 0);

    // Everything else is the server's to judge.
    honk.check_in("Nightly Backup")
        .interval(Duration::from_secs(30))
        .grace(Duration::from_secs(2 * 86_400))
        .await
        .unwrap();
    let sent = mock.requests()[0].json();
    assert_eq!(
        (
            sent["slug"].as_str(),
            sent["interval_seconds"].as_u64(),
            sent["grace_seconds"].as_u64()
        ),
        (Some("Nightly Backup"), Some(30), Some(172_800))
    );
}

#[tokio::test]
async fn an_answer_without_a_heartbeat_id_and_a_timeout() {
    let mock = Mock::start(vec![
        Reply::json(202, r#"{"status":"up"}"#),
        Reply::checked_in("late", "up").after(Duration::from_secs(5)),
    ])
    .await;
    let honk = client(&mock)
        .timeout(Duration::from_millis(100))
        .retries(0)
        .build()
        .unwrap();
    let err = honk.check_in("backup").await.unwrap_err();
    assert!(matches!(err, Error::Http(_)), "{err:?}");
    assert!(err.to_string().contains("heartbeat id"), "{err}");

    let err = honk.check_in("backup").await.unwrap_err();
    let Error::Timeout(f) = &err else {
        panic!("{err:?}")
    };
    assert_eq!((f.code.as_str(), f.attempts), ("timeout", 1));
    assert!(err.to_string().contains("check-in"), "{err}");
    assert_eq!(err.idempotency_key(), None);
    assert!(err.is_retryable());
}
