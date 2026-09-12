use chirrp::{Command, Engine, Recipe, Session, SoundKind, catalog, render};

#[test]
fn every_preset_has_finite_audible_stereo_and_clean_edges() {
    for entry in catalog() {
        let audio = render(&Recipe::new(entry.kind, 42), 24_000).unwrap();
        let m = audio.metrics();
        assert!(
            audio.samples().iter().all(|x| x.is_finite()),
            "{:?}",
            entry.kind
        );
        assert!(
            m.peak
                > if entry.kind == SoundKind::UiHover {
                    0.005
                } else {
                    0.03
                }
                && m.peak <= 0.890_001,
            "{:?}: {m:?}",
            entry.kind
        );
        assert!(m.rms > 0.001, "{:?}: {m:?}", entry.kind);
        assert!(
            audio
                .samples()
                .chunks_exact(2)
                .any(|p| (p[0] - p[1]).abs() > 0.0001),
            "{:?}",
            entry.kind
        );
        assert_eq!(&audio.samples()[..2], &[0., 0.]);
        assert_eq!(&audio.samples()[audio.samples().len() - 2..], &[0., 0.]);
        assert_eq!(audio.channels(), 2);
        assert!(m.duration_seconds < 6.);
    }
}

#[test]
fn render_is_repeatable_and_recipes_round_trip() {
    let recipe = Recipe::new(SoundKind::Explosion, 91);
    let back = Recipe::from_json(&recipe.to_json().unwrap()).unwrap();
    assert_eq!(recipe, back);
    assert_eq!(
        render(&recipe, 24_000).unwrap().samples(),
        render(&back, 24_000).unwrap().samples()
    );
    let mut other = recipe.clone();
    other.seed += 1;
    assert_ne!(
        render(&recipe, 24_000).unwrap().samples(),
        render(&other, 24_000).unwrap().samples()
    );
}

#[test]
fn width_zero_is_mono_and_widening_keeps_the_mid_signal() {
    let mut recipe = Recipe::new(SoundKind::Laser, 42);
    recipe.genome.width = 0.;
    let mono = render(&recipe, 24_000).unwrap();
    assert!(mono.samples().chunks_exact(2).all(|p| p[0] == p[1]));
    recipe.genome.width = 1.5;
    let wide = render(&recipe, 24_000).unwrap();
    // Linked mastering may apply a different scalar gain. The recovered mid
    // must otherwise match, without comb filtering from a delayed dry channel.
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
fn wav_header_and_payload_match_stereo_pcm16() {
    let a = render(&Recipe::new(SoundKind::UiClick, 1), 48_000).unwrap();
    let wav = a.wav_bytes();
    assert_eq!(&wav[..4], b"RIFF");
    assert_eq!(&wav[8..16], b"WAVEfmt ");
    assert_eq!(u16::from_le_bytes(wav[20..22].try_into().unwrap()), 1);
    assert_eq!(u16::from_le_bytes(wav[22..24].try_into().unwrap()), 2);
    assert_eq!(u32::from_le_bytes(wav[24..28].try_into().unwrap()), 48_000);
    assert_eq!(u32::from_le_bytes(wav[28..32].try_into().unwrap()), 192_000);
    assert_eq!(
        u32::from_le_bytes(wav[40..44].try_into().unwrap()) as usize,
        a.frames() * 4
    );
    assert_eq!(wav.len(), 44 + a.frames() * 4);
}

#[test]
fn favorite_survives_randomization_and_ratings_preserve_the_winner() {
    let mut s = Session::new(SoundKind::Laser, 12, 6).unwrap();
    s.select(3).unwrap();
    let favorite = s.favorite().clone();
    s.randomize(0.8, 99).unwrap();
    assert_eq!(s.candidates()[0], favorite);
    assert_eq!(s.selected(), 0);
    assert!(
        s.candidates()[1..]
            .iter()
            .any(|p| p.genome != favorite.genome)
    );
    let winner = s.candidates()[4].clone();
    s.evolve(&[0., 0., 0., 0., 1., 0.], 0.6, 122).unwrap();
    assert_eq!(s.candidates()[0], winner);
    assert_eq!(s.generation(), 2);
}

#[test]
fn evolution_is_repeatable_and_restorable() {
    let mut a = Session::new(SoundKind::Footstep, 14, 6).unwrap();
    a.randomize(0.5, 7).unwrap();
    let mut b = Session::from_json(&a.to_json().unwrap()).unwrap();
    for seed in 0..20 {
        a.randomize(0.7, seed).unwrap();
        b.randomize(0.7, seed).unwrap();
    }
    assert_eq!(a, b);
    a.randomize(0., 56).unwrap();
    assert!(a.candidates().iter().all(|p| p == a.favorite()));
}

#[test]
fn invalid_requests_are_errors_and_do_not_mutate_session() {
    let mut engine = Engine::default();
    assert!(engine.session().is_err());
    engine
        .execute(Command::Create {
            kind: SoundKind::Impact,
            seed: 1,
            population: 6,
        })
        .unwrap();
    let before = engine.session().unwrap().clone();
    for cmd in [
        r#"{"op":"select","index":99}"#,
        r#"{"op":"randomize","strength":2,"seed":1}"#,
        r#"{"op":"evolve","ratings":[1],"strength":0.5,"seed":1}"#,
        r#"{"op":"create","kind":"unknown","population":6,"seed":1}"#,
        r#"{"op":"snapshot","typo":1}"#,
        "{}",
        "nope",
    ] {
        let reply: serde_json::Value = serde_json::from_str(&engine.invoke(cmd)).unwrap();
        assert_eq!(reply["ok"], false, "{cmd}");
        assert_eq!(engine.session().unwrap(), &before);
    }
    let mut recipe = Recipe::new(SoundKind::Impact, 1);
    recipe.genome.tone.freq_hz = f32::NAN;
    assert!(render(&recipe, 48_000).is_err());
    recipe.genome.tone.freq_hz = 100.;
    recipe.genome.room = f32::INFINITY;
    assert!(render(&recipe, 48_000).is_err());
    assert!(render(&Recipe::new(SoundKind::Impact, 1), 0).is_err());
    assert!(render(&Recipe::new(SoundKind::Impact, 1), 192_000).is_err());
    let mut json = serde_json::to_value(&before).unwrap();
    json["candidates"] = serde_json::json!([]);
    assert!(Session::from_json(&json.to_string()).is_err());
    json = serde_json::to_value(&before).unwrap();
    json["version"] = serde_json::json!(99);
    assert!(Session::from_json(&json.to_string()).is_err());
}

#[test]
fn many_generations_stay_valid_and_render_at_supported_rates() {
    for entry in catalog() {
        let mut s = Session::new(entry.kind, 77, 4).unwrap();
        for seed in 0..40 {
            s.select(1).unwrap();
            s.randomize(1., seed).unwrap();
            s.validate().unwrap();
        }
        for sr in [22_050, 96_000] {
            let audio = s.render(1, sr).unwrap();
            assert!(
                audio
                    .samples()
                    .iter()
                    .all(|v| v.is_finite() && v.abs() <= 0.890_001)
            );
        }
    }
}

#[cfg(feature = "bevy")]
#[test]
fn plugin_works_in_a_bare_bevy_app() {
    let mut app = bevy_app::App::new();
    app.add_plugins(chirrp::ChirrpPlugin);
    app.update();
    let mut engine = app.world_mut().resource_mut::<Engine>();
    let reply: serde_json::Value = serde_json::from_str(
        &engine.invoke(r#"{"op":"create","kind":"laser","seed":42,"population":6}"#),
    )
    .unwrap();
    assert_eq!(reply["ok"], true);
    assert!(
        engine
            .session()
            .unwrap()
            .render(0, 24_000)
            .unwrap()
            .frames()
            > 0
    );
}
