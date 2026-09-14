//! Stereo theater bank plus exact dry-source comparisons. Run with an output directory.
use chirrp::{
    Recipe, RenderOptions, SoundAssetRequest, catalog, render, render_sound_asset,
    render_with_options,
};
use std::{fs, path::PathBuf};
fn hash(samples: &[f32]) -> String {
    let h = samples
        .iter()
        .flat_map(|v| v.to_bits().to_le_bytes())
        .fold(0xcbf29ce484222325u64, |h, b| {
            (h ^ b as u64).wrapping_mul(0x100000001b3)
        });
    format!("{h:016x}")
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let root = PathBuf::from(
        std::env::args()
            .nth(1)
            .unwrap_or_else(|| "audition/spatial".into()),
    );
    fs::create_dir_all(root.join("dry"))?;
    let mut manifest = vec![];
    let kinds = catalog().into_iter().map(|e| e.kind);
    for kind in kinds {
        let recipe = Recipe::new(kind, 42);
        let wet = render(&recipe, 48_000)?;
        let dry = render_with_options(&recipe, 48_000, RenderOptions { dry_mid: true })?;
        if kind.is_space() {
            let name = serde_json::to_value(kind)?.as_str().unwrap().to_string();
            let audition = render_sound_asset(&SoundAssetRequest {
                recipe: recipe.clone(),
                sample_rate: 48_000,
                loop_s: kind.is_bed().then_some(8.),
                dry_mid: false,
                mono: false,
            })?;
            fs::write(root.join(format!("{name}.wav")), audition.wav_bytes())?;
            fs::write(
                root.join("dry").join(format!("{name}.wav")),
                dry.wav_bytes_mono(),
            )?;
            fs::write(
                root.join(format!("{name}.json")),
                serde_json::to_vec_pretty(&audition.sidecar)?,
            )?;
        }
        manifest.push(serde_json::json!({"recipe":recipe,"wet_hash":hash(wet.samples()),"dry_hash":hash(dry.samples()),"wet_metrics":wet.metrics(),"dry_metrics":dry.metrics()}));
    }
    fs::write(
        root.join("bank.json"),
        serde_json::to_vec_pretty(&manifest)?,
    )?;
    Ok(())
}
