//! Render the current preset bank for listening and signal inspection.
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let directory = std::env::args().nth(1).unwrap_or_else(|| "audition".into());
    std::fs::create_dir_all(&directory)?;
    for entry in chirrp::catalog() {
        let name = serde_json::to_value(entry.kind)?
            .as_str()
            .unwrap()
            .to_owned();
        let audio = chirrp::render(&chirrp::Recipe::new(entry.kind, 42), 48_000)?;
        std::fs::write(
            std::path::Path::new(&directory).join(format!("{name}.wav")),
            audio.wav_bytes(),
        )?;
        println!("{name}: {}", serde_json::to_string(&audio.metrics())?);
    }
    Ok(())
}
