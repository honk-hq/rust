//! The synchronous client (feature `blocking`).
#![cfg(feature = "blocking")]

mod common;

use std::time::Duration;

use common::{KEY, Mock, Reply};
use honk::blocking::Honk;
use honk::{Error, Message, Severity};

fn mock(script: Vec<Reply>) -> (tokio::runtime::Runtime, Mock) {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let mock = rt.block_on(Mock::start(script));
    (rt, mock)
}

#[test]
fn sends_retries_and_maps_errors_without_a_runtime() {
    let (_rt, mock) = mock(vec![
        Reply::error(503, "unavailable", "busy"),
        Reply::accepted("msg_1"),
        Reply::error(401, "invalid_key", "bad key"),
    ]);
    let honk = Honk::builder()
        .url(&mock.url)
        .key(KEY)
        .backoff(Duration::from_millis(1), Duration::from_millis(5))
        .build_blocking()
        .unwrap();
    let a = honk
        .loud("Disk 91% full", "db-1 /var is at 91%")
        .group_key("disk/db-1")
        .action("Open", "https://grafana.example.com/d/disk")
        .idempotency_key("b-1")
        .send()
        .unwrap();
    assert_eq!(a.id, "msg_1");
    let err = honk
        .send(&Message::new("x").severity(Severity::BLAST), None)
        .unwrap_err();
    assert!(matches!(err, Error::Auth(_)), "{err:?}");

    let reqs = mock.requests();
    assert_eq!(reqs.len(), 3);
    assert_eq!(reqs[0].header("idempotency-key"), "b-1");
    assert_eq!(reqs[1].header("idempotency-key"), "b-1");
    assert_eq!(reqs[1].json()["severity"], "warning");
    assert_eq!(
        reqs[1].json()["actions"][0]["url"],
        "https://grafana.example.com/d/disk"
    );
    assert_eq!(reqs[2].json()["severity"], "critical");
}

#[test]
fn honk_uhoh_and_phew_are_send_problem_and_recovery() {
    let (_rt, mock) = mock(vec![]);
    let honk = Honk::new(&mock.url, KEY).unwrap();
    let order = Message::new("Someone bought 3 rubber chickens").severity(Severity::BEEP);
    honk.honk(&order, "order-1042").unwrap();
    honk.send(&order, "order-1042").unwrap();
    honk.uhoh("kitchen/oven", "The cake is burning", "Oven at 260 °C")
        .idempotency_key("oven-1")
        .send()
        .unwrap();
    honk.problem("kitchen/oven", "The cake is burning", "Oven at 260 °C")
        .idempotency_key("oven-1")
        .send()
        .unwrap();
    honk.phew("kitchen/oven", "Cake saved", "Nobody needs to know.")
        .idempotency_key("oven-2")
        .send()
        .unwrap();
    honk.recovery("kitchen/oven", "Cake saved", "Nobody needs to know.")
        .idempotency_key("oven-2")
        .send()
        .unwrap();

    let reqs = mock.requests();
    assert_eq!(reqs.len(), 6);
    for pair in reqs.chunks(2) {
        assert_eq!(pair[0].json(), pair[1].json());
        assert_eq!(
            pair[0].header("idempotency-key"),
            pair[1].header("idempotency-key")
        );
    }
    assert_eq!(reqs[0].json()["severity"], "success");
    assert_eq!(reqs[2].json()["event_type"], "problem");
    assert_eq!(reqs[2].json()["severity"], "error");
    assert_eq!(reqs[4].json()["event_type"], "recovery");
    assert_eq!(reqs[4].json()["group_key"], "kitchen/oven");
}

#[test]
fn is_shareable_between_threads() {
    let (_rt, mock) = mock(vec![]);
    let honk = Honk::new(&mock.url, KEY).unwrap();
    std::thread::scope(|s| {
        for i in 0..4 {
            let honk = honk.clone();
            s.spawn(move || honk.beep(format!("job {i}"), "done").send().unwrap());
        }
    });
    assert_eq!(mock.count(), 4);
    assert!(Honk::new("", KEY).is_err());
}

#[test]
fn checks_in() {
    let (_rt, mock) = mock(vec![
        Reply::error(503, "unavailable", "busy"),
        Reply::checked_in("hbt_1", "up"),
    ]);
    let honk = Honk::builder()
        .url(&mock.url)
        .key(KEY)
        .backoff(Duration::from_millis(1), Duration::from_millis(5))
        .build_blocking()
        .unwrap();
    let c = honk
        .check_in("nightly-backup")
        .interval(Duration::from_secs(86_400))
        .fail()
        .message("pg_dump exited with 1")
        .send()
        .unwrap();
    assert_eq!(
        (c.heartbeat_id.as_str(), c.status),
        ("hbt_1", honk::HeartbeatStatus::Up)
    );
    let reqs = mock.requests();
    assert_eq!(reqs.len(), 2);
    assert_eq!(reqs[1].path, "/v1/heartbeats/check-in");
    assert_eq!(
        reqs[1].json(),
        serde_json::json!({
            "slug": "nightly-backup",
            "interval_seconds": 86400,
            "status": "fail",
            "message": "pg_dump exited with 1"
        })
    );
}
