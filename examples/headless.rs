fn main() -> Result<(), Box<dyn std::error::Error>> {
    use chirrp::{ChirrpPlugin, Engine, SoundKind};
    let mut app = bevy_app::App::new();
    app.add_plugins(ChirrpPlugin);
    let mut engine = app.world_mut().resource_mut::<Engine>();
    engine.create_sound(SoundKind::Laser, 42, 6)?;
    let audio = engine.render_audio(0, 48_000)?;
    println!(
        "Rendered {} stereo frames without a window or device",
        audio.frames()
    );
    Ok(())
}
