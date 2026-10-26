//! Encoding: the client checks only that a message has a text or a title, and sends every
//! other value as given (the server corrects it and answers with warnings).
use honk::{
    Category, Defaults, Error, EventType, Message, MetadataValue, Priority, Severity,
    encode_message,
};

#[test]
fn a_message_or_a_title_is_required() {
    for m in [
        Message::new(""),
        Message::new("  \n"),
        Message::new("").title("   "),
        Message::default(),
    ] {
        let err = encode_message(&m, &Defaults::new()).unwrap_err();
        let Error::Validation(f) = &err else {
            panic!("{err:?}")
        };
        assert!(f.local);
        assert_eq!(f.attempts, 0);
        assert_eq!(
            (f.fields[0].field.as_str(), f.fields[0].code.as_str()),
            ("message", "required")
        );
        assert_eq!(
            err.to_string(),
            "honk: invalid request (message required: a message or a title is required)"
        );
    }
    // A title alone is enough: "message" is left out and the server stores "".
    assert_eq!(
        encode_message(
            &Message::default().title("Deploy finished"),
            &Defaults::new()
        )
        .unwrap(),
        r#"{"title":"Deploy finished"}"#
    );
}

#[test]
fn every_other_value_is_sent_as_given() {
    let m = Message::new("x".repeat(9000))
        .title("t".repeat(300))
        .group_key("g".repeat(200))
        .url("ftp://example.com")
        .image_url("https://user:pw@example.com/a.png#x")
        .action("Run", "javascript:alert(1)")
        .action("a", "tel:1")
        .action("b", "tel:2")
        .action("c", "tel:3")
        .metadata("bad key", "v".repeat(600))
        .ttl_seconds(5)
        .source_sequence(1 << 60)
        .event_type(EventType::Recovery);
    let body = encode_message(&m, &Defaults::new()).unwrap();
    let json: serde_json::Value = serde_json::from_str(&body).unwrap();
    assert_eq!(json["title"].as_str().unwrap().len(), 300);
    assert_eq!(json["message"].as_str().unwrap().len(), 9000);
    assert_eq!(json["url"], "ftp://example.com");
    assert_eq!(json["actions"].as_array().unwrap().len(), 4);
    assert_eq!(json["metadata"]["bad key"].as_str().unwrap().len(), 600);
    assert_eq!(json["ttl_seconds"], 5);
    assert_eq!(json["source_sequence"], 1u64 << 60);
    // A recovery or a source sequence no longer needs a group key.
    let body = encode_message(
        &Message::new("ok")
            .event_type(EventType::Recovery)
            .source_sequence(7),
        &Defaults::new(),
    )
    .unwrap();
    assert_eq!(
        body,
        r#"{"message":"ok","event_type":"recovery","source_sequence":7}"#
    );
}

#[test]
fn encode_applies_defaults_and_is_canonical() {
    let d = Defaults::new()
        .source("nightly")
        .environment("prod")
        .channel("ops");
    let body = encode_message(
        &Message::new("Backup finished")
            .severity(Severity::BEEP)
            .channel("db"),
        &d,
    )
    .unwrap();
    assert_eq!(
        body,
        r#"{"message":"Backup finished","severity":"success","source":"nightly","environment":"prod","channel":"db"}"#
    );
    // The same event with a horn alias or the canonical value is the same payload (and so the
    // same idempotency payload).
    let a = encode_message(&Message::new("x").severity(Severity::LOUD), &d).unwrap();
    let b = encode_message(&Message::new("x").severity(Severity::Warning), &d).unwrap();
    assert_eq!(a, b);
    // Metadata keys are sorted, so the body is deterministic.
    let m = encode_message(
        &Message::new("x").metadata("b", 1).metadata("a", 2),
        &Defaults::new(),
    )
    .unwrap();
    assert_eq!(m, r#"{"message":"x","metadata":{"a":2,"b":1}}"#);
}

#[test]
fn the_honk_scale() {
    assert_eq!(
        [
            Severity::LIGHT,
            Severity::BEEP,
            Severity::LOUD,
            Severity::LONG,
            Severity::BLAST
        ],
        [
            Severity::Info,
            Severity::Success,
            Severity::Warning,
            Severity::Error,
            Severity::Critical
        ]
    );
    for (input, want) in [
        ("light", Severity::Info),
        ("BEEP", Severity::Success),
        (" Loud ", Severity::Warning),
        ("long", Severity::Error),
        ("blast", Severity::Critical),
        ("warning", Severity::Warning),
        ("CRITICAL", Severity::Critical),
    ] {
        assert_eq!(input.parse::<Severity>().unwrap(), want, "{input}");
    }
    let err = "honk".parse::<Severity>().unwrap_err().to_string();
    assert!(
        err.contains("light (info)") && err.contains("blast (critical)"),
        "{err}"
    );
    assert_eq!(Severity::Critical.to_string(), "critical");
    assert_eq!(Severity::Critical.horn(), "blast");
}

#[test]
fn other_enums_parse_and_print_their_wire_values() {
    assert_eq!("HIGH".parse::<Priority>().unwrap(), Priority::High);
    assert_eq!(
        "recovery".parse::<EventType>().unwrap(),
        EventType::Recovery
    );
    assert_eq!("payments".parse::<Category>().unwrap(), Category::Payments);
    assert!(
        "soon"
            .parse::<Priority>()
            .unwrap_err()
            .to_string()
            .contains("low, normal, high, urgent")
    );
    assert_eq!(Category::ALL.len(), 10);
    assert_eq!(Priority::Urgent.to_string(), "urgent");
    assert_eq!(MetadataValue::from(7u8), MetadataValue::Integer(7));
    assert_eq!(MetadataValue::from("x"), MetadataValue::String("x".into()));
}
