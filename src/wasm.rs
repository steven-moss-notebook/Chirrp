use crate::{Engine, Recipe, Session, SoundEdits, render};
use wasm_bindgen::prelude::*;

/// Stateful sound engine. Run inside a Web Worker to keep rendering off the UI thread.
#[wasm_bindgen]
#[derive(Default)]
pub struct Chirrp {
    engine: Engine,
}
#[wasm_bindgen]
impl Chirrp {
    #[wasm_bindgen(constructor)]
    pub fn new() -> Self {
        Self::default()
    }
    pub fn list_sounds(&self) -> std::result::Result<String, JsValue> {
        json(self.engine.list_sounds())
    }
    pub fn random_sound(
        &mut self,
        seed: u32,
        population: usize,
    ) -> std::result::Result<String, JsValue> {
        json(
            self.engine
                .random_sound(seed, population)
                .map_err(js_error)?,
        )
    }
    pub fn create_sound(
        &mut self,
        kind: &str,
        seed: u32,
        population: usize,
    ) -> std::result::Result<String, JsValue> {
        let kind = serde_json::from_value(serde_json::Value::String(kind.into()))
            .map_err(|e| js_error(e.into()))?;
        json(
            self.engine
                .create_sound(kind, seed, population)
                .map_err(js_error)?,
        )
    }
    pub fn list_candidates(&self) -> std::result::Result<String, JsValue> {
        json(self.engine.list_candidates().map_err(js_error)?)
    }
    pub fn select_candidate(&mut self, index: usize) -> std::result::Result<String, JsValue> {
        json(self.engine.select_candidate(index).map_err(js_error)?)
    }
    pub fn randomize(&mut self, strength: f32, seed: u32) -> std::result::Result<String, JsValue> {
        json(self.engine.randomize(strength, seed).map_err(js_error)?)
    }
    pub fn evolve(
        &mut self,
        ratings: Vec<f32>,
        strength: f32,
        seed: u32,
    ) -> std::result::Result<String, JsValue> {
        json(
            self.engine
                .evolve(&ratings, strength, seed)
                .map_err(js_error)?,
        )
    }
    /// Flat SoundEdits object serialized as JSON; wasm/tools.js accepts named fields.
    pub fn edit_sound(&mut self, edits_json: &str) -> std::result::Result<String, JsValue> {
        if edits_json.len() > 4096 {
            return Err(JsValue::from_str("edits exceed 4 KiB"));
        }
        let edits: SoundEdits = serde_json::from_str(edits_json).map_err(|e| js_error(e.into()))?;
        json(self.engine.edit_sound(edits).map_err(js_error)?)
    }
    pub fn get_recipe(&self, index: usize) -> std::result::Result<String, JsValue> {
        json(self.engine.get_recipe(index).map_err(js_error)?)
    }
    pub fn replace_recipe(&mut self, recipe_json: &str) -> std::result::Result<String, JsValue> {
        let recipe = Recipe::from_json(recipe_json).map_err(js_error)?;
        json(self.engine.replace_recipe(recipe).map_err(js_error)?)
    }
    pub fn snapshot(&self) -> std::result::Result<String, JsValue> {
        json(self.engine.snapshot().map_err(js_error)?)
    }
    pub fn restore_session(&mut self, session_json: &str) -> std::result::Result<String, JsValue> {
        let session = Session::from_json(session_json).map_err(js_error)?;
        json(self.engine.restore_session(session).map_err(js_error)?)
    }
    pub fn analyze(&self, index: usize, sample_rate: u32) -> std::result::Result<String, JsValue> {
        json(self.engine.analyze(index, sample_rate).map_err(js_error)?)
    }
    pub fn render_audio(
        &self,
        index: usize,
        sample_rate: u32,
    ) -> std::result::Result<Vec<f32>, JsValue> {
        self.engine
            .render_audio(index, sample_rate)
            .map(|a| a.into_samples())
            .map_err(js_error)
    }
    pub fn export_wav(
        &self,
        index: usize,
        sample_rate: u32,
    ) -> std::result::Result<Vec<u8>, JsValue> {
        self.engine.export_wav(index, sample_rate).map_err(js_error)
    }
    /// Stateless asset exports; JSON arguments match the additive tool schemas.
    pub fn render_sound(&self, args_json: &str) -> std::result::Result<Vec<u8>, JsValue> {
        asset_wav("render_sound", args_json)
    }
    pub fn generate_loop(&self, args_json: &str) -> std::result::Result<Vec<u8>, JsValue> {
        asset_wav("generate_loop", args_json)
    }
    pub fn mix_layers(&self, args_json: &str) -> std::result::Result<Vec<u8>, JsValue> {
        asset_wav("mix_layers", args_json)
    }
    /// Compatibility adapter. New integrations should use the named methods.
    pub fn invoke(&mut self, json: &str) -> String {
        self.engine.invoke(json)
    }
    /// Stereo interleaved Float32Array. Frame count is array.length / 2.
    pub fn render(&self, index: usize, sample_rate: u32) -> std::result::Result<Vec<f32>, JsValue> {
        self.engine
            .session()
            .and_then(|s| s.render(index, sample_rate))
            .map(|audio| audio.into_samples())
            .map_err(js_error)
    }
    /// PCM16 WAV as a Uint8Array, ready for a Blob/download.
    pub fn wav(&self, index: usize, sample_rate: u32) -> std::result::Result<Vec<u8>, JsValue> {
        self.engine
            .session()
            .and_then(|s| s.render(index, sample_rate))
            .map(|audio| audio.wav_bytes())
            .map_err(js_error)
    }
}
/// Stateless rendering for agents or UI integrations storing individual recipes.
#[wasm_bindgen]
pub fn render_recipe(json: &str, sample_rate: u32) -> std::result::Result<Vec<f32>, JsValue> {
    Recipe::from_json(json)
        .and_then(|p| render(&p, sample_rate))
        .map(|a| a.into_samples())
        .map_err(js_error)
}
/// Render and mix a nonempty JSON array of recipes. Returns stereo interleaved
/// Float32Array PCM, aligned at frame zero and lasting as long as the longest
/// sound, with a shared peak ceiling of 0.89.
#[wasm_bindgen]
pub fn render_mix(json: &str, sample_rate: u32) -> std::result::Result<Vec<f32>, JsValue> {
    let recipes: Vec<Recipe> = serde_json::from_str(json).map_err(|e| js_error(e.into()))?;
    crate::render_mix(&recipes, sample_rate)
        .map(|audio| audio.into_samples())
        .map_err(js_error)
}
fn js_error(error: crate::Error) -> JsValue {
    JsValue::from_str(&error.to_string())
}
fn json(value: impl serde::Serialize) -> std::result::Result<String, JsValue> {
    serde_json::to_string(&value).map_err(|e| js_error(e.into()))
}
/// Independent JSON Schema definitions for tool registration.
#[wasm_bindgen]
pub fn tool_definitions() -> std::result::Result<String, JsValue> {
    json(crate::tool_definitions())
}

fn asset_wav(name: &str, args_json: &str) -> std::result::Result<Vec<u8>, JsValue> {
    if args_json.len() > 1024 * 1024 {
        return Err(JsValue::from_str("asset request exceeds 1 MiB"));
    }
    let args = serde_json::from_str(args_json).map_err(|e| js_error(e.into()))?;
    crate::execute_asset_tool(name, args)
        .map(|asset| asset.wav_bytes())
        .map_err(js_error)
}

/// Stateless exact-length loop rendering, returning stereo interleaved PCM.
#[wasm_bindgen]
pub fn render_loop(
    json: &str,
    sample_rate: u32,
    loop_s: f32,
) -> std::result::Result<Vec<f32>, JsValue> {
    Recipe::from_json(json)
        .and_then(|r| crate::render_loop(&r, sample_rate, loop_s))
        .map(|a| a.into_samples())
        .map_err(js_error)
}
/// Additive dry-mid rendering. Existing render_recipe retains its behavior.
#[wasm_bindgen]
pub fn render_with_options(
    json: &str,
    sample_rate: u32,
    options_json: &str,
) -> std::result::Result<Vec<f32>, JsValue> {
    let options = serde_json::from_str(options_json).map_err(|e| js_error(e.into()))?;
    Recipe::from_json(json)
        .and_then(|r| crate::render_with_options(&r, sample_rate, options))
        .map(|a| a.into_samples())
        .map_err(js_error)
}
#[wasm_bindgen]
pub fn render_loop_with_options(
    json: &str,
    sample_rate: u32,
    loop_s: f32,
    options_json: &str,
) -> std::result::Result<Vec<f32>, JsValue> {
    let options = serde_json::from_str(options_json).map_err(|e| js_error(e.into()))?;
    Recipe::from_json(json)
        .and_then(|r| crate::render_loop_with_options(&r, sample_rate, loop_s, options))
        .map(|a| a.into_samples())
        .map_err(js_error)
}
#[wasm_bindgen]
pub fn render_mix_layers(json: &str, sample_rate: u32) -> std::result::Result<Vec<f32>, JsValue> {
    render_mix_layers_with_options(json, sample_rate, "{}")
}
#[wasm_bindgen]
pub fn render_mix_layers_with_options(
    json: &str,
    sample_rate: u32,
    options_json: &str,
) -> std::result::Result<Vec<f32>, JsValue> {
    if json.len() > 1024 * 1024 {
        return Err(JsValue::from_str("layers exceed 1 MiB"));
    }
    let layers: Vec<crate::MixLayer> =
        serde_json::from_str(json).map_err(|e| js_error(e.into()))?;
    let options = serde_json::from_str(options_json).map_err(|e| js_error(e.into()))?;
    crate::render_mix_layers_with_options(&layers, sample_rate, options)
        .map(|a| a.into_samples())
        .map_err(js_error)
}
