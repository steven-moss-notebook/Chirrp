//! Stateless, additive asset requests shared by Rust, WASM, and MCP.
//! Existing session tools and recipe/mix APIs remain independent of this layer.
use crate::{
    AudioBuffer, Error, MixLayer, Recipe, RenderOptions, Result, SoundEdits, SoundKind,
    ToolDefinition, catalog, render_loop_with_options, render_mix_layers_with_options,
    render_with_options,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

fn rate() -> u32 {
    48_000
}
fn seed() -> u32 {
    42
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SoundAssetRequest {
    pub recipe: Recipe,
    #[serde(default = "rate")]
    pub sample_rate: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub loop_s: Option<f32>,
    #[serde(default)]
    pub dry_mid: bool,
    #[serde(default)]
    pub mono: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LoopAssetRequest {
    pub kind: SoundKind,
    #[serde(default = "seed")]
    pub seed: u32,
    pub loop_s: f32,
    #[serde(default = "rate")]
    pub sample_rate: u32,
    #[serde(default)]
    pub dry_mid: bool,
    #[serde(default)]
    pub mono: bool,
    /// Optional bed sustain; rejected for one-shot categories.
    #[serde(default)]
    pub sustain_level: Option<f32>,
    #[serde(default)]
    pub edits: Option<SoundEdits>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MixAssetRequest {
    pub layers: Vec<MixLayer>,
    #[serde(default = "rate")]
    pub sample_rate: u32,
    #[serde(default)]
    pub dry_mid: bool,
    #[serde(default)]
    pub mono: bool,
}

/// Audio stays stereo f32 internally; mono is applied only at WAV delivery.
/// The sidecar stores every setting needed to reproduce the exported bytes.
pub struct RenderedAsset {
    pub audio: AudioBuffer,
    pub mono: bool,
    pub sidecar: Value,
}
impl RenderedAsset {
    pub fn wav_bytes(&self) -> Vec<u8> {
        if self.mono {
            self.audio.wav_bytes_mono()
        } else {
            self.audio.wav_bytes()
        }
    }
    pub fn channels(&self) -> u16 {
        if self.mono { 1 } else { 2 }
    }
}

pub fn render_sound_asset(request: &SoundAssetRequest) -> Result<RenderedAsset> {
    let options = RenderOptions {
        dry_mid: request.dry_mid,
    };
    let audio = if let Some(seconds) = request.loop_s {
        render_loop_with_options(&request.recipe, request.sample_rate, seconds, options)?
    } else {
        render_with_options(&request.recipe, request.sample_rate, options)?
    };
    Ok(RenderedAsset {
        audio,
        mono: request.mono,
        sidecar: json!({
            "sample_rate":request.sample_rate,"recipes":[request.recipe],
            "render_sound":request
        }),
    })
}
pub fn generate_loop_asset(request: &LoopAssetRequest) -> Result<RenderedAsset> {
    let mut recipe = Recipe::new(request.kind, request.seed);
    if let Some(edits) = &request.edits {
        edits.apply(&mut recipe)?;
    }
    if let Some(sustain) = request.sustain_level {
        recipe.genome.envelope.sustain_level = sustain;
    }
    render_sound_asset(&SoundAssetRequest {
        recipe,
        sample_rate: request.sample_rate,
        loop_s: Some(request.loop_s),
        dry_mid: request.dry_mid,
        mono: request.mono,
    })
}
pub fn render_mix_asset(request: &MixAssetRequest) -> Result<RenderedAsset> {
    let audio = render_mix_layers_with_options(
        &request.layers,
        request.sample_rate,
        RenderOptions {
            dry_mid: request.dry_mid,
        },
    )?;
    Ok(RenderedAsset {
        audio,
        mono: request.mono,
        sidecar: json!({
            "sample_rate":request.sample_rate,
            "recipes":request.layers.iter().map(|l| &l.recipe).collect::<Vec<_>>(),
            "mix_layers":request
        }),
    })
}

/// Transport adapter for the three additive asset tools. No session is mutated.
pub fn execute_asset_tool(name: &str, args: Value) -> Result<RenderedAsset> {
    match name {
        "render_sound" => render_sound_asset(&serde_json::from_value(args)?),
        "generate_loop" => generate_loop_asset(&serde_json::from_value(args)?),
        "mix_layers" => render_mix_asset(&serde_json::from_value(args)?),
        _ => Err(Error("unknown asset tool".into())),
    }
}

/// Separate definitions allow hosts to opt into new capabilities independently.
pub fn asset_tool_definitions() -> Vec<ToolDefinition> {
    let rate = json!({"type":"integer","minimum":22050,"maximum":96000,"default":48000});
    let boolean = json!({"type":"boolean","default":false});
    let recipe = json!({"type":"object","description":"Complete Recipe from get_recipe. Space kinds always use their current design; no version selection is needed. Bed kinds accept decay_s up to 16 and sustain_level 0–1; other kinds retain their bounds. Validated by the library."});
    let seconds = json!({"type":"number","minimum":0.1,"maximum":16});
    let schema = |properties: Value, required: &[&str]| json!({"type":"object","additionalProperties":false,"properties":properties,"required":required});
    vec![
        ToolDefinition {
            name: "render_sound",
            description: "Export a saved recipe as WAV. Optional loop_s bakes an exact-length seamless loop with WAV loop metadata. dry_mid bypasses room and width; mono exports one channel for game spatializers. Does not change the editing session.",
            input_schema: schema(
                json!({
                    "recipe":recipe,"sample_rate":rate,"loop_s":seconds,"dry_mid":boolean,"mono":boolean
                }),
                &["recipe"],
            ),
        },
        ToolDefinition {
            name: "generate_loop",
            description: "Generate a 0.1–16 second seamless loop from a preset. Continuous bed kinds maintain sustain; other kinds repeat at their natural duration. WAV includes infinite forward loop metadata. dry_mid plus mono suits world emitters. Does not change the editing session.",
            input_schema: schema(
                json!({
                    "kind":{"type":"string","enum":catalog().iter().map(|c| c.kind).collect::<Vec<_>>()},
                    "seed":{"type":"integer","minimum":0,"maximum":4294967295u64,"default":42},
                    "loop_s":seconds,"sample_rate":rate,"dry_mid":boolean,"mono":boolean,
                    "sustain_level":{"type":"number","minimum":0,"maximum":1,"description":"Only continuous bed kinds support nonzero sustain."},
                    "edits":crate::tools::edits_schema()
                }),
                &["kind", "loop_s"],
            ),
        },
        ToolDefinition {
            name: "mix_layers",
            description: "Export 1–32 timed recipe layers with linear gain (0–8) and delay_s (0–16). Preserves all tails and applies shared 0.89 peak protection. Saves gains, delays, recipes and export options. Does not change the editing session.",
            input_schema: schema(
                json!({
                    "layers":{"type":"array","minItems":1,"maxItems":32,"items":schema(json!({
                        "recipe":recipe,"gain":{"type":"number","minimum":0,"maximum":8,"default":1},
                        "delay_s":{"type":"number","minimum":0,"maximum":16,"default":0}
                    }), &["recipe"])},
                    "sample_rate":rate,"dry_mid":boolean,"mono":boolean
                }),
                &["layers"],
            ),
        },
    ]
}
