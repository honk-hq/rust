//! cargo run --example blocking --features blocking   (with HONK_URL and HONK_KEY set)
use honk::blocking::Honk;

fn main() -> honk::Result<()> {
    let honk = Honk::from_env()?;
    let accepted = honk
        .beep(
            "Hello from sync Rust",
            "Not a single .await was written for this honk.",
        )
        .send()?;
    println!("stored as {}", accepted.id);
    Ok(())
}
