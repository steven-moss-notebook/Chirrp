use chirrp::{Engine, SoundEdits, SoundKind, tool_definitions};

#[test]
fn random_sound_is_seeded_varied_and_atomic() {
    let mut engine = Engine::default();
    let mut kinds = std::collections::HashSet::new();
    for seed in 0..1024 {
        let summary = engine.random_sound(seed, 6).unwrap();
        assert_eq!(summary.selected, 1);
        assert_eq!(summary.generation, 0);
        kinds.insert(format!("{:?}", summary.kind));
        let state = engine.snapshot().unwrap();
        engine.random_sound(seed, 6).unwrap();
        assert_eq!(engine.snapshot().unwrap(), state);
        assert!(engine.random_sound(seed, 1).is_err());
        assert_eq!(engine.snapshot().unwrap(), state);
    }
    assert_eq!(kinds.len(), 39);
}

#[test]
fn typed_operations_work_without_commands_and_return_compact_state() {
    let mut engine = Engine::default();
    assert_eq!(engine.list_sounds().len(), 39);
    assert!(engine.list_candidates().is_err());
    let summary = engine.create_sound(SoundKind::Laser, 42, 6).unwrap();
    assert_eq!(summary.candidates.len(), 6);
    assert!(!serde_json::to_string(&summary).unwrap().contains("genome"));
    engine
        .edit_sound(SoundEdits {
            pitch_hz: Some(440.),
            stereo_width: Some(1.2),
            ..Default::default()
        })
        .unwrap();
    let favorite = engine.get_recipe(0).unwrap();
    assert_eq!(favorite.genome.tone.freq_hz, 440.);
    assert_eq!(favorite.genome.width, 1.2);
    engine.randomize(0.7, 43).unwrap();
    assert_eq!(engine.get_recipe(0).unwrap(), favorite);
    engine.select_candidate(2).unwrap();
    let winner = engine.get_recipe(2).unwrap();
    engine.evolve(&[0., 0., 1., 0., 0., 0.], 0.5, 44).unwrap();
    assert_eq!(engine.get_recipe(0).unwrap(), winner);
    let snapshot = engine.snapshot().unwrap();
    engine.create_sound(SoundKind::Explosion, 1, 2).unwrap();
    engine.restore_session(snapshot.clone()).unwrap();
    assert_eq!(engine.snapshot().unwrap(), snapshot);
    let audio = engine.render_audio(0, 24_000).unwrap();
    assert_eq!(engine.export_wav(0, 24_000).unwrap(), audio.wav_bytes());
    assert_eq!(
        engine.analyze(0, 24_000).unwrap().peak,
        audio.metrics().peak
    );
}

#[test]
fn edits_are_partial_atomic_and_strict() {
    let mut engine = Engine::default();
    engine.create_sound(SoundKind::Footstep, 42, 6).unwrap();
    let before = engine.snapshot().unwrap();
    for edits in [
        SoundEdits::default(),
        SoundEdits {
            pitch_hz: Some(600.),
            room: Some(2.),
            ..Default::default()
        },
        SoundEdits {
            pitch_hz: Some(f32::NAN),
            ..Default::default()
        },
    ] {
        assert!(engine.edit_sound(edits).is_err());
        assert_eq!(engine.snapshot().unwrap(), before);
    }
    assert!(serde_json::from_str::<SoundEdits>(r#"{"typo":1}"#).is_err());
    engine
        .edit_sound(SoundEdits {
            room: Some(0.8),
            ..Default::default()
        })
        .unwrap();
    let mut expected = before.favorite().clone();
    expected.genome.room = 0.8;
    assert_eq!(engine.get_recipe(0).unwrap(), expected);
    assert!(engine.select_candidate(99).is_err());
    assert!(engine.create_sound(SoundKind::Laser, 42, 0).is_err());
    assert_eq!(engine.get_recipe(0).unwrap(), expected);
}

#[test]
fn tool_catalog_has_independent_discoverable_arguments() {
    let tools = tool_definitions();
    assert_eq!(tools.len(), 12);
    for tool in &tools {
        assert!(!tool.description.is_empty());
        assert_eq!(tool.input_schema["additionalProperties"], false);
        assert!(tool.input_schema["properties"].get("op").is_none());
    }
    let create = tools.iter().find(|t| t.name == "create_sound").unwrap();
    assert_eq!(
        create.input_schema["properties"]["kind"]["enum"]
            .as_array()
            .unwrap()
            .len(),
        39
    );
    let edit = tools.iter().find(|t| t.name == "edit_sound").unwrap();
    assert!(edit.input_schema["properties"].get("recipe").is_none());
    assert_eq!(
        edit.input_schema["properties"]["stereo_width"]["maximum"],
        1.5
    );
}
