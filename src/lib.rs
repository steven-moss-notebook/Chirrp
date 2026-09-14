#![doc = include_str!("../README.md")]
mod assets;
mod cinematic;
mod design;
mod evolution;
mod laser;
mod natural;
mod recipe;
mod render;
mod spatial;
mod tools;
#[cfg(target_arch = "wasm32")]
mod wasm;

pub use assets::{
    LoopAssetRequest, MixAssetRequest, RenderedAsset, SoundAssetRequest, asset_tool_definitions,
    execute_asset_tool, generate_loop_asset, render_mix_asset, render_sound_asset,
};

pub use evolution::{Command, Engine, Session};
pub use recipe::{CatalogEntry, Genome, Recipe, SoundKind, catalog};
pub use render::{
    AudioBuffer, AudioMetrics, MixLayer, RenderOptions, mix, render, render_loop,
    render_loop_with_options, render_mix, render_mix_layers, render_mix_layers_with_options,
    render_with_options,
};
pub use symbios_audio;
pub use tools::{CandidateSummary, SessionSummary, SoundEdits, ToolDefinition, tool_definitions};

/// A recoverable validation, serialization, or synthesis error.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Error(pub String);
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}
impl std::error::Error for Error {}
impl From<serde_json::Error> for Error {
    fn from(value: serde_json::Error) -> Self {
        Self(value.to_string())
    }
}
/// The result of validating, editing, or rendering a sound.
pub type Result<T> = std::result::Result<T, Error>;

/// Registers [`Engine`] as a resource. No renderer, window, task pool, or audio backend.
#[cfg(feature = "bevy")]
pub struct ChirrpPlugin;
#[cfg(feature = "bevy")]
impl bevy_app::Plugin for ChirrpPlugin {
    fn build(&self, app: &mut bevy_app::App) {
        app.init_resource::<Engine>();
    }
}

pub(crate) fn range(name: &str, value: f32, min: f32, max: f32) -> Result<()> {
    if !value.is_finite() || !(min..=max).contains(&value) {
        return Err(Error(format!(
            "{name} must be finite and in [{min}, {max}]"
        )));
    }
    Ok(())
}
