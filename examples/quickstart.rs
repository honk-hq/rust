//! cargo run --example quickstart   (with HONK_URL and HONK_KEY set; sends one message)
use honk::{Honk, Message, Severity};

#[tokio::main]
async fn main() -> honk::Result<()> {
    let honk = Honk::from_env()?;
    let hello = Message::new("It compiled, it ran, it buzzed.")
        .title("Hello from Rust")
        .severity(Severity::BEEP);
    let accepted = honk.honk(&hello, None).await?;
    println!(
        "stored as {} (duplicate: {})",
        accepted.id, accepted.duplicate
    );
    Ok(())
}
