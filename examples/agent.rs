//! Typed agent integration; --tools emits individual tool registration schemas.
use chirrp::{Engine, SoundEdits, SoundKind, tool_definitions};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    if std::env::args().any(|arg| arg == "--tools") {
        println!("{}", serde_json::to_string_pretty(&tool_definitions())?);
        return Ok(());
    }
    let mut engine = Engine::default();
    engine.create_sound(SoundKind::Laser, 42, 6)?;
    engine.edit_sound(SoundEdits {
        pitch_hz: Some(440.),
        stereo_width: Some(1.2),
        ..Default::default()
    })?;
    engine.randomize(0.45, 43)?;
    // A host can render and evaluate candidates with its own evaluator.
    engine.select_candidate(2)?;
    println!(
        "{}",
        serde_json::to_string_pretty(&engine.list_candidates()?)?
    );
    println!(
        "{}",
        serde_json::to_string_pretty(&engine.analyze(2, 48_000)?)?
    );
    Ok(())
}
