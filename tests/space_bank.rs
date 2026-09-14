use chirrp::{
    MixLayer, Recipe, RenderOptions, SoundKind, catalog, execute_asset_tool, render, render_loop,
    render_loop_with_options, render_mix, render_mix_layers, render_with_options,
};
use serde_json::json;

#[test]
fn additive_defaults_preserve_existing_render_and_mix_contracts() {
    for entry in catalog()
        .into_iter()
        .filter(|e| Recipe::new(e.kind, 42).version < 6)
    {
        let recipe = Recipe::new(entry.kind, 42);
        assert_eq!(
            render(&recipe, 22_050).unwrap().wav_bytes(),
            render_with_options(&recipe, 22_050, RenderOptions::default())
                .unwrap()
                .wav_bytes()
        );
    }
    let recipes = [
        Recipe::new(SoundKind::UiClick, 1),
        Recipe::new(SoundKind::UiHover, 2),
    ];
    let layers: Vec<_> = recipes
        .iter()
        .cloned()
        .map(|r| MixLayer::new(r, 1., 0.))
        .collect();
    assert_eq!(
        render_mix(&recipes, 22_050).unwrap().samples(),
        render_mix_layers(&layers, 22_050).unwrap().samples()
    );
}

#[test]
fn dry_mid_bypasses_room_width_and_direct_detail_and_exports_true_mono() {
    let mut recipe = Recipe::new(SoundKind::Thruster, 42);
    let options = RenderOptions { dry_mid: true };
    let dry = render_with_options(&recipe, 24_000, options).unwrap();
    recipe.genome.room = 1.;
    recipe.genome.width = 1.5;
    assert_eq!(
        dry.samples(),
        render_with_options(&recipe, 24_000, options)
            .unwrap()
            .samples()
    );
    assert!(dry.samples().chunks_exact(2).all(|p| p[0] == p[1]));
    let wet = render(&recipe, 24_000).unwrap();
    assert!(wet.frames() > dry.frames());
    let wav = dry.wav_bytes_mono();
    assert_eq!(u16::from_le_bytes(wav[22..24].try_into().unwrap()), 1);
    assert_eq!(u32::from_le_bytes(wav[28..32].try_into().unwrap()), 48_000);
    assert_eq!(wav.len(), 44 + dry.frames() * 2);
    for (frame, pcm) in dry.samples().chunks_exact(2).zip(wav[44..].chunks_exact(2)) {
        assert_eq!(
            i16::from_le_bytes(pcm.try_into().unwrap()),
            (frame[0] * 32767.).round() as i16
        );
    }
}

#[test]
fn loops_have_exact_duration_sustained_energy_and_no_abnormal_seam() {
    for entry in catalog().into_iter().filter(|e| e.kind.is_bed()) {
        for sr in [22_050, 48_000, 96_000] {
            let recipe = Recipe::new(entry.kind, 42);
            let audio = render_loop(&recipe, sr, 4.).unwrap();
            assert_eq!(audio.frames(), sr as usize * 4);
            assert!(audio.is_loop());
            assert!(audio.metrics().peak <= 0.890_001);
            assert!(audio.samples().iter().all(|s| s.is_finite()));
            for second in audio.samples().chunks_exact(sr as usize * 2) {
                let rms = (second.iter().map(|s| s * s).sum::<f32>() / second.len() as f32).sqrt();
                assert!(rms > 0.002, "{:?} {sr}: {rms}", entry.kind);
            }
            for ch in 0..2 {
                let signal: Vec<_> = audio.samples().chunks_exact(2).map(|f| f[ch]).collect();
                let max_step = signal
                    .windows(2)
                    .map(|p| (p[1] - p[0]).abs())
                    .fold(0f32, f32::max);
                let seam = (signal[0] - signal[signal.len() - 1]).abs();
                assert!(
                    seam < max_step * 1.1 + 1e-5,
                    "{:?}: {seam} vs {max_step}",
                    entry.kind
                );
                let n = (sr as usize / 100).max(1);
                let edge_energy: f32 = signal[..n]
                    .iter()
                    .chain(&signal[signal.len() - n..])
                    .map(|s| s * s)
                    .sum();
                assert!(
                    edge_energy / (2 * n) as f32 > 1e-6,
                    "loop must not fade to silence"
                );
            }
        }
    }
}

#[test]
fn loop_metadata_is_inclusive_and_survives_mono_export() {
    let recipe = Recipe::new(SoundKind::HullRumble, 7);
    let audio = render_loop(&recipe, 22_050, 0.10003).unwrap();
    assert_eq!(audio.frames(), (0.10003f32 * 22_050.).round() as usize);
    for (wav, channels) in [(audio.wav_bytes(), 2), (audio.wav_bytes_mono(), 1)] {
        assert_eq!(
            u32::from_le_bytes(wav[4..8].try_into().unwrap()) as usize,
            wav.len() - 8
        );
        let smpl = 44 + audio.frames() * channels * 2;
        assert_eq!(&wav[smpl..smpl + 4], b"smpl");
        let field = |i: usize| {
            u32::from_le_bytes(wav[smpl + 8 + i * 4..smpl + 12 + i * 4].try_into().unwrap())
        };
        assert_eq!(field(7), 1); // loop count
        assert_eq!(field(10), 0); // forward
        assert_eq!(field(11), 0); // start
        assert_eq!(field(12), audio.frames() as u32 - 1); // inclusive end
        assert_eq!(field(14), 0); // infinite playback
    }
    assert_eq!(
        audio.wav_bytes(),
        render_loop(&recipe, 22_050, 0.10003).unwrap().wav_bytes()
    );
    let alarm = render_loop(&Recipe::new(SoundKind::UiError, 42), 24_000, 4.).unwrap();
    for second in alarm.samples().chunks_exact(48_000) {
        assert!(second.iter().any(|s| s.abs() > 0.02));
    }
}

#[test]
fn long_beds_preserve_sustain_through_evolution_and_respect_validation() {
    let mut recipe = Recipe::new(SoundKind::ChoirInterval, 42);
    recipe.genome.envelope.decay_s = 16.;
    recipe.genome.envelope.sustain_level = 0.85;
    let long = render(&recipe, 22_050).unwrap();
    assert!(long.frames() > 16 * 22_050);
    assert!(
        long.samples()[15 * 22_050 * 2..16 * 22_050 * 2]
            .iter()
            .any(|s| s.abs() > 0.05)
    );
    assert_eq!(
        render_loop(&recipe, 22_050, 16.).unwrap().frames(),
        16 * 22_050
    );
    let mut engine = chirrp::Engine::default();
    engine
        .create_sound(SoundKind::ChoirInterval, 42, 4)
        .unwrap();
    engine.replace_recipe(recipe.clone()).unwrap();
    engine.randomize(0.7, 17).unwrap();
    for candidate in engine.snapshot().unwrap().candidates() {
        candidate.validate().unwrap();
        assert!(candidate.genome.envelope.decay_s > 2.);
        assert!(candidate.genome.envelope.sustain_level > 0.);
    }
    recipe.version = 5;
    assert!(recipe.validate().is_err());
    let mut old = Recipe::new(SoundKind::Laser, 42);
    old.genome.envelope.sustain_level = 0.1;
    assert!(old.validate().is_err());
    old.genome.envelope.sustain_level = 0.;
    old.genome.envelope.decay_s = 16.;
    assert!(old.validate().is_err());
    for invalid in [f32::NAN, f32::INFINITY, -1., 0., 0.09, 16.01] {
        assert!(render_loop(&Recipe::new(SoundKind::BeamLoop, 1), 24_000, invalid).is_err());
    }
}

#[test]
fn timed_layers_preserve_delay_gain_tail_and_linked_peak_protection() {
    let recipe = Recipe::new(SoundKind::UiClick, 3);
    let original = render(&recipe, 24_000).unwrap();
    let delayed = render_mix_layers(&[MixLayer::new(recipe.clone(), 0.25, 0.125)], 24_000).unwrap();
    assert_eq!(delayed.frames(), original.frames() + 3000);
    assert!(delayed.samples()[..6000].iter().all(|s| *s == 0.));
    for (actual, expected) in delayed.samples()[6000..].iter().zip(original.samples()) {
        assert_eq!(*actual, expected * 0.25);
    }
    let hot = render_mix_layers(&vec![MixLayer::new(recipe.clone(), 8., 0.); 32], 24_000).unwrap();
    assert!((hot.metrics().peak - 0.89).abs() < 1e-6);
    for (actual, expected) in hot.samples().iter().zip(original.samples()) {
        assert!((actual - expected * (0.89 / original.metrics().peak)).abs() < 2e-6);
    }
    for (gain, delay) in [
        (f32::NAN, 0.),
        (1., f32::INFINITY),
        (-1., 0.),
        (1., -1.),
        (8.01, 0.),
        (1., 16.01),
    ] {
        assert!(render_mix_layers(&[MixLayer::new(recipe.clone(), gain, delay)], 24_000).is_err());
    }
    assert!(render_mix_layers(&[], 24_000).is_err());
    assert!(render_mix_layers(&vec![MixLayer::new(recipe, 1., 0.); 33], 24_000).is_err());
}

#[test]
fn shared_asset_tools_round_trip_all_export_settings() {
    let loop_asset = execute_asset_tool("generate_loop", json!({
        "kind":"magnet_pulse","seed":7,"sample_rate":24000,"loop_s":4.,"dry_mid":true,"mono":true,"sustain_level":0.8
    })).unwrap();
    assert_eq!(loop_asset.channels(), 1);
    assert_eq!(
        loop_asset.wav_bytes(),
        execute_asset_tool("render_sound", loop_asset.sidecar["render_sound"].clone())
            .unwrap()
            .wav_bytes()
    );
    let recipe = Recipe::new(SoundKind::HeavySlug, 7);
    let mixed = execute_asset_tool(
        "mix_layers",
        json!({"layers":[
        {"recipe":recipe,"gain":0.5,"delay_s":0.125},
        {"recipe":recipe,"gain":0.25,"delay_s":0.15}
    ],"dry_mid":true,"mono":true,"sample_rate":24000}),
    )
    .unwrap();
    assert_eq!(
        mixed.wav_bytes(),
        execute_asset_tool("mix_layers", mixed.sidecar["mix_layers"].clone())
            .unwrap()
            .wav_bytes()
    );
    assert!(
        execute_asset_tool(
            "generate_loop",
            json!({"kind":"laser","loop_s":4.,"sustain_level":1.})
        )
        .is_err()
    );
    let dry_loop = render_loop_with_options(
        &Recipe::new(SoundKind::VacuumLoop, 4),
        24_000,
        1.,
        RenderOptions { dry_mid: true },
    )
    .unwrap();
    assert!(dry_loop.samples().chunks_exact(2).all(|p| p[0] == p[1]));
}
