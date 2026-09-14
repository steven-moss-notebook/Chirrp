use chirrp::{Recipe, SoundKind, render};

// Correlation is normalized by energy, so making a source louder cannot pass.
fn periodicity(samples: &[f32], sr: u32, start: f32, span: f32, hz: (f32, f32)) -> (f32, f32) {
    let offset = (start * sr as f32) as usize * 2;
    let n = (span * sr as f32) as usize;
    let mut best = (-1., 0.);
    for lag in (sr as f32 / hz.1) as usize..=(sr as f32 / hz.0) as usize {
        let (mut xy, mut xx, mut yy) = (0f64, 0f64, 0f64);
        for i in 0..n {
            let x = samples[offset + i * 2] as f64;
            let y = samples[offset + (i + lag) * 2] as f64;
            xy += x * y;
            xx += x * x;
            yy += y * y;
        }
        let correlation = (xy / (xx * yy).sqrt()) as f32;
        if correlation > best.0 {
            best = (correlation, sr as f32 / lag as f32);
        }
    }
    best
}

#[test]
fn gull_harmonics_follow_one_falling_voice_across_rates_and_seeds() {
    for sr in [22_050, 48_000, 96_000] {
        for seed in [42, 108, 791] {
            let mut recipe = Recipe::new(SoundKind::Seagull, seed);
            recipe.genome.room = 0.;
            recipe.genome.width = 0.;
            let audio = render(&recipe, sr).unwrap();
            let early = periodicity(audio.samples(), sr, 0.29, 0.04, (950., 1400.));
            let late = periodicity(audio.samples(), sr, 0.65, 0.04, (750., 1100.));
            assert!(
                early.0 > 0.85 && late.0 > 0.8,
                "{sr}/{seed}: {early:?}, {late:?}"
            );
            assert!(late.1 < early.1 * 0.85, "{sr}/{seed}: {early:?}, {late:?}");
        }
    }
}

#[test]
fn engine_has_combustion_rate_periodicity_and_sustained_energy() {
    for sr in [22_050, 48_000, 96_000] {
        for seed in [42, 108, 791] {
            let recipe = Recipe::new(SoundKind::CarEngineRumble, seed);
            let audio = render(&recipe, sr).unwrap();
            let pulse = periodicity(audio.samples(), sr, 0.3, 0.6, (40., 60.));
            // Alternating bank delays and uneven cylinder strength reduce
            // single-firing correlation; a stable shared clock still dominates.
            assert!(pulse.0 > 0.5, "{sr}/{seed}: {pulse:?}");
            assert!((47.0..53.0).contains(&pulse.1), "{sr}/{seed}: {pulse:?}");
            let energies: Vec<f32> = (2..15)
                .map(|window| {
                    let start = (window as f32 * 0.1 * sr as f32) as usize * 2;
                    audio.samples()[start..start + sr as usize / 10 * 2]
                        .iter()
                        .map(|x| x * x)
                        .sum()
                })
                .collect();
            let quiet = energies.iter().copied().fold(f32::MAX, f32::min);
            let loud = energies.iter().copied().fold(0., f32::max);
            assert!(
                quiet / loud > 0.45,
                "{sr}/{seed}: energy ratio {}",
                quiet / loud
            );
            let mut faster = recipe.clone();
            faster.genome.tone.freq_hz *= 1.5;
            let fast = render(&faster, sr).unwrap();
            let pulse = periodicity(fast.samples(), sr, 0.3, 0.6, (65., 85.));
            assert!((71.0..79.0).contains(&pulse.1), "edited RPM: {pulse:?}");
        }
    }
}

#[test]
fn saved_gull_and_engine_keep_v4_signal_baselines() {
    for (fixture, rms, duration) in [
        (
            include_str!("fixtures/v4-seagull.json"),
            0.16447718,
            0.88102084,
        ),
        (
            include_str!("fixtures/v4-car-engine-rumble.json"),
            0.11582162,
            1.6980417,
        ),
    ] {
        let old = Recipe::from_json(fixture).unwrap();
        assert_eq!(old.version, 4);
        assert_eq!(Recipe::new(old.kind, old.seed).version, 5);
        let metrics = render(&old, 48_000).unwrap().metrics();
        assert!((metrics.rms - rms).abs() < 1e-5, "{metrics:?}");
        assert!(
            (metrics.duration_seconds - duration).abs() < 1e-5,
            "{metrics:?}"
        );
    }
}
