//! Generate once during Startup; button events only play cached asset handles.
use bevy::{audio::Volume, prelude::*};
use chirrp::{ChirrpPlugin, Engine, SoundKind};
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum Cue {
    Click,
    Laser,
    Impact,
}

const CUES: [(Cue, SoundKind, &str); 3] = [
    (Cue::Click, SoundKind::UiClick, "UI click"),
    (Cue::Laser, SoundKind::Laser, "Laser"),
    (Cue::Impact, SoundKind::Impact, "Impact"),
];

/// Strong handles keep the generated assets alive for the lifetime of the bank.
#[derive(Resource, Default)]
struct SoundBank(HashMap<Cue, Handle<AudioSource>>);

#[derive(Component)]
struct SoundButton(Cue);

/// Bevy 0.19 observers consume triggered Events. For buffered Messages instead,
/// use MessageWriter / MessageReader in explicitly ordered Update systems.
#[derive(Event)]
struct PlaySound(Cue);

fn main() {
    App::new()
        .add_plugins((
            DefaultPlugins.set(WindowPlugin {
                primary_window: Some(Window {
                    title: "Chirrp — cached game sounds".into(),
                    resolution: (640, 360).into(),
                    ..default()
                }),
                ..default()
            }),
            ChirrpPlugin,
        ))
        .init_resource::<SoundBank>()
        .add_systems(Startup, (generate_bank, build_ui).chain())
        .add_observer(play_sound)
        .run();
}

/// Synthesis and asset creation are explicit mutations at initialization time.
/// For a large bank, prepare audio on a worker/loading state and insert the
/// completed assets on the main world before making the buttons available.
fn generate_bank(
    mut engine: ResMut<Engine>,
    mut bank: ResMut<SoundBank>,
    mut assets: ResMut<Assets<AudioSource>>,
) -> bevy::ecs::error::Result {
    for (seed, (cue, kind, _)) in CUES.iter().enumerate() {
        engine.create_sound(*kind, 42 + seed as u32, 2)?;
        let audio = engine.render_audio(0, 48_000)?;
        // AudioSource expects encoded audio, not raw f32 bytes. The demo
        // enables Bevy's WAV decoder and encodes just once, here at startup.
        let handle = assets.add(AudioSource {
            bytes: audio.wav_bytes().into(),
        });
        bank.0.insert(*cue, handle);
    }
    Ok(())
}

fn build_ui(mut commands: Commands) {
    commands.spawn(Camera2d);
    commands
        .spawn((
            Node {
                width: percent(100),
                height: percent(100),
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                row_gap: px(20),
                ..default()
            },
            BackgroundColor(Color::srgb(0.035, 0.045, 0.075)),
        ))
        .with_children(|root| {
            root.spawn((
                Text::new("Chirrp sound bank"),
                TextFont {
                    font_size: FontSize::Px(28.),
                    ..default()
                },
            ));
            root.spawn(Text::new("Click a button to play a generated sound."));
            root.spawn(Node {
                column_gap: px(16),
                ..default()
            })
            .with_children(|row| {
                for (cue, _, label) in CUES {
                    row.spawn((
                        Button,
                        SoundButton(cue),
                        Node {
                            width: px(145),
                            height: px(64),
                            align_items: AlignItems::Center,
                            justify_content: JustifyContent::Center,
                            border_radius: BorderRadius::all(px(10)),
                            ..default()
                        },
                        BackgroundColor(Color::srgb(0.18, 0.3, 0.55)),
                    ))
                    .observe(button_clicked)
                    .with_children(|button| {
                        button.spawn(Text::new(label));
                    });
                }
            });
        });
}

fn button_clicked(click: On<Pointer<Click>>, buttons: Query<&SoundButton>, mut commands: Commands) {
    if click.button != PointerButton::Primary {
        return;
    }
    if let Ok(button) = buttons.get(click.entity) {
        commands.trigger(PlaySound(button.0));
    }
}

/// Playback only reads the bank; each request clones a cheap asset handle and
/// spawns a separate voice. Bevy owns decoding, device output, and voice cleanup.
fn play_sound(event: On<PlaySound>, bank: Res<SoundBank>, mut commands: Commands) {
    let Some(handle) = bank.0.get(&event.0) else {
        warn!("Sound bank has no {:?} cue yet", event.0);
        return;
    };
    commands.spawn((
        AudioPlayer::new(handle.clone()),
        PlaybackSettings::DESPAWN.with_volume(Volume::Linear(0.25)),
    ));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn playback_reuses_startup_assets_without_mutating_the_bank_or_engine() {
        let mut app = App::new();
        app.add_plugins(ChirrpPlugin)
            .init_resource::<SoundBank>()
            .init_resource::<Assets<AudioSource>>()
            .add_systems(Startup, (generate_bank, build_ui).chain())
            .add_observer(play_sound);
        app.update();
        // PointerTraversal queries Window as well as ChildOf. Register the
        // window component without installing Winit or opening a native window.
        app.world_mut().spawn(Window::default());
        let bank = app.world().resource::<SoundBank>().0.clone();
        let session = app.world().resource::<Engine>().snapshot().unwrap();
        let initial_assets = app.world().resource::<Assets<AudioSource>>().len();
        assert_eq!(initial_assets, 3);

        // The observer runs in a bare ECS app: no window, GPU, or device needed.
        app.world_mut().trigger(PlaySound(Cue::Laser));
        app.world_mut().trigger(PlaySound(Cue::Laser));
        app.world_mut().flush();
        let mut voices = app.world_mut().query::<(&AudioPlayer, &PlaybackSettings)>();
        let voices: Vec<_> = voices.iter(app.world()).collect();
        assert_eq!(voices.len(), 2);
        for (player, settings) in voices {
            assert_eq!(player.0, bank[&Cue::Laser]);
            assert!(matches!(settings.mode, bevy::audio::PlaybackMode::Despawn));
        }
        // A click on a button's text must bubble to the button's observer and
        // trigger the same playback path, not synthesize another sound.
        let mut buttons = app.world_mut().query::<(Entity, &SoundButton, &Children)>();
        let text = buttons
            .iter(app.world())
            .find(|(_, button, _)| button.0 == Cue::Laser)
            .unwrap()
            .2[0];
        let click = Pointer::new(
            bevy::picking::pointer::PointerId::Mouse,
            bevy::picking::pointer::Location {
                target: bevy::camera::NormalizedRenderTarget::None {
                    width: 640,
                    height: 360,
                },
                position: Vec2::ZERO,
            },
            Click {
                button: PointerButton::Primary,
                hit: bevy::picking::backend::HitData::new(Entity::PLACEHOLDER, 0., None, None),
                duration: std::time::Duration::from_millis(50),
                count: 1,
            },
            text,
        );
        app.world_mut().trigger(click);
        app.world_mut().flush();
        app.update();
        assert_eq!(
            app.world_mut()
                .query::<&AudioPlayer>()
                .iter(app.world())
                .count(),
            3
        );
        assert_eq!(app.world().resource::<SoundBank>().0, bank);
        assert_eq!(
            app.world().resource::<Assets<AudioSource>>().len(),
            initial_assets
        );
        assert_eq!(
            app.world().resource::<Engine>().snapshot().unwrap(),
            session
        );
    }
}
