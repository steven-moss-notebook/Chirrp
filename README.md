# Chirrp

Deterministic procedural sound effects for Rust, with stereo PCM/WAV rendering, seeded variation, and optional Bevy integration. No audio files, soundfonts, window, GPU, server, or audio device are needed.

Chirrp provides 39 synthesized presets: UI click, hover, confirm, error, footstep, explosion, laser, impact, pickup, jump, whoosh, power up, drop, tap, shake, wiggle, rattle, calculator, squish, squeeze, turn, tear, twist, tighten, poke, grab, ring, droplets, rain, wind, leaves, waves, rustling, thunder, dodge, slide, swing, stomp, and bird chirps. Use `catalog()` to discover their names and descriptions.

## Install

```toml
[dependencies]
chirrp = "0.1.0"
```

The default feature set is empty. Enable `bevy` to use `ChirrpPlugin` and register `Engine` as a Bevy 0.19 resource:

```toml
[dependencies]
chirrp = { version = "0.1.0", features = ["bevy"] }
bevy_app = { version = "0.19", default-features = false, features = ["std"] }
```

## Render a sound

```rust
use chirrp::{Recipe, SoundKind, render};

let recipe = Recipe::new(SoundKind::Explosion, 42);
let audio = render(&recipe, 48_000)?;
let pcm = audio.samples(); // Interleaved f32: left, right, left, right…
assert_eq!(audio.channels(), 2);
std::fs::write("explosion.wav", audio.wav_bytes())?;
# Ok::<(), Box<dyn std::error::Error>>(())
```

Rendering is synchronous and offline. Bake sounds before playback or schedule rendering on your application's job system; it allocates and should not run in a real-time audio callback. The crate returns audio data; your application handles playback and storage.

Sample rates from 22,050 through 96,000 Hz are supported. Identical recipes and rates reproduce identical PCM on the same target; floating-point differences may occur between architectures and WASM. These are bounded one-shot sounds, not seamless ambience loops.

## Edit and evolve sounds

```rust
use chirrp::{Engine, SoundEdits, SoundKind};

let mut engine = Engine::default();
engine.create_sound(SoundKind::Laser, 42, 6)?;
engine.edit_sound(SoundEdits {
    pitch_hz: Some(440.0),
    stereo_width: Some(1.2),
    room: Some(0.3),
    ..Default::default()
})?;
engine.randomize(0.45, 43)?;
engine.select_candidate(2)?;
let audio = engine.render_audio(2, 48_000)?;

// Save and restore a complete session, including the selected favorite.
let saved = engine.snapshot()?.to_json()?;
let session = chirrp::Session::from_json(&saved)?;
engine.restore_session(session)?;
# Ok::<(), chirrp::Error>(())
```

Populations contain 2–12 candidates with zero-based indices. `randomize(strength, seed)` preserves the favorite at index 0 and generates variations; zero strength produces exact copies. `evolve(ratings, strength, seed)` accepts one rating in `[0, 1]` per candidate and preserves the highest-rated candidate. Strength is a per-field mutation probability in `[0, 1]`. Explicit seeds make evolution repeatable.

`SoundEdits` exposes pitch, attack, decay, brightness, noise, stereo width, room, drive, and texture. Invalid edits return an error without changing the session. Use `get_recipe` / `replace_recipe` for full recipe access, `analyze` for signal measurements, and `export_wav` for PCM16 WAV bytes. Recipes and sessions support validated JSON round trips and retain their sound design version when restored.

## Bevy integration

With the `bevy` feature enabled:

```rust
# #[cfg(feature = "bevy")]
# fn main() -> Result<(), Box<dyn std::error::Error>> {
use bevy_app::App;
use chirrp::{ChirrpPlugin, Engine, SoundKind};

let mut app = App::new();
app.add_plugins(ChirrpPlugin);
let mut engine = app.world_mut().resource_mut::<Engine>();
engine.create_sound(SoundKind::Laser, 42, 6)?;
let audio = engine.render_audio(0, 48_000)?;
# Ok(())
# }
# #[cfg(not(feature = "bevy"))]
# fn main() {}
```

The plugin only registers the resource. Generation and rendering remain explicit calls; it installs no audio backend or playback systems.

## Agent integration

`tool_definitions()` returns 12 provider-neutral tool definitions, each with its own JSON Schema. Register each definition with its corresponding `Engine` method: `list_sounds`, `random_sound`, `create_sound`, `list_candidates`, `select_candidate`, `randomize`, `evolve`, `edit_sound`, `get_recipe`, `analyze`, `render_audio`, and `export_wav`.

State-changing methods return compact summaries. Fetch full recipes only when needed, and deliver audio as binary artifacts instead of JSON sample lists. Use a separate engine per session and serialize mutations. `Command`, `Engine::execute`, and `Engine::invoke` remain available as compatibility adapters.

```sh
cargo run --release --example agent
cargo run --release --example agent -- --tools
```

## WASM library

The crate also builds as a `cdylib` for `wasm32-unknown-unknown`. The JavaScript bindings expose a stateful `Chirrp` class and stateless `render_recipe` function. Build from the source directory with:

```sh
rustup target add wasm32-unknown-unknown
cargo install wasm-bindgen-cli --version 0.2.127 --locked
bash scripts/build-wasm.sh
```

Generated JavaScript, TypeScript declarations, and WASM go in `wasm/pkg/`. `WASM_BINDGEN=/path/to/wasm-bindgen bash scripts/build-wasm.sh` selects a matching binding executable.

```js
import init, { Chirrp } from './wasm/pkg/chirrp.js';
import { createChirrpTools } from './wasm/tools.js';

await init();
const engine = new Chirrp();
const tools = createChirrpTools(engine);
tools.create_sound.execute({ kind: 'laser', seed: 42 });
const audio = tools.render_audio.execute({ index: 0 }); // data: Float32Array
const wav = tools.export_wav.execute({ index: 0 });     // data: Uint8Array
engine.free();
```

Structured results from the direct WASM methods are JSON strings; the JavaScript tool adapter parses them and validates named arguments. Direct audio methods return typed arrays. An optional `publishAudio` callback passed to `createChirrpTools(engine, { publishAudio })` can store audio and return your host's artifact reference. The library itself performs no network upload. In browser applications, run offline rendering in your own Web Worker to avoid blocking the UI.

## Examples and development

```sh
cargo run --release --example render -- explosion /tmp/explosion.wav 42
cargo run --release --example audition -- /tmp/chirrp-audition
cargo run --release --features bevy --example headless

cargo fmt --check
cargo test --release --locked
cargo test --release --locked --all-features
cargo clippy --all-targets --all-features --locked -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --no-deps --all-features --locked
bash scripts/build-wasm.sh
node scripts/test-wasm.mjs
node scripts/test-tools.mjs
```

Tests cover all presets, deterministic PCM, stereo/mono behavior, WAV encoding, mutation and selection, persistence, validation, legacy sound designs, and headless Bevy integration. The JavaScript tests execute the actual WASM binary. See [sound design notes](docs/sound-design.md) for synthesis details and parameter bounds. Chirrp uses `symbios-audio` 0.2.1 and `symbios-genetics`; it does not dispatch Rayon work, although its dependencies bring Rayon into the dependency tree.

## License

Licensed under either the [MIT license](LICENSE-MIT) or the [Apache License, Version 2.0](LICENSE-APACHE), at your option.
