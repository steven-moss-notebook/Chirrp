//! cargo run --release --example render -- explosion /tmp/explosion.wav 42
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();
    let kind: chirrp::SoundKind = serde_json::from_value(serde_json::Value::String(
        args.get(1).cloned().unwrap_or("explosion".into()),
    ))?;
    let path = args.get(2).map(String::as_str).unwrap_or("chirrp.wav");
    let seed = args.get(3).map(|s| s.parse()).transpose()?.unwrap_or(42);
    let audio = chirrp::render(&chirrp::Recipe::new(kind, seed), 48_000)?;
    std::fs::write(path, audio.wav_bytes())?;
    println!("{}", serde_json::to_string_pretty(&audio.metrics())?);
    Ok(())
}
