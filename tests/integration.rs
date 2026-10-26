//! Against a real Honk server: runs when HONK_URL and HONK_KEY are set (sdk/scripts/integration.sh
//! starts a throwaway one), skipped otherwise. The key must not allow urgent priority.
use honk::{Action, Category, Error, EventType, Honk, Message, Priority, Severity};

fn honk() -> Option<Honk> {
    match (std::env::var("HONK_URL"), std::env::var("HONK_KEY")) {
        (Ok(url), Ok(key)) if !url.is_empty() && !key.is_empty() => {
            Some(Honk::new(url, key).unwrap())
        }
        _ => {
            eprintln!("skipped: HONK_URL / HONK_KEY not set");
            None
        }
    }
}

fn unique(prefix: &str) -> String {
    format!("{prefix}-{}", honk::new_idempotency_key())
}

#[tokio::test]
async fn integration_minimal() {
    let Some(honk) = honk() else { return };
    let a = honk
        .send(&Message::new("rust: minimal"), None)
        .await
        .unwrap();
    assert!(a.id.starts_with("msg_"), "{a:?}");
    assert!(!a.duplicate);
    assert!(a.received_at_time().is_some(), "{a:?}");
}

#[tokio::test]
async fn integration_every_field_and_duplicate() {
    let Some(honk) = honk() else { return };
    let key = unique("rust-all");
    let msg = Message::new("rust: every field")
        .title("Rust SDK")
        .severity(Severity::LOUD)
        .priority(Priority::High)
        .category(Category::Automation)
        .source("rust-sdk")
        .environment("test")
        .channel("sdk")
        .group_key(unique("rust/group"))
        .occurred_at(std::time::SystemTime::now())
        .url("https://example.com/admin/requests/4812")
        .image_url("https://example.com/images/quote.png")
        .metadata("lang", "rust")
        .metadata("n", 1)
        .ttl_seconds(600);
    let first = honk.send(&msg, key.as_str()).await.unwrap();
    let again = honk.send(&msg, key.as_str()).await.unwrap();
    assert!(again.duplicate);
    assert_eq!(first.id, again.id);
}

#[tokio::test]
async fn integration_actions_and_duplicate() {
    let Some(honk) = honk() else { return };
    let key = unique("rust-actions");
    let mut msg = Message::new("rust: Emily Carter asked for a quote")
        .title("New quote request")
        .group_key(unique("rust/requests"))
        .action("Reply", "mailto:emily@example.com?subject=Your%20quote")
        .action("Call", "tel:+15550134");
    let first = honk.send(&msg, key.as_str()).await.unwrap();
    let again = honk.send(&msg, key.as_str()).await.unwrap();
    assert!(again.duplicate);
    assert_eq!(first.id, again.id);
    // The actions are part of the idempotency payload.
    msg.actions = vec![Action::new("Reply", "mailto:emily@example.com")];
    let err = honk.send(&msg, key.as_str()).await.unwrap_err();
    assert!(
        matches!(&err, Error::Conflict(f) if f.code == "idempotency_conflict"),
        "{err:?}"
    );
}

#[tokio::test]
async fn integration_conflict() {
    let Some(honk) = honk() else { return };
    let key = unique("rust-conflict");
    honk.send(&Message::new("rust: one"), key.as_str())
        .await
        .unwrap();
    let err = honk
        .send(&Message::new("rust: two"), key.as_str())
        .await
        .unwrap_err();
    assert!(
        matches!(&err, Error::Conflict(f) if f.code == "idempotency_conflict"),
        "{err:?}"
    );
}

#[tokio::test]
async fn integration_problem_and_recovery() {
    let Some(honk) = honk() else { return };
    let group = unique("rust/incident");
    honk.problem(&group, "Rust problem", "it broke")
        .await
        .unwrap();
    honk.recovery(&group, "Rust recovery", "it works")
        .await
        .unwrap();
}

#[tokio::test]
async fn integration_wrong_key() {
    let Some(_) = honk() else { return };
    let url = std::env::var("HONK_URL").unwrap();
    let bad = Honk::new(url, "honk_000000000000_00000000000000000000000000000000").unwrap();
    let err = bad
        .send(&Message::new("rust: wrong key"), None)
        .await
        .unwrap_err();
    assert!(
        matches!(&err, Error::Auth(f) if f.code == "invalid_key"),
        "{err:?}"
    );
}

#[tokio::test]
async fn integration_urgent_is_accepted() {
    let Some(honk) = honk() else { return };
    // Kept from a key with allow_urgent; lowered to high with a warning from any other key.
    let a = honk
        .send(
            &Message::new("rust: urgent").priority(Priority::Urgent),
            None,
        )
        .await
        .unwrap();
    assert!(
        a.warnings.is_empty()
            || a.warnings
                .iter()
                .any(|w| (w.field.as_str(), w.code.as_str()) == ("priority", "downgraded")),
        "{a:?}"
    );
}

#[tokio::test]
async fn integration_server_corrects_and_warns() {
    let Some(_) = honk() else { return };
    let seen = std::sync::Arc::new(std::sync::Mutex::new(0usize));
    let count = seen.clone();
    let honk = Honk::builder()
        .url(std::env::var("HONK_URL").unwrap())
        .key(std::env::var("HONK_KEY").unwrap())
        .on_warnings(move |a| *count.lock().unwrap() += a.warnings.len())
        .build()
        .unwrap();
    let msg = Message::new("rust: corrected")
        .title("t".repeat(200))
        .url("http://example.com/not-https")
        .ttl_seconds(5)
        .event_type(EventType::Recovery) // no group key needed any more
        .action("Call", "tel:+15550134")
        .action("Open", "javascript:alert(1)");
    let a = honk.send(&msg, None).await.unwrap();
    let mut got: Vec<(&str, &str)> = a
        .warnings
        .iter()
        .map(|w| (w.field.as_str(), w.code.as_str()))
        .collect();
    got.sort_unstable();
    assert_eq!(
        got,
        [
            ("actions[1].url", "ignored"),
            ("title", "truncated"),
            ("ttl_seconds", "clamped"),
            ("url", "ignored")
        ],
        "{a:?}"
    );
    assert_eq!(*seen.lock().unwrap(), 4);
}

#[tokio::test]
async fn integration_heartbeat_check_in() {
    let Some(honk) = honk() else { return };
    // A fixed slug: the first run creates the heartbeat, later runs check in with it.
    let c = honk
        .check_in("rust-sdk-integration")
        .name("Rust SDK integration")
        .interval(std::time::Duration::from_secs(604_800))
        .await
        .unwrap();
    assert!(c.heartbeat_id.starts_with("hbt_"), "{c:?}");
    assert_ne!(c.status, honk::HeartbeatStatus::Unknown, "{c:?}");

    let err = honk.check_in(unique("rust-hb")).await.unwrap_err();
    assert_eq!(
        (err.status(), err.code()),
        (Some(404), Some("not_found")),
        "{err}"
    );
}
