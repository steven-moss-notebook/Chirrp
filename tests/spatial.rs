use chirrp::{Recipe, RenderOptions, SoundKind, render, render_with_options};
fn presets() -> Vec<Recipe> {
    serde_json::from_str(include_str!("fixtures/space-bank.json")).unwrap()
}
fn side_fraction(samples: &[f32], sr: f32) -> f64 {
    let c = 1. - (-std::f32::consts::TAU * 400. / sr).exp();
    let (mut lm, mut ls, mut me, mut se) = (0., 0., 0f64, 0f64);
    for p in samples.chunks_exact(2) {
        let m = (p[0] + p[1]) * 0.5;
        let s = (p[0] - p[1]) * 0.5;
        lm += c * (m - lm);
        ls += c * (s - ls);
        me += ((m - lm) as f64).powi(2);
        se += ((s - ls) as f64).powi(2);
    }
    se / (me + se).max(1e-20)
}
#[test]
fn all_25_space_presets_retain_their_latest_design_and_dry_export() {
    let recipes = presets();
    assert_eq!(recipes.len(), 25);
    for expected in recipes {
        let mut current = Recipe::new(expected.kind, expected.seed);
        assert_eq!(current, expected);
        current.genome.width = 0.;
        current.genome.room = 0.;
        let options = RenderOptions { dry_mid: true };
        assert_eq!(
            render_with_options(&expected, 24000, options)
                .unwrap()
                .samples(),
            render_with_options(&current, 24000, options)
                .unwrap()
                .samples(),
            "{:?}",
            expected.kind
        );
    }
}

#[test]
fn space_recipes_load_automatically_into_the_current_design() {
    let space: Vec<_> = chirrp::catalog()
        .into_iter()
        .filter(|e| e.kind.is_space())
        .collect();
    assert_eq!(space.len(), 25);
    for entry in space {
        let mut current = Recipe::new(entry.kind, 108);
        current.genome.tone.freq_hz *= 1.08;
        current.genome.width = 0.7;
        for version in [1, 6, 7] {
            let mut incoming = serde_json::to_value(&current).unwrap();
            incoming["version"] = serde_json::json!(version);
            let json = incoming.to_string();
            assert_eq!(Recipe::from_json(&json).unwrap(), current);
            assert_eq!(serde_json::from_str::<Recipe>(&json).unwrap(), current);
            let request: chirrp::SoundAssetRequest =
                serde_json::from_value(serde_json::json!({"recipe":incoming})).unwrap();
            assert_eq!(request.recipe, current);
        }
    }
    let current = Recipe::new(SoundKind::BeamLoop, 42);
    let expected = chirrp::render_loop(&current, 24000, 0.5).unwrap();
    for version in [1, 6, 7] {
        let mut incoming = current.clone();
        incoming.version = version;
        assert_eq!(
            chirrp::render_loop(&incoming, 24000, 0.5)
                .unwrap()
                .samples(),
            expected.samples()
        );
        assert_eq!(
            render(&incoming, 24000).unwrap().samples(),
            render(&current, 24000).unwrap().samples()
        );
        assert_eq!(
            serde_json::to_value(&incoming).unwrap()["version"],
            current.version
        );
        let mut request = serde_json::json!({"recipe":incoming,"sample_rate":24000,"loop_s":0.5});
        request["recipe"]["version"] = serde_json::json!(version);
        let asset = chirrp::execute_asset_tool("render_sound", request).unwrap();
        assert_eq!(asset.audio.samples(), expected.samples());
        assert_eq!(
            asset.sidecar["render_sound"]["recipe"]["version"],
            current.version
        );
    }
    let mut removed = serde_json::to_value(Recipe::new(SoundKind::Thruster, 42)).unwrap();
    removed["kind"] = serde_json::json!("tissue_wet");
    assert!(Recipe::from_json(&removed.to_string()).is_err());
    assert!(serde_json::from_str::<SoundKind>("\"tissue_wet\"").is_err());
    assert!(
        chirrp::execute_asset_tool(
            "generate_loop",
            serde_json::json!({"kind":"tissue_wet","loop_s":1})
        )
        .is_err()
    );
}

#[test]
fn room_and_width_increase_reflected_energy_and_upper_band_stereo_spread() {
    for kind in [
        SoundKind::PlasmaPulse,
        SoundKind::HeavySlug,
        SoundKind::RocketLaunch,
        SoundKind::Ricochet,
        SoundKind::Thruster,
        SoundKind::ChoirInterval,
        SoundKind::AnvilPulse,
    ] {
        let current = Recipe::new(kind, 42);
        let mut restrained = current.clone();
        restrained.genome.room = 0.15;
        restrained.genome.width = 0.4;
        let a = render(&restrained, 24000).unwrap();
        let b = render(&current, 24000).unwrap();
        assert!(
            side_fraction(b.samples(), 24000.) > side_fraction(a.samples(), 24000.) * 1.5,
            "{kind:?}"
        );
        let source = render_with_options(&current, 24000, RenderOptions { dry_mid: true }).unwrap();
        let tail_energy = |audio: &chirrp::AudioBuffer| {
            audio.samples()[source.samples().len()..]
                .iter()
                .map(|s| (*s as f64).powi(2))
                .sum::<f64>()
        };
        assert!(
            tail_energy(&b) > tail_energy(&a) * 2.,
            "{kind:?}: reflected decay must have more presence"
        );
    }
}
#[test]
fn theater_respects_mono_room_zero_and_extreme_controls() {
    for sr in [22050, 48000, 96000] {
        let mut recipe = Recipe::new(SoundKind::HeavySlug, 42);
        recipe.genome.width = 0.;
        let mono = render(&recipe, sr).unwrap();
        assert!(mono.samples().chunks_exact(2).all(|p| p[0] == p[1]));
        recipe.genome.room = 0.;
        let dry_room = render(&recipe, sr).unwrap();
        assert!(dry_room.frames() < mono.frames());
        recipe.genome.room = 1.;
        recipe.genome.width = 1.5;
        let wide = render(&recipe, sr).unwrap();
        assert!(
            wide.samples()
                .iter()
                .all(|x| x.is_finite() && x.abs() <= 0.890_001)
        );
        assert_eq!(&wide.samples()[wide.samples().len() - 2..], &[0., 0.]);
    }
}
