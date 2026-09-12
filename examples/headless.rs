//! Startup generates a bank; a playback observer only reads cached PCM.
//! No window/device is needed: a voice entity stands in for an audio backend.
use bevy_app::{App, Startup};
use bevy_ecs::prelude::*;
use chirrp::{AudioBuffer, ChirrpPlugin, Engine, SoundKind};
use std::sync::Arc;

#[derive(Resource, Default)]
struct SoundBank {
    laser: Option<Arc<AudioBuffer>>,
}

#[derive(Event)]
struct PlayLaser;

#[derive(Component)]
struct Voice(Arc<AudioBuffer>);

fn main() {
    let mut app = App::new();
    app.add_plugins(ChirrpPlugin)
        .init_resource::<SoundBank>()
        .add_systems(Startup, generate_bank)
        .add_observer(play_laser);
    app.update(); // Startup runs once, before gameplay requests playback.
    app.world_mut().trigger(PlayLaser); // Stand-in for a UI/gameplay event.
    app.world_mut().flush(); // Apply the observer's deferred spawn command.
    let mut voices = app.world_mut().query::<&Voice>();
    for voice in voices.iter(app.world()) {
        println!(
            "Queued {} cached stereo frames without a window or device",
            voice.0.frames()
        );
    }
}

fn generate_bank(
    mut engine: ResMut<Engine>,
    mut bank: ResMut<SoundBank>,
) -> bevy_ecs::error::Result {
    engine.create_sound(SoundKind::Laser, 42, 6)?;
    bank.laser = Some(Arc::new(engine.render_audio(0, 48_000)?));
    Ok(())
}

fn play_laser(_event: On<PlayLaser>, bank: Res<SoundBank>, mut commands: Commands) {
    if let Some(audio) = &bank.laser {
        // No synthesis or mutable bank access in the playback observer.
        commands.spawn(Voice(Arc::clone(audio)));
    }
}
