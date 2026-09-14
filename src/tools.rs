//! Discoverable, independent tool definitions. Register each definition with its
//! corresponding Engine method; no command discriminator is part of its inputs.
use crate::{Error, Recipe, Result, Session, SoundKind, catalog};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

/// Compact state returned after editing/evolution; fetch DNA with get_recipe.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SessionSummary {
    pub kind: SoundKind,
    pub generation: u32,
    pub selected: usize,
    pub candidates: Vec<CandidateSummary>,
}
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CandidateSummary {
    pub index: usize,
    pub seed: u32,
    pub selected: bool,
}
impl From<&Session> for SessionSummary {
    fn from(session: &Session) -> Self {
        Self {
            kind: session.favorite().kind,
            generation: session.generation(),
            selected: session.selected(),
            candidates: session
                .candidates()
                .iter()
                .enumerate()
                .map(|(index, recipe)| CandidateSummary {
                    index,
                    seed: recipe.seed,
                    selected: index == session.selected(),
                })
                .collect(),
        }
    }
}

/// Flat, optional edits to the favorite. Omitted or null fields stay unchanged.
/// At least one value is required. Validation is atomic: a failed edit applies nothing.
#[derive(Debug, Default, Clone, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct SoundEdits {
    pub pitch_hz: Option<f32>,
    pub attack_seconds: Option<f32>,
    pub decay_seconds: Option<f32>,
    pub brightness_hz: Option<f32>,
    pub noise_level: Option<f32>,
    pub stereo_width: Option<f32>,
    pub room: Option<f32>,
    pub drive: Option<f32>,
    pub texture: Option<f32>,
}
impl SoundEdits {
    pub(crate) fn apply(&self, recipe: &mut Recipe) -> Result<()> {
        let g = &mut recipe.genome;
        let mut changed = false;
        for (source, target) in [
            (self.pitch_hz, &mut g.tone.freq_hz),
            (self.attack_seconds, &mut g.envelope.attack_s),
            (self.decay_seconds, &mut g.envelope.decay_s),
            (self.brightness_hz, &mut g.filter.cutoff_hz),
            (self.noise_level, &mut g.noise.gain),
            (self.stereo_width, &mut g.width),
            (self.room, &mut g.room),
            (self.drive, &mut g.drive),
            (self.texture, &mut g.texture),
        ] {
            if let Some(value) = source {
                *target = value;
                changed = true;
            }
        }
        if !changed {
            return Err(Error("supply at least one sound edit".into()));
        }
        recipe.validate()
    }
}

/// Provider-neutral tool definition with JSON Schema arguments.
#[derive(Debug, Serialize)]
pub struct ToolDefinition {
    pub name: &'static str,
    pub description: &'static str,
    pub input_schema: Value,
}
fn object(properties: Value, required: &[&str]) -> Value {
    json!({"type":"object","properties":properties,"required":required,"additionalProperties":false})
}
fn number(description: &str, min: f64, max: f64) -> Value {
    json!({"type":"number","description":description,"minimum":min,"maximum":max})
}
fn index() -> Value {
    json!({"type":"integer","minimum":0,"maximum":11,"description":"Zero-based candidate index from list_candidates. Must exist in the current population."})
}
fn seed() -> Value {
    json!({"type":"integer","minimum":0,"maximum":4294967295_u64,"default":42,"description":"Reproducible random seed. Use a different seed to explore a new variation."})
}
fn strength() -> Value {
    let mut value = number(
        "Per-field mutation probability. 0 copies the favorite during randomize; 1 explores most strongly.",
        0.,
        1.,
    );
    value["default"] = json!(0.45);
    value
}
fn rate() -> Value {
    json!({"type":"integer","minimum":22050,"maximum":96000,"default":48000,"description":"Output sample rate in Hz. 48000 is recommended."})
}

pub(crate) fn edits_schema() -> Value {
    let mut edits = object(
        json!({
            "pitch_hz":number("Base pitch in Hz. Lower for weight; higher for bright UI sounds.",35.,4000.),
            "attack_seconds":number("Time to reach full volume in seconds.",0.001,0.5),
            "decay_seconds":number("Time for the main sound to fade in seconds; reverb adds a tail.",0.025,2.),
            "brightness_hz":number("Low-pass cutoff in Hz. Lower values darken the sound.",150.,16000.),
            "noise_level":number("Noise layer gain. Increase for blasts, air, and surface crunch.",0.,1.5),
            "stereo_width":number("Stereo width. 0 is mono, 0.9 is the preset width, 1.5 is widest.",0.,1.5),
            "room":number("Reverb amount and tail length. 0 is dry; 1 is spacious.",0.,1.),
            "drive":number("Saturation drive. 1 is mild; 4 is aggressive.",1.,4.),
            "texture":number("Amount of bright harmonics and detuned edge.",0.,1.),
        }),
        &[],
    );
    edits["minProperties"] = json!(1);
    edits
}

/// Definitions for session tools plus additive stateless asset tools.
/// JS handlers are in wasm/tools.js; binary results are typed arrays.
pub fn tool_definitions() -> Vec<ToolDefinition> {
    let edits = edits_schema();
    let entries = [
        (
            "random_sound",
            "Choose a random category and generate a fresh variation, replacing the active session. Use randomize instead to keep exploring the current favorite.",
            object(
                json!({"seed":seed(),"population":{"type":"integer","minimum":2,"maximum":12,"default":6,"description":"Total candidates in the fresh session."}}),
                &[],
            ),
        ),
        (
            "list_sounds",
            "List available sound categories and descriptions. Does not require a session.",
            object(json!({}), &[]),
        ),
        (
            "create_sound",
            "Start a new sound session, replacing the current one. Returns candidate indices; candidate 0 is the initial favorite.",
            object(
                json!({
                    "kind":{"type":"string","enum":catalog().iter().map(|c|c.kind).collect::<Vec<_>>(),"description":"Type of UI or game action to synthesize."},
                    "seed":seed(),"population":{"type":"integer","minimum":2,"maximum":12,"default":6,"description":"Total candidates, including the favorite."}
                }),
                &["kind"],
            ),
        ),
        (
            "list_candidates",
            "Get compact current state: category, generation, favorite index, and candidate indices/seeds. Create a sound first.",
            object(json!({}), &[]),
        ),
        (
            "select_candidate",
            "Keep a candidate as the favorite used by randomize and edit_sound. Does not change its audio.",
            object(json!({"index":index()}), &["index"]),
        ),
        (
            "randomize",
            "Generate variations around the favorite. Keeps that favorite unchanged at index 0, then returns the new candidate indices. Create a sound first.",
            object(json!({"strength":strength(),"seed":seed()}), &[]),
        ),
        (
            "evolve",
            "Breed a generation using your ratings. Highest-rated candidate survives unchanged at index 0. Supply one rating for every current candidate; higher is better.",
            object(
                json!({
                    "ratings":{"type":"array","items":number("Preference score; higher is better.",0.,1.),"minItems":2,"maxItems":12},
                    "strength":strength(),"seed":seed()
                }),
                &["ratings"],
            ),
        ),
        (
            "edit_sound",
            "Adjust only the supplied controls of the current favorite. Other controls stay unchanged. Supply at least one control; no recipe JSON is needed.",
            edits,
        ),
        (
            "get_recipe",
            "Get the complete reproducible recipe for one candidate. Use only when DNA details are needed; list_candidates is smaller.",
            object(json!({"index":index()}), &["index"]),
        ),
        (
            "analyze",
            "Measure peak amplitude, RMS, stereo correlation, and duration. These are signal diagnostics, not a perceptual quality score.",
            object(json!({"index":index(),"sample_rate":rate()}), &["index"]),
        ),
        (
            "render_audio",
            "Render a candidate as interleaved stereo Float32Array for host playback. The host should present audio or an artifact reference to the agent.",
            object(json!({"index":index(),"sample_rate":rate()}), &["index"]),
        ),
        (
            "export_wav",
            "Render a candidate as PCM16 stereo WAV bytes (Uint8Array). The host saves or attaches this artifact; the tool takes no filesystem path.",
            object(json!({"index":index(),"sample_rate":rate()}), &["index"]),
        ),
    ];
    entries
        .into_iter()
        .map(|(name, description, input_schema)| ToolDefinition {
            name,
            description,
            input_schema,
        })
        .chain(crate::asset_tool_definitions())
        .collect()
}
