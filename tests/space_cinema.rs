use chirrp::{
    MixLayer, Recipe, RenderOptions, SoundKind, render, render_loop,
    render_mix_layers_with_options, render_with_options,
};

fn previous() -> Vec<Recipe> {
    serde_json::from_str(include_str!("fixtures/pre-cinema-bank.json")).unwrap()
}

#[test]
fn original_41_presets_are_unchanged_and_saved_space_recipes_keep_v6() {
    let mut originals = 0;
    let mut space = 0;
    for recipe in previous() {
        let current = Recipe::new(recipe.kind, recipe.seed);
        if recipe.version < 6 {
            originals += 1;
            assert_eq!(current, recipe);
            assert_eq!(
                render(&recipe, 24_000).unwrap().samples(),
                render(&current, 24_000).unwrap().samples()
            );
        } else {
            space += 1;
            assert_eq!(current.version, 7);
            let saved = Recipe::from_json(&recipe.to_json().unwrap()).unwrap();
            assert_eq!(saved.version, 6);
            assert_ne!(
                render(&saved, 24_000).unwrap().samples(),
                render(&current, 24_000).unwrap().samples()
            );
        }
    }
    assert_eq!((originals, space), (41, 26));
}

#[test]
fn weapon_strikes_have_an_attack_and_a_body() {
    let options = RenderOptions { dry_mid: true };
    for kind in [
        SoundKind::PlasmaPulse,
        SoundKind::HeavySlug,
        SoundKind::AnvilPulse,
    ] {
        for seed in [7, 42, 108] {
            let audio = render_with_options(&Recipe::new(kind, seed), 24_000, options).unwrap();
            let m = mid(audio.samples());
            let peak = m.iter().fold(0f32, |p, x| p.max(x.abs()));
            let attack = m.iter().take(2400).fold(0f32, |p, x| p.max(x.abs()));
            let body: Vec<_> = m.iter().skip(3600).take(8000).copied().collect();
            let body_rms = (body.iter().map(|s| (*s as f64).powi(2)).sum::<f64>()
                / body.len().max(1) as f64)
                .sqrt() as f32;
            assert!(audio.metrics().peak <= 0.890_001, "{kind:?} seed {seed}");
            assert!(attack > peak * 0.55, "{kind:?} seed {seed}: attack buried");
            assert!(
                body_rms > 0.008,
                "{kind:?} seed {seed}: body too quiet {body_rms}"
            );
        }
    }
}

#[test]
fn cinematic_room_adds_a_diffuse_tail_while_dry_exports_keep_the_source() {
    let mut recipe = Recipe::new(SoundKind::HeavySlug, 42);
    let dry = render_with_options(&recipe, 24_000, RenderOptions { dry_mid: true }).unwrap();
    let wet = render(&recipe, 24_000).unwrap();
    let tail = &wet.samples()[dry.samples().len()..];
    assert!(tail.len() > 24_000);
    assert!(tail.iter().any(|s| s.abs() > 0.0001));
    assert!(tail.chunks_exact(2).any(|p| (p[0] - p[1]).abs() > 0.0001));
    recipe.genome.room = 1.;
    recipe.genome.width = 1.5;
    assert_eq!(
        dry.samples(),
        render_with_options(&recipe, 24_000, RenderOptions { dry_mid: true })
            .unwrap()
            .samples()
    );
}

#[test]
fn wide_continuous_sources_keep_the_low_body_centered() {
    for kind in [
        SoundKind::HullRumble,
        SoundKind::GravityDrone,
        SoundKind::ChoirInterval,
    ] {
        let audio = render_loop(&Recipe::new(kind, 42), 24_000, 2.).unwrap();
        let mut m = 0.;
        let mut s = 0.;
        let mut me = 0f64;
        let mut se = 0f64;
        let c = 1. - (-std::f32::consts::TAU * 100. / 24_000.).exp();
        for p in audio.samples().chunks_exact(2) {
            m += c * ((p[0] + p[1]) * 0.5 - m);
            s += c * ((p[0] - p[1]) * 0.5 - s);
            me += (m as f64).powi(2);
            se += (s as f64).powi(2);
        }
        assert!(se / me < 0.08, "{kind:?}: bass side/mid {}", se / me);
    }
}

fn mid(samples: &[f32]) -> Vec<f32> {
    samples
        .chunks_exact(2)
        .map(|p| (p[0] + p[1]) * 0.5)
        .collect()
}

fn filtered_energy(samples: &[f32], sr: f32, hz: f32, highpass: bool) -> f64 {
    let c = 1. - (-std::f32::consts::TAU * hz / sr).exp();
    let mut y = 0.;
    let mut energy = 0f64;
    for &x in samples {
        y += c * (x - y);
        let v = if highpass { x - y } else { y };
        energy += (v as f64).powi(2);
    }
    energy
}

fn periodicity(samples: &[f32], sr: f32, hz: f32) -> f64 {
    let lag = (sr / hz).round() as usize;
    if samples.len() <= lag {
        return 0.;
    }
    let mut num = 0f64;
    let mut den_a = 0f64;
    let mut den_b = 0f64;
    for i in 0..samples.len() - lag {
        let a = samples[i] as f64;
        let b = samples[i + lag] as f64;
        num += a * b;
        den_a += a * a;
        den_b += b * b;
    }
    num / (den_a * den_b).sqrt().max(1e-12)
}

#[test]
fn space_identities_keep_distinct_spectra_and_periodicity() {
    let options = RenderOptions { dry_mid: true };
    let pulse =
        render_with_options(&Recipe::new(SoundKind::PlasmaPulse, 42), 24_000, options).unwrap();
    let slug =
        render_with_options(&Recipe::new(SoundKind::HeavySlug, 42), 24_000, options).unwrap();
    let ice =
        render_with_options(&Recipe::new(SoundKind::IceShatter, 42), 24_000, options).unwrap();
    let debris =
        render_with_options(&Recipe::new(SoundKind::DebrisClatter, 42), 24_000, options).unwrap();
    let pulse_m = mid(pulse.samples());
    let slug_m = mid(slug.samples());
    let ice_m = mid(ice.samples());
    let debris_m = mid(debris.samples());
    let ratio = |s: &[f32]| {
        filtered_energy(s, 24_000., 2500., true)
            / filtered_energy(s, 24_000., 120., false).max(1e-12)
    };
    assert!(
        ratio(&pulse_m) > ratio(&slug_m) * 1.8,
        "plasma {} vs slug {}",
        ratio(&pulse_m),
        ratio(&slug_m)
    );
    let crack = |s: &[f32]| {
        let n = s.len().min(1200);
        filtered_energy(&s[..n], 24_000., 800., true)
            / s[..n]
                .iter()
                .map(|x| (*x as f64).powi(2))
                .sum::<f64>()
                .max(1e-12)
    };
    assert!(
        crack(&ice_m) > crack(&debris_m) * 1.2,
        "ice onset {} vs debris {}",
        crack(&ice_m),
        crack(&debris_m)
    );
    assert!(ice.metrics().duration_seconds < debris.metrics().duration_seconds);
    let bass_vs_dust = |s: &[f32]| {
        filtered_energy(s, 24_000., 350., false)
            / filtered_energy(s, 24_000., 2500., true).max(1e-12)
    };
    assert!(
        bass_vs_dust(&debris_m) > bass_vs_dust(&ice_m) * 1.4,
        "debris {} vs ice {}",
        bass_vs_dust(&debris_m),
        bass_vs_dust(&ice_m)
    );
    let beam = render_loop(&Recipe::new(SoundKind::BeamLoop, 42), 24_000, 2.).unwrap();
    let furnace = render_loop(&Recipe::new(SoundKind::FurnaceBed, 42), 24_000, 2.).unwrap();
    let beam_p = periodicity(&mid(beam.samples()), 24_000., 92.);
    let furnace_p = periodicity(&mid(furnace.samples()), 24_000., 92.);
    assert!(
        beam_p > furnace_p + 0.12,
        "beam periodicity {beam_p} vs furnace {furnace_p}"
    );
}

fn frame_energy_var(samples: &[f32], hop: usize) -> f64 {
    let frames: Vec<f64> = samples
        .chunks(hop)
        .map(|c| (c.iter().map(|x| (*x as f64).powi(2)).sum::<f64>() / c.len() as f64).sqrt())
        .collect();
    let mean = frames.iter().sum::<f64>() / frames.len().max(1) as f64;
    frames.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / frames.len().max(1) as f64
}

#[test]
fn continuous_beds_have_separate_source_identities() {
    let looped = |kind| render_loop(&Recipe::new(kind, 42), 24_000, 2.).unwrap();
    let gravity = mid(looped(SoundKind::GravityDrone).samples());
    let hull = mid(looped(SoundKind::HullRumble).samples());
    let vacuum = mid(looped(SoundKind::VacuumLoop).samples());
    let magnet = mid(looped(SoundKind::MagnetPulse).samples());
    let furnace = mid(looped(SoundKind::FurnaceBed).samples());
    let nanite = mid(looped(SoundKind::NaniteHiss).samples());
    let bright = |s: &[f32]| {
        filtered_energy(s, 24_000., 1800., true)
            / filtered_energy(s, 24_000., 120., false).max(1e-12)
    };
    assert!(
        bright(&nanite) > bright(&vacuum) * 1.6,
        "nanite {} vs vacuum {}",
        bright(&nanite),
        bright(&vacuum)
    );
    assert!(
        bright(&vacuum) > bright(&gravity) * 2.,
        "vacuum {} vs gravity {}",
        bright(&vacuum),
        bright(&gravity)
    );
    assert!(
        bright(&furnace) > bright(&gravity) * 1.5,
        "furnace {} vs gravity {}",
        bright(&furnace),
        bright(&gravity)
    );
    assert!(
        bright(&hull) > bright(&gravity),
        "hull {} vs gravity {}",
        bright(&hull),
        bright(&gravity)
    );
    let gravity_p = periodicity(&gravity, 24_000., 31.2);
    let furnace_p = periodicity(&furnace, 24_000., 31.2);
    assert!(
        gravity_p > furnace_p + 0.15,
        "gravity well {gravity_p} vs furnace {furnace_p}"
    );
    let magnet_mod = frame_energy_var(&magnet, 2400);
    let hull_mod = frame_energy_var(&hull, 2400);
    assert!(
        magnet_mod > hull_mod * 1.4,
        "magnet cadence {magnet_mod} vs hull {hull_mod}"
    );
}

#[test]
fn beam_ignite_holds_the_loop_hum() {
    let ignite = render_with_options(
        &Recipe::new(SoundKind::BeamIgnite, 42),
        24_000,
        RenderOptions { dry_mid: true },
    )
    .unwrap();
    let m = mid(ignite.samples());
    let start = (0.28 * 24_000.) as usize;
    let held = &m[start.min(m.len().saturating_sub(1))..(start + 7200).min(m.len())];
    let p = periodicity(held, 24_000., 92.);
    assert!(p > 0.12, "ignite hold periodicity {p}");
}

#[test]
fn gameplay_cues_stay_audible_and_on_source() {
    let dry = RenderOptions { dry_mid: true };
    let anvil = render_with_options(&Recipe::new(SoundKind::AnvilPulse, 42), 24_000, dry).unwrap();
    assert!(anvil.metrics().duration_seconds > 0.9);
    assert!(anvil.mono_metrics().rms > 0.03);

    let mut magnet = Recipe::new(SoundKind::MagnetPulse, 42);
    magnet.genome.envelope.attack_s = 0.04;
    magnet.genome.envelope.decay_s = 2.1;
    magnet.genome.envelope.release_s = 0.2;
    magnet.genome.envelope.sustain_level = 0.62;
    let mut tooth = Recipe::new(SoundKind::AnvilPulse, 59);
    tooth.genome.envelope.decay_s = 0.28;
    let clockwork = render_mix_layers_with_options(
        &[
            MixLayer::new(magnet, 0.55, 0.),
            MixLayer::new(tooth.clone(), 0.24, 0.),
            MixLayer::new(tooth, 0.2, 0.5),
        ],
        24_000,
        dry,
    )
    .unwrap();
    assert!(clockwork.mono_metrics().rms > 0.03);
    assert!(clockwork.mono_metrics().peak > 0.15);

    let mut rumble = Recipe::new(SoundKind::HullRumble, 42);
    rumble.genome.envelope.attack_s = 0.06;
    rumble.genome.envelope.decay_s = 3.4;
    rumble.genome.envelope.release_s = 0.3;
    rumble.genome.envelope.sustain_level = 0.22;
    rumble.genome.room = 0.48;
    let horn = |seed, hz| {
        let mut recipe = Recipe::new(SoundKind::EnergyShield, seed);
        recipe.genome.tone.freq_hz = hz;
        recipe.genome.envelope.attack_s = 0.012;
        recipe.genome.envelope.decay_s = 0.36;
        recipe.genome.envelope.release_s = 0.08;
        recipe.genome.width = 1.42;
        recipe.genome.room = 0.62;
        recipe.genome.tone.amplitude = 0.42;
        recipe.genome.noise.gain = 1.3;
        recipe.genome.texture = 0.75;
        recipe
    };
    let alarm = render_mix_layers_with_options(
        &[
            MixLayer::new(rumble, 0.12, 0.),
            MixLayer::new(horn(59, 118.), 0.7, 0.),
            MixLayer::new(horn(108, 78.), 0.74, 0.16),
            MixLayer::new(horn(211, 118.), 0.7, 0.62),
            MixLayer::new(horn(17, 78.), 0.74, 0.78),
            MixLayer::new(horn(83, 118.), 0.68, 1.24),
            MixLayer::new(horn(241, 78.), 0.72, 1.4),
        ],
        24_000,
        RenderOptions { dry_mid: false },
    )
    .unwrap();
    assert!(alarm.mono_metrics().peak > 0.22);
    assert!(alarm.mono_metrics().rms > 0.02);
    assert!(alarm.metrics().duration_seconds > 2.4);
    let wide = stereo_highpass_correlation(alarm.samples(), 24_000., 400.);
    assert!(wide < 0.86, "low health highs still centered {wide}");
    let magnet = render_loop(&Recipe::new(SoundKind::MagnetPulse, 42), 24_000, 2.).unwrap();
    let vacuum = render_loop(&Recipe::new(SoundKind::VacuumLoop, 42), 24_000, 2.).unwrap();
    let alarm_m = first_mid(alarm.samples(), 48_000);
    let alarm_vs_magnet = corr(&alarm_m, &first_mid(magnet.samples(), 48_000));
    let alarm_vs_vacuum = corr(&alarm_m, &first_mid(vacuum.samples(), 48_000));
    assert!(
        alarm_vs_magnet < 0.45,
        "low health still matches magnet pulse {alarm_vs_magnet}"
    );
    assert!(
        alarm_vs_vacuum < 0.5,
        "low health still a vacuum bed {alarm_vs_vacuum}"
    );
    let alarm_var = frame_energy_var(&mid(alarm.samples()), 2400);
    let vacuum_var = frame_energy_var(&mid(vacuum.samples()), 2400);
    assert!(
        alarm_var > vacuum_var * 1.8,
        "low health has no warning cadence {alarm_var} vs {vacuum_var}"
    );

    let mut choir = Recipe::new(SoundKind::ChoirInterval, 42);
    choir.genome.envelope.attack_s = 0.18;
    choir.genome.envelope.decay_s = 1.35;
    choir.genome.envelope.release_s = 0.35;
    choir.genome.envelope.sustain_level = 0.28;
    let moss = render_mix_layers_with_options(
        &[
            MixLayer::new(choir, 0.38, 0.),
            MixLayer::new(Recipe::new(SoundKind::Leaves, 59), 0.22, 0.04),
        ],
        24_000,
        dry,
    )
    .unwrap();
    let ring = render_with_options(&Recipe::new(SoundKind::Ring, 42), 24_000, dry).unwrap();
    let moss_bell = periodicity(&mid(moss.samples()), 24_000., 330.);
    let ring_bell = periodicity(&mid(ring.samples()), 24_000., 330.);
    assert!(moss.mono_metrics().peak > 0.08);
    assert!(
        moss_bell < ring_bell * 0.7,
        "moss still rings like a bell {moss_bell} vs {ring_bell}"
    );
}

fn corr(a: &[f32], b: &[f32]) -> f64 {
    let n = a.len().min(b.len());
    if n == 0 {
        return 1.;
    }
    let (mut num, mut da, mut db) = (0f64, 0f64, 0f64);
    for i in 0..n {
        let x = a[i] as f64;
        let y = b[i] as f64;
        num += x * y;
        da += x * x;
        db += y * y;
    }
    num / (da * db).sqrt().max(1e-12)
}

fn first_mid(samples: &[f32], frames: usize) -> Vec<f32> {
    mid(samples).into_iter().take(frames).collect()
}

#[test]
fn movement_cues_are_not_thruster_clones() {
    let dry = RenderOptions { dry_mid: true };
    let mut slide = Recipe::new(SoundKind::Slide, 59);
    slide.genome.envelope.decay_s = 0.32;
    let dash = render_mix_layers_with_options(
        &[
            MixLayer::new(Recipe::new(SoundKind::Dodge, 42), 0.7, 0.),
            MixLayer::new(slide, 0.45, 0.02),
        ],
        24_000,
        dry,
    )
    .unwrap();
    let mut shield = Recipe::new(SoundKind::EnergyShield, 59);
    shield.genome.envelope.attack_s = 0.05;
    shield.genome.envelope.decay_s = 0.42;
    let jump = render_mix_layers_with_options(
        &[
            MixLayer::new(Recipe::new(SoundKind::Jump, 42), 0.8, 0.),
            MixLayer::new(shield, 0.4, 0.02),
        ],
        24_000,
        dry,
    )
    .unwrap();
    let thruster = render_with_options(&Recipe::new(SoundKind::Thruster, 42), 24_000, dry).unwrap();
    let dash_m = first_mid(dash.samples(), 9600);
    let jump_m = first_mid(jump.samples(), 9600);
    let thruster_m = first_mid(thruster.samples(), 9600);
    let dash_thruster = corr(&dash_m, &thruster_m);
    let jump_thruster = corr(&jump_m, &thruster_m);
    let dash_jump = corr(&dash_m, &jump_m);
    assert!(
        dash_thruster < 0.55,
        "dash still matches thruster {dash_thruster}"
    );
    assert!(
        jump_thruster < 0.55,
        "jump pad still matches thruster {jump_thruster}"
    );
    assert!(dash_jump < 0.72, "dash still matches jump pad {dash_jump}");
}

fn stereo_highpass_correlation(samples: &[f32], sr: f32, hz: f32) -> f32 {
    let c = 1. - (-std::f32::consts::TAU * hz / sr).exp();
    let mut yl = 0.;
    let mut yr = 0.;
    let (mut ll, mut rr, mut lr) = (0f64, 0f64, 0f64);
    for p in samples.chunks_exact(2) {
        yl += c * (p[0] - yl);
        yr += c * (p[1] - yr);
        let l = (p[0] - yl) as f64;
        let r = (p[1] - yr) as f64;
        ll += l * l;
        rr += r * r;
        lr += l * r;
    }
    if ll * rr > 1e-20 {
        (lr / (ll * rr).sqrt()).clamp(-1., 1.) as f32
    } else {
        1.
    }
}

#[test]
fn ricochet_and_rocket_keep_wide_room_images() {
    let ricochet = render(&Recipe::new(SoundKind::Ricochet, 42), 24_000).unwrap();
    let rocket = render(&Recipe::new(SoundKind::RocketLaunch, 42), 24_000).unwrap();
    let ricochet_w = stereo_highpass_correlation(ricochet.samples(), 24_000., 400.);
    let rocket_w = stereo_highpass_correlation(rocket.samples(), 24_000., 400.);
    assert!(
        ricochet_w < 0.82,
        "ricochet highs still centered {ricochet_w}"
    );
    assert!(rocket_w < 0.82, "rocket highs still centered {rocket_w}");
    assert!(Recipe::new(SoundKind::Ricochet, 42).genome.room > 0.4);
    assert!(Recipe::new(SoundKind::RocketLaunch, 42).genome.room > 0.5);
    assert!(Recipe::new(SoundKind::Ricochet, 42).genome.width > 1.2);
    assert!(Recipe::new(SoundKind::RocketLaunch, 42).genome.width > 1.2);
}

#[test]
fn siren_lock_scans_then_locks() {
    let audio = render_with_options(
        &Recipe::new(SoundKind::SirenLock, 42),
        24_000,
        RenderOptions { dry_mid: true },
    )
    .unwrap();
    let m = mid(audio.samples());
    let scan = &m[..m.len().min(9600)];
    let lock = &m[m.len().min(11040)..m.len().min(18000)];
    let scan_rms =
        (scan.iter().map(|s| (*s as f64).powi(2)).sum::<f64>() / scan.len().max(1) as f64).sqrt();
    let lock_rms =
        (lock.iter().map(|s| (*s as f64).powi(2)).sum::<f64>() / lock.len().max(1) as f64).sqrt();
    assert!(scan_rms > 0.008, "lock-on scan too quiet {scan_rms}");
    assert!(lock_rms > 0.01, "lock strike too quiet {lock_rms}");
    assert!(audio.metrics().duration_seconds > 0.9);
}

#[test]
fn rain_weapon_is_a_wide_storm_not_a_plasma_packet() {
    let mut rain = Recipe::new(SoundKind::Rain, 42);
    rain.genome.width = 1.42;
    rain.genome.room = 0.7;
    rain.genome.envelope.attack_s = 0.08;
    rain.genome.envelope.decay_s = 1.45;
    rain.genome.filter.cutoff_hz = 5200.;
    let mut arc = Recipe::new(SoundKind::ArcZap, 59);
    arc.genome.width = 1.42;
    arc.genome.room = 0.5;
    let storm = render_mix_layers_with_options(
        &[MixLayer::new(rain, 0.7, 0.), MixLayer::new(arc, 0.58, 0.05)],
        24_000,
        RenderOptions { dry_mid: false },
    )
    .unwrap();
    let plasma = render(&Recipe::new(SoundKind::PlasmaPulse, 42), 24_000).unwrap();
    let vs_plasma = corr(
        &first_mid(storm.samples(), 24_000),
        &first_mid(plasma.samples(), 24_000),
    );
    assert!(
        vs_plasma < 0.5,
        "rain weapon still matches plasma {vs_plasma}"
    );
    let wide = stereo_highpass_correlation(storm.samples(), 24_000., 400.);
    assert!(wide < 0.85, "rain weapon highs still centered {wide}");
    assert!(storm.metrics().duration_seconds > 1.2);
}

#[test]
fn xp_pickup_stays_short_quiet_and_unpitched() {
    let mut tick = Recipe::new(SoundKind::UiHover, 42);
    tick.genome.tone.freq_hz = 520.;
    tick.genome.envelope.decay_s = 0.04;
    tick.genome.room = 0.;
    tick.genome.width = 0.;
    tick.genome.filter.cutoff_hz = 1600.;
    let xp = render_mix_layers_with_options(
        &[MixLayer::new(tick, 0.9, 0.)],
        24_000,
        RenderOptions { dry_mid: true },
    )
    .unwrap();
    let ring = render_with_options(
        &Recipe::new(SoundKind::Ring, 42),
        24_000,
        RenderOptions { dry_mid: true },
    )
    .unwrap();
    let pickup = render_with_options(
        &Recipe::new(SoundKind::Pickup, 42),
        24_000,
        RenderOptions { dry_mid: true },
    )
    .unwrap();
    assert!(xp.metrics().duration_seconds < 0.12);
    assert!(xp.mono_metrics().peak < 0.08);
    let xp_bell = periodicity(&mid(xp.samples()), 24_000., 330.);
    let ring_bell = periodicity(&mid(ring.samples()), 24_000., 330.);
    let pickup_fifth = periodicity(&mid(pickup.samples()), 24_000., 990.);
    let xp_fifth = periodicity(&mid(xp.samples()), 24_000., 780.);
    assert!(
        xp_bell < ring_bell * 0.5,
        "xp still rings {xp_bell} vs {ring_bell}"
    );
    assert!(
        xp_fifth < pickup_fifth * 0.85,
        "xp still a pickup dyad {xp_fifth} vs {pickup_fifth}"
    );
}
