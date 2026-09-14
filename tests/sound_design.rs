use chirrp::{Recipe, Session, SoundKind, render};

#[test]
fn hover_stays_short_and_quiet_even_in_a_dense_series() {
    let hover = render(&Recipe::new(SoundKind::UiHover, 42), 24_000).unwrap();
    let confirm = render(&Recipe::new(SoundKind::UiConfirm, 42), 24_000).unwrap();
    assert!(hover.metrics().peak < 0.0451);
    assert!(hover.metrics().duration_seconds < 0.12);
    let energy = |samples: &[f32]| samples.iter().map(|v| (*v as f64).powi(2)).sum::<f64>();
    assert!(energy(hover.samples()) < energy(confirm.samples()) * 0.02);
    // 100 hover transitions, twenty per second. No normalization is allowed
    // to raise each tiny cue to the loudness of a notification or game impact.
    let hop = 1200 * 2;
    let mut series = vec![0f32; hop * 100 + hover.samples().len()];
    for start in (0..100).map(|i| i * hop) {
        for (out, x) in series[start..].iter_mut().zip(hover.samples()) {
            *out += x;
        }
    }
    assert!(series.iter().all(|v| v.abs() < 0.065));
}

#[test]
fn hover_evolution_preserves_its_frequent_use_role() {
    let mut session = Session::new(SoundKind::UiHover, 42, 4).unwrap();
    for seed in 0..25 {
        session.select(1).unwrap();
        session.randomize(1., seed).unwrap();
        let audio = session.render(1, 24_000).unwrap();
        assert!(audio.metrics().peak <= 0.0451);
        assert!(audio.metrics().duration_seconds < 0.18);
    }
}

fn spectral_centroid(samples: &[f32], start: f32, sr: f32) -> f32 {
    let offset = (start * sr) as usize * 2;
    let n = 2048;
    let mut energy = 0f64;
    let mut moment = 0f64;
    for bin in 1..256 {
        let w = std::f64::consts::TAU * bin as f64 / n as f64;
        let (mut re, mut im) = (0., 0.);
        for i in 0..n {
            let x = samples.get(offset + i * 2).copied().unwrap_or(0.) as f64;
            let window = 0.5 - 0.5 * (std::f64::consts::TAU * i as f64 / n as f64).cos();
            re += x * window * (w * i as f64).cos();
            im += x * window * (w * i as f64).sin();
        }
        let power = re * re + im * im;
        energy += power;
        moment += power * bin as f64 * sr as f64 / n as f64;
    }
    (moment / energy) as f32
}

#[test]
fn confirm_resolves_up_error_resolves_down_and_laser_has_a_pew_contour() {
    let confirm = render(&Recipe::new(SoundKind::UiConfirm, 42), 24_000).unwrap();
    let error = render(&Recipe::new(SoundKind::UiError, 42), 24_000).unwrap();
    let laser = render(&Recipe::new(SoundKind::Laser, 42), 24_000).unwrap();
    let first = spectral_centroid(confirm.samples(), 0., 24_000.);
    let last = spectral_centroid(confirm.samples(), 0.22, 24_000.);
    assert!(last > first * 1.15, "confirm: {first} -> {last}");
    let first = spectral_centroid(error.samples(), 0., 24_000.);
    let last = spectral_centroid(error.samples(), 0.19, 24_000.);
    assert!(last < first * 0.95, "error: {first} -> {last}");
    let early = spectral_centroid(laser.samples(), 0., 24_000.);
    let late = spectral_centroid(laser.samples(), 0.2, 24_000.);
    assert!((750.0..1150.0).contains(&early), "laser onset: {early}");
    assert!(
        late < early * 0.8 && late > 250.,
        "laser: {early} -> {late}"
    );
}

#[test]
fn footstep_has_follow_through_and_distinct_seeded_contacts() {
    let a = render(&Recipe::new(SoundKind::Footstep, 42), 24_000).unwrap();
    let b = render(&Recipe::new(SoundKind::Footstep, 43), 24_000).unwrap();
    let early = a.samples()[..2400].iter().map(|x| x * x).sum::<f32>();
    let sole = a.samples()[2400..4800].iter().map(|x| x * x).sum::<f32>();
    // Follow-through is deliberately softer now that the scrape is reduced.
    assert!(sole > early * 0.02, "sole/heel energy: {}", sole / early);
    assert_ne!(a.samples(), b.samples());
}

#[test]
fn earlier_recipes_keep_their_rendering_version() {
    let recipe = Recipe::from_json(include_str!("fixtures/v1-laser.json")).unwrap();
    assert_eq!(recipe.version, 1);
    assert_eq!(Recipe::new(SoundKind::Laser, 42).version, 3);
    let legacy = render(&recipe, 24_000).unwrap();
    assert!(legacy.metrics().peak > 0.7);
    assert_eq!(
        Recipe::from_json(&recipe.to_json().unwrap()).unwrap(),
        recipe
    );
}

#[test]
fn calculator_and_rattle_separate_tones_from_contacts() {
    let mut rattle = Recipe::new(SoundKind::Rattle, 42);
    rattle.genome.noise.gain = 0.;
    rattle.genome.body.gain = 0.;
    assert_eq!(render(&rattle, 24_000).unwrap().metrics().peak, 0.);

    let mut calculator = Recipe::new(SoundKind::Calculator, 42);
    let bleeps = render(&calculator, 24_000).unwrap();
    calculator.genome.noise.gain = 1.5;
    calculator.genome.body.gain = 1.2;
    assert_eq!(
        bleeps.samples(),
        render(&calculator, 24_000).unwrap().samples()
    );
    calculator.genome.tone.amplitude = 0.;
    assert_eq!(render(&calculator, 24_000).unwrap().metrics().peak, 0.);
    calculator.version = 4;
    assert!(calculator.validate().is_err());
}

#[test]
fn breeze_is_quieter_than_surf_and_stays_subtle_after_mutation() {
    for seed in [7, 42, 83, 1024] {
        let wind = render(&Recipe::new(SoundKind::Wind, seed), 24_000).unwrap();
        let waves = render(&Recipe::new(SoundKind::Waves, seed), 24_000).unwrap();
        assert!(
            wind.metrics().rms < waves.metrics().rms * 0.3,
            "seed {seed}: wind {:?}, waves {:?}",
            wind.metrics(),
            waves.metrics()
        );
        let mut session = Session::new(SoundKind::Wind, seed, 4).unwrap();
        session.randomize(1., seed + 1).unwrap();
        for recipe in session.candidates() {
            assert!(render(recipe, 24_000).unwrap().metrics().peak <= 0.180_001);
        }
    }
}

#[test]
fn foliage_softens_high_frequency_edges_independently_of_volume() {
    let roughness = |samples: &[f32]| {
        let mid: Vec<f64> = samples
            .chunks_exact(2)
            .map(|p| (p[0] as f64 + p[1] as f64) * 0.5)
            .collect();
        mid.windows(2).map(|p| (p[1] - p[0]).powi(2)).sum::<f64>()
            / mid.iter().map(|x| x * x).sum::<f64>()
    };
    for (kind, fixture) in [
        (SoundKind::Leaves, include_str!("fixtures/v4-leaves.json")),
        (
            SoundKind::Rustling,
            include_str!("fixtures/v4-rustling.json"),
        ),
    ] {
        for seed in [7, 42, 83, 1024] {
            let mut old = Recipe::from_json(fixture).unwrap();
            old.seed = seed;
            let previous = render(&old, 24_000).unwrap();
            let current = render(&Recipe::new(kind, seed), 24_000).unwrap();
            assert!(
                roughness(current.samples()) < roughness(previous.samples()) * 0.6,
                "{kind:?}, seed {seed}: {} -> {}",
                roughness(previous.samples()),
                roughness(current.samples())
            );
        }
    }
}

#[test]
fn footstep_reduces_hiss_without_merely_turning_everything_down() {
    let previous = Recipe::from_json(include_str!("fixtures/v2-footstep.json")).unwrap();
    let current = Recipe::new(SoundKind::Footstep, 42);
    let a = render(&previous, 24_000).unwrap();
    let b = render(&current, 24_000).unwrap();
    // First-difference energy weights higher frequencies, normalized by total
    // energy so reducing the overall volume alone cannot satisfy this check.
    let roughness = |samples: &[f32]| {
        let mono: Vec<f32> = samples
            .chunks_exact(2)
            .map(|p| (p[0] + p[1]) * 0.5)
            .collect();
        let high = mono.windows(2).map(|w| (w[1] - w[0]).powi(2)).sum::<f32>();
        high / mono.iter().map(|v| v * v).sum::<f32>()
    };
    assert!(roughness(b.samples()) < roughness(a.samples()) * 0.4);
    assert!(b.metrics().rms > a.metrics().rms * 0.25);
}

#[test]
fn laser_pitch_matches_the_reference_across_sample_rates() {
    let recipe = Recipe::new(SoundKind::Laser, 42);
    assert!((recipe.genome.tone.freq_hz - 1030.).abs() < 2.);
    assert!((recipe.genome.envelope.decay_s - 0.3306).abs() < 0.001);
    for sr in [22_050, 48_000, 96_000] {
        let audio = render(&recipe, sr).unwrap();
        for start in [0.01, 0.21] {
            let first = (start * sr as f32) as usize;
            let last = ((start + 0.06) * sr as f32) as usize;
            let crossings = (first + 1..last)
                .filter(|&i| audio.samples()[(i - 1) * 2] <= 0. && audio.samples()[i * 2] > 0.)
                .count();
            let measured = crossings as f32 / 0.06;
            let expected =
                recipe.genome.tone.freq_hz * (-recipe.genome.sweep * (start + 0.03)).exp();
            assert!(
                (measured / expected - 1.).abs() < 0.08,
                "{sr} Hz at {start}s: {measured} vs {expected}"
            );
        }
    }
}

#[test]
fn cinematic_scenes_round_trip_vary_and_stay_bounded_at_extreme_edits() {
    for entry in chirrp::catalog() {
        let mut recipe = Recipe::new(entry.kind, 42);
        if recipe.version < 4 {
            continue;
        }
        let saved = Recipe::from_json(&recipe.to_json().unwrap()).unwrap();
        let original = render(&recipe, 22_050).unwrap();
        assert_eq!(
            original.samples(),
            render(&saved, 22_050).unwrap().samples()
        );
        recipe.seed = 43;
        assert_ne!(
            original.samples(),
            render(&recipe, 22_050).unwrap().samples(),
            "{:?}",
            entry.kind
        );
        recipe.genome.width = 0.;
        let mono = render(&recipe, 22_050).unwrap();
        assert!(mono.samples().chunks_exact(2).all(|p| p[0] == p[1]));
        recipe.genome.width = 1.5;
        recipe.genome.room = 1.;
        recipe.genome.envelope.attack_s = 0.5;
        recipe.genome.envelope.decay_s = 2.;
        recipe.genome.tone.freq_hz = 4000.;
        recipe.genome.filter.cutoff_hz = 16000.;
        recipe.genome.filter.q = 2.;
        recipe.genome.drive = 4.;
        let extreme = render(&recipe, 22_050).unwrap();
        assert!(
            extreme.metrics().duration_seconds < if recipe.version >= 7 { 9. } else { 6. },
            "{:?}: {:?}",
            entry.kind,
            extreme.metrics()
        );
        assert!(
            extreme
                .samples()
                .iter()
                .all(|x| x.is_finite() && x.abs() <= 0.890_001)
        );
        recipe.version = 3;
        assert!(recipe.validate().is_err());
    }
}

#[test]
fn weather_rolls_beyond_contacts_and_birds_live_above_the_rumble() {
    let thunder = render(&Recipe::new(SoundKind::Thunder, 42), 24_000).unwrap();
    let birds = render(&Recipe::new(SoundKind::BirdChirps, 42), 24_000).unwrap();
    let tap = render(&Recipe::new(SoundKind::Tap, 42), 24_000).unwrap();
    let energy = |audio: &chirrp::AudioBuffer, start: f32, end: f32| {
        audio
            .samples()
            .chunks_exact(2)
            .skip((start * 24_000.) as usize)
            .take(((end - start) * 24_000.) as usize)
            .map(|p| ((p[0] + p[1]) * 0.5).powi(2))
            .sum::<f32>()
    };
    assert!(energy(&thunder, 0.8, 2.) > energy(&thunder, 0., 0.2) * 0.1);
    assert!(energy(&tap, 0.8, 1.2) < energy(&tap, 0., 0.2) * 0.001);
    assert!(spectral_centroid(birds.samples(), 0.02, 24_000.) > 1500.);
    assert!(spectral_centroid(thunder.samples(), 0.8, 24_000.) < 500.);
}

#[test]
fn dodge_crosses_the_stereo_field_without_losing_its_mono_body() {
    let mut recipe = Recipe::new(SoundKind::Dodge, 42);
    recipe.genome.room = 0.;
    let wide = render(&recipe, 24_000).unwrap();
    let balance = |start: f32, end: f32| {
        wide.samples()
            .chunks_exact(2)
            .skip((start * 24_000.) as usize)
            .take(((end - start) * 24_000.) as usize)
            .map(|p| p[1].powi(2) - p[0].powi(2))
            .sum::<f32>()
    };
    assert!(balance(0.01, 0.07) < 0.);
    assert!(balance(0.3, 0.4) > 0.);
    recipe.genome.width = 0.;
    let mono = render(&recipe, 24_000).unwrap();
    let (mut mm, mut ww, mut mw) = (0f64, 0f64, 0f64);
    for (m, w) in mono
        .samples()
        .chunks_exact(2)
        .zip(wide.samples().chunks_exact(2))
    {
        let a = m[0] as f64;
        let b = (w[0] as f64 + w[1] as f64) * 0.5;
        mm += a * a;
        ww += b * b;
        mw += a * b;
    }
    assert!(mw / (mm * ww).sqrt() > 0.999_999);
}

#[test]
fn car_idle_chugs_steadily_instead_of_cracking_like_thunder() {
    let engine = render(&Recipe::new(SoundKind::CarEngineRumble, 42), 24_000).unwrap();
    let thunder = render(&Recipe::new(SoundKind::Thunder, 42), 24_000).unwrap();
    let energy = |audio: &chirrp::AudioBuffer, start: f32, end: f32| {
        audio
            .samples()
            .chunks_exact(2)
            .skip((start * 24_000.) as usize)
            .take(((end - start) * 24_000.) as usize)
            .map(|p| ((p[0] + p[1]) * 0.5).powi(2))
            .sum::<f32>()
    };
    let roughness = |samples: &[f32], start: f32, end: f32| {
        let mid: Vec<f32> = samples
            .chunks_exact(2)
            .skip((start * 24_000.) as usize)
            .take(((end - start) * 24_000.) as usize)
            .map(|p| (p[0] + p[1]) * 0.5)
            .collect();
        mid.windows(2).map(|w| (w[1] - w[0]).powi(2)).sum::<f32>()
            / mid.iter().map(|v| v * v).sum::<f32>()
    };
    assert!(energy(&engine, 0.7, 1.3) > energy(&engine, 0.0, 0.12) * 0.45);
    assert!(
        spectral_centroid(engine.samples(), 0.4, 24_000.)
            < spectral_centroid(thunder.samples(), 0.04, 24_000.) * 0.55
    );
    assert!(roughness(engine.samples(), 0.05, 0.2) < roughness(thunder.samples(), 0.0, 0.12) * 0.7);
    let mut pulsy = Recipe::new(SoundKind::CarEngineRumble, 42);
    pulsy.genome.tone.amplitude = 0.;
    assert!(render(&pulsy, 24_000).unwrap().metrics().rms > 0.02);
}

#[test]
fn seagull_is_a_harsh_kee_aww() {
    let mut recipe = Recipe::new(SoundKind::Seagull, 42);
    recipe.genome.room = 0.;
    let cry = render(&recipe, 24_000).unwrap();
    let birds = render(&Recipe::new(SoundKind::BirdChirps, 42), 24_000).unwrap();
    let ring = render(&Recipe::new(SoundKind::Ring, 42), 24_000).unwrap();
    let seconds = cry.metrics().duration_seconds;
    assert!(
        (0.55..1.35).contains(&seconds),
        "seagull duration {seconds}"
    );
    let early = spectral_centroid(cry.samples(), 0.04, 24_000.);
    assert!(early > 800. && early < 2600., "kya centroid {early}");
    let energy = |start: f32, end: f32| {
        cry.samples()
            .chunks_exact(2)
            .skip((start * 24_000.) as usize)
            .take(((end - start) * 24_000.) as usize)
            .map(|p| ((p[0] + p[1]) * 0.5).powi(2))
            .sum::<f32>()
    };
    assert!(energy(0.22, 0.40) > energy(0.00, 0.14) * 1.2);
    assert!(early < spectral_centroid(birds.samples(), 0.02, 24_000.));
    let roughness = |samples: &[f32]| {
        let mid: Vec<f32> = samples
            .chunks_exact(2)
            .map(|p| (p[0] + p[1]) * 0.5)
            .collect();
        mid.windows(2).map(|w| (w[1] - w[0]).powi(2)).sum::<f32>()
            / mid.iter().map(|v| v * v).sum::<f32>()
    };
    assert!(roughness(cry.samples()) > roughness(ring.samples()) * 1.3);
    assert!(cry.metrics().stereo_correlation > 0.9);
}
