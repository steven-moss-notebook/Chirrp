use chirrp::{Recipe, SoundKind, mix, render, render_mix};

#[test]
fn mixing_preserves_stereo_and_the_longest_tail() {
    let short = render(&Recipe::new(SoundKind::UiHover, 1), 24_000).unwrap();
    let long = render(&Recipe::new(SoundKind::Laser, 2), 24_000).unwrap();
    assert!(short.frames() < long.frames());
    let mixed = mix(&[&short, &long]).unwrap();
    let expected: Vec<f32> = long
        .samples()
        .iter()
        .enumerate()
        .map(|(i, sample)| short.samples().get(i).copied().unwrap_or(0.) + sample)
        .collect();
    let peak = expected.iter().fold(0f32, |peak, x| peak.max(x.abs()));
    let gain = if peak > 0.89 { 0.89 / peak } else { 1. };
    assert_eq!(mixed.sample_rate(), 24_000);
    assert_eq!(mixed.channels(), 2);
    assert_eq!(mixed.frames(), long.frames());
    for (actual, expected) in mixed.samples().iter().zip(expected) {
        assert!((actual - expected * gain).abs() < 1e-6);
    }
    assert!(mixed.samples().chunks_exact(2).any(|p| p[0] != p[1]));
    assert_eq!(mixed.wav_bytes().len(), 44 + mixed.frames() * 4);
}

#[test]
fn a_single_sound_is_unchanged_and_quiet_mixes_are_not_amplified() {
    let sound = render(&Recipe::new(SoundKind::UiHover, 1), 22_050).unwrap();
    assert_eq!(mix(&[&sound]).unwrap().samples(), sound.samples());
    assert!(sound.metrics().peak * 2. < 0.89);
    let mixed = mix(&[&sound, &sound]).unwrap();
    for (actual, original) in mixed.samples().iter().zip(sound.samples()) {
        assert_eq!(*actual, original * 2.);
    }
}

#[test]
fn many_inputs_are_deterministic_and_share_peak_attenuation() {
    let sound = render(&Recipe::new(SoundKind::Explosion, 42), 24_000).unwrap();
    let sounds = vec![&sound; 33];
    let mixed = mix(&sounds).unwrap();
    assert_eq!(mixed.samples(), mix(&sounds).unwrap().samples());
    assert!(mixed.samples().iter().all(|x| x.is_finite()));
    assert!((mixed.metrics().peak - 0.89).abs() < 1e-6);
    // Repeated identical sounds retain their waveform, without hard clipping.
    let gain = 0.89 / sound.metrics().peak;
    for (actual, original) in mixed.samples().iter().zip(sound.samples()) {
        assert!((actual - original * gain).abs() < 2e-6);
    }

    // A distinct final layer must contribute even beyond the session's
    // 12-candidate limit or a fixed-size mixer's usual input count.
    let extra = render(&Recipe::new(SoundKind::Laser, 7), 24_000).unwrap();
    let mut extended = sounds;
    extended.push(&extra);
    let with_extra = mix(&extended).unwrap();
    assert_ne!(with_extra.samples(), mixed.samples());
    let expected: Vec<f32> = (0..with_extra.samples().len())
        .map(|i| {
            sound.samples().get(i).copied().unwrap_or(0.) * 33.
                + extra.samples().get(i).copied().unwrap_or(0.)
        })
        .collect();
    let peak = expected.iter().fold(0f32, |peak, x| peak.max(x.abs()));
    let gain = (0.89 / peak).min(1.);
    for (actual, expected) in with_extra.samples().iter().zip(expected) {
        assert!((actual - expected * gain).abs() < 2e-6);
    }
}

#[test]
fn recipe_mixing_matches_separate_rendering_including_legacy_designs() {
    let recipes = [
        Recipe::from_json(include_str!("fixtures/v1-laser.json")).unwrap(),
        Recipe::new(SoundKind::UiClick, 3),
        Recipe::new(SoundKind::Impact, 4),
    ];
    let sounds: Vec<_> = recipes.iter().map(|r| render(r, 24_000).unwrap()).collect();
    assert_eq!(
        render_mix(&recipes, 24_000).unwrap().samples(),
        mix(&sounds.iter().collect::<Vec<_>>()).unwrap().samples()
    );
    assert_eq!(
        render_mix(&recipes[..1], 24_000).unwrap().samples(),
        sounds[0].samples()
    );
}

#[test]
fn invalid_mix_requests_return_errors() {
    assert!(mix(&[]).is_err());
    assert!(render_mix(&[], 24_000).is_err());
    let recipe = Recipe::new(SoundKind::UiClick, 1);
    let a = render(&recipe, 22_050).unwrap();
    let b = render(&recipe, 48_000).unwrap();
    assert!(mix(&[&a, &b]).is_err());
    assert!(render_mix(std::slice::from_ref(&recipe), 0).is_err());
    assert!(render_mix(std::slice::from_ref(&recipe), 192_000).is_err());
    let mut invalid = recipe.clone();
    invalid.genome.room = f32::NAN;
    assert!(render_mix(&[recipe, invalid], 24_000).is_err());
}
