# Chirrp

Deterministic procedural sound effects for Rust, with stereo PCM/WAV rendering, seeded variation, and optional Bevy integration. No audio files, soundfonts, window, GPU, server, or audio device are needed.

Chirrp provides 66 synthesized presets. The original bank includes UI click, hover, confirm, error, footstep, explosion, laser, impact, pickup, jump, whoosh, power up, drop, tap, shake, wiggle, rattle, calculator, squish, squeeze, turn, tear, twist, tighten, poke, grab, ring, droplets, rain, wind, leaves, waves, rustling, thunder, car engine rumble, dodge, slide, swing, stomp, bird chirps, and seagull. The 25 space-bank additions cover energy weapons, ship machinery, ice, and five faction palettes. Use `catalog()` to discover their names and descriptions; `SoundKind::is_bed()` identifies continuous sources.

An optional `chirrp-mcp` executable lets local agents generate and export game sounds through MCP. It is enabled by the `mcp` feature and is **not required to use the Rust library or WASM bindings**. See [local agent setup](#optional-mcp-server-for-local-agents).

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

Sample rates from 22,050 through 96,000 Hz are supported. Identical recipes and rates reproduce identical PCM on the same target; floating-point differences may occur between architectures and WASM. Existing one-shot rendering remains available. The additive loop API bakes seamless clips up to 16 seconds.

## Mix sounds

Use `render_mix` to layer any nonempty list of recipes into one stereo buffer:

```rust
use chirrp::{Recipe, SoundKind, render_mix};

let recipes = [
    Recipe::new(SoundKind::Explosion, 42),
    Recipe::new(SoundKind::Impact, 7),
    Recipe::new(SoundKind::Laser, 19),
];
let audio = render_mix(&recipes, 48_000)?;
let wav = audio.wav_bytes();
# Ok::<(), chirrp::Error>(())
```

For already rendered sounds, use `chirrp::mix(&[&first, &second, &third])` to avoid rendering again. Inputs must have the same sample rate; an empty list or mismatched rates returns an error. There is no fixed limit on the number of sounds beyond available memory.

Mixing uses the `symbios_audio::Mix` node. Sounds start together at frame zero, retain independent left/right channels, and last as long as the longest input, including room tails. Samples are summed at unity gain; if the result exceeds the 0.89 peak ceiling, the entire mix is attenuated equally in both channels. Quiet mixes are not amplified, and mixing one sound returns identical PCM.

## Loops, dry mono exports, and timed layers

Existing `render`, `mix`, `render_mix`, session methods, recipe fields, and their default stereo WAV format retain their contracts. New functions provide the additional export behavior:

```rust
use chirrp::{Recipe, SoundKind, RenderOptions, MixLayer,
    render_loop_with_options, render_mix_layers_with_options};

let world = RenderOptions { dry_mid: true };
let bed = Recipe::new(SoundKind::HullRumble, 42);
let audio = render_loop_with_options(&bed, 48_000, 8.0, world)?;
assert_eq!(audio.frames(), 384_000);
std::fs::write("hull.wav", audio.wav_bytes_mono())?;

let layers = [
    MixLayer::new(Recipe::new(SoundKind::BeamIgnite, 42), 0.65, 0.0),
    MixLayer::new(Recipe::new(SoundKind::HeavySlug, 7), 0.4, 0.18),
];
let hazard = render_mix_layers_with_options(&layers, 48_000, world)?;
std::fs::write("hazard.wav", hazard.wav_bytes_mono())?;
# Ok::<(), Box<dyn std::error::Error>>(())
```

`render_loop(recipe, sr, loop_s)` and `render_mix_layers(layers, sr)` use the normal stereo room. Their `*_with_options` variants, plus `render_with_options`, accept `RenderOptions`. `dry_mid` bypasses baked room and direct side detail. Internal PCM stays interleaved stereo; `wav_bytes_mono()` writes a true one-channel WAV for Bevy spatial emitters. Width zero alone still includes mid-channel room.

Loops last 0.1–16 seconds, rounded to the nearest frame, and use a smooth 50 ms end/start crossfade (at most a quarter of the clip). Bed kinds synthesize continuously after a one-second filter/room preroll. Other kinds repeat their natural one-shot duration, useful for alarms. WAVs include an infinite forward `smpl` loop with an inclusive end frame. Configure runtime looping explicitly when the game decoder does not honor `smpl`; in Bevy use looping playback settings and the game's spatial scale, such as 0.12.

The eight continuous kinds are `BeamLoop`, `GravityDrone`, `HullRumble`, `VacuumLoop`, `MagnetPulse`, `FurnaceBed`, `NaniteHiss`, and `ChoirInterval`. Each bed has its own source engine rather than a shared noise swell. They accept `genome.envelope.decay_s` up to 16 and `sustain_level` from 0 to 1. One-shot bed rendering includes attack, decay, release, and room tail; loop duration is set independently by `loop_s`. Other kinds keep their original envelope bounds. The space bank has one current design per kind, with a wide reflection field and a prominent theater room. Create recipes with `Recipe::new` or the agent tools; space recipes load automatically into the current design, with no version selection needed. The original 41 presets and their saved recipes are unchanged.

Timed mixes accept 1–32 `MixLayer { recipe, gain, delay_s }` values. Gains are linear 0–8; delays are 0–16 seconds, rounded to the nearest frame. The mix retains every delayed tail and applies shared attenuation only above a 0.89 sample peak.

For portable exports, `render_sound_asset`, `generate_loop_asset`, and `render_mix_asset` accept typed requests and return `RenderedAsset` with WAV encoding and a reproducible sidecar. Rust, WASM, and MCP use this shared stateless asset layer. The existing session API stays separate. See [spatial sound design](docs/sound-design.md#space-theater-field) for the room architecture and a reproducible bank export, and [the MCP guide](docs/mcp.md#additive-space-bank-tools) for agent tools.

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

The `bevy` feature registers `Engine` as a resource through `ChirrpPlugin`. Generation belongs in startup/loading systems with `ResMut<Engine>` and `ResMut<SoundBank>`. Playback should only read the cached bank; it does not need the engine or another render.

The [interactive Bevy example](examples/bevy-app/src/main.rs) uses a normal `DefaultPlugins` app with three buttons and actual device playback. Run it from a repository checkout:

```sh
cargo run --manifest-path examples/bevy-app/Cargo.toml --locked
```

Its event flow is:

```text
Startup: generate_bank → Assets<AudioSource> + SoundBank
Button click → On<Pointer<Click>> → trigger PlaySound
On<PlaySound> + Res<SoundBank> → AudioPlayer + PlaybackSettings::DESPAWN
```

Startup renders at 48 kHz, encodes WAV once, and stores strong asset handles in the bank. The playback observer clones a handle and spawns a voice; Bevy handles decoding, device output, and cleanup. It needs `Commands` to spawn the voice but only `Res<SoundBank>` for sound data, with no `ResMut<Engine>` or `ResMut<Assets<AudioSource>>`. In Bevy 0.19, observers handle triggered **Events**; buffered **Messages** use `MessageWriter` / `MessageReader` instead.

The demo is a separate example crate so its UI, renderer, WAV decoder, and audio-device dependencies do not enter the library or MCP dependency tree. It needs a graphical session and an audio output device; Linux may need the platform's ALSA and windowing development packages. Its playback gain leaves some headroom, but production games should manage overlapping voice counts and bus levels. Larger sound banks should render during a loading state or on a worker, then insert completed assets before enabling playback.

The [headless example](examples/headless.rs) demonstrates the same startup bank and read-only observer pattern with shared PCM and a voice entity, without opening a window or audio device:

```sh
cargo run --release --features bevy --example headless
```

`ChirrpPlugin` itself only registers the engine resource; the application owns the sound bank and playback backend.

## Agent integration

Use the optional MCP server below to connect an existing agent, or integrate the library directly with your own agent host.

`tool_definitions()` returns 12 provider-neutral tool definitions, each with its own JSON Schema. Register each definition with its corresponding `Engine` method: `list_sounds`, `random_sound`, `create_sound`, `list_candidates`, `select_candidate`, `randomize`, `evolve`, `edit_sound`, `get_recipe`, `analyze`, `render_audio`, and `export_wav`.

State-changing methods return compact summaries. Fetch full recipes only when needed, and deliver audio as binary artifacts instead of JSON sample lists. Use a separate engine per session and serialize mutations. `Command`, `Engine::execute`, and `Engine::invoke` remain available as compatibility adapters.

```sh
cargo run --release --example agent
cargo run --release --example agent -- --tools
```

### Optional MCP server for local agents

`chirrp-mcp` is an optional executable that exposes 14 tools for generation, editing, evolution, analysis, mixing, and WAV export. It can run as a background local HTTP server or as an agent-managed stdio process. It needs no audio device or API key. The agent host supplies the model and handles playback.

The server uses the official Rust MCP SDK, `rmcp` 3.3.0. The SDK and async runtime are optional native dependencies activated only by `mcp`; default library builds and WASM builds do not compile them. All MCP implementation code lives under `src/bin/`. Cargo features apply to the whole package, so enabling `mcp` also activates those dependencies when building native library targets in the same package; library users should leave it disabled.

Install from a published release containing the MCP server:

```sh
cargo install chirrp --locked --features mcp --bin chirrp-mcp
```

The crates.io command requires a release containing the `mcp` feature. To install the current checkout, including changes that have not been published yet, run this from the repository root:

```sh
cargo install --path . --locked --features mcp --bin chirrp-mcp
chirrp-mcp --help
```

Cargo installs the executable into `$CARGO_HOME/bin` (normally `~/.cargo/bin`; `%USERPROFILE%\.cargo\bin` on Windows). Add that directory to `PATH`, or use the executable's full path in your agent configuration. The `mcp` feature is opt-in; normal `chirrp` library dependencies keep the default feature set empty and do not build the server.

The install configuration is in [`Cargo.toml`](Cargo.toml): `[[bin]]` names `chirrp-mcp` and sets `required-features = ["mcp"]`; `[features].mcp` enables its optional native dependencies. Cargo builds and installs that binary when you use the commands above. No separate install script is needed.

Start the background server and choose where generated assets go:

```sh
chirrp-mcp start --output-folder /absolute/path/to/game/assets/audio
```

The command returns once the server is ready at `http://127.0.0.1:8765/mcp`. In your agent, add a **Streamable HTTP** MCP server using that URL. For clients accepting `mcpServers` URL entries:

```json
{
  "mcpServers": {
    "chirrp": { "url": "http://127.0.0.1:8765/mcp" }
  }
}
```

Stop it when finished:

```sh
chirrp-mcp stop
```

Use `--port 8766` on `start` to change the port. Alternatively, agents that launch their own subprocess can use command `chirrp-mcp` with arguments `["stdio", "--output-folder", "/absolute/path/to/game/assets/audio"]`; the agent then manages that process's lifetime. See [local agent setup](docs/mcp.md) for configuration and state/log locations.

Then ask your agent:

> Use Chirrp to generate a short laser shot, a heavy impact, and a quiet UI hover at 48 kHz with little reverb. Save the WAVs, mix the laser and impact into a fourth sound, and return the asset paths.

`generate_sound` creates an asset in one call. For iterative design, use `create_sound`, `edit_sound` or `randomize`, then `export_wav`. `mix_sounds` layers up to 32 recipe objects into one stereo asset. Exports create fresh directories containing `sound.wav` and reproducible `recipes.json` sidecars without overwriting existing files. Rendering happens locally; the tools return file paths and signal metrics.

HTTP authentication is optional: set `CHIRRP_MCP_AUTH_TOKEN` to a random 32–256 character bearer token before `start`, then configure the same token in your agent. The server remains loopback-only. Requests and tool work are bounded, and discovery/export tools publish output schemas. See [the MCP guide](docs/mcp.md) for authentication setup, limits, recipe reuse, and protocol details.

## WASM library

The crate also builds as a `cdylib` for `wasm32-unknown-unknown`. The JavaScript bindings expose a stateful `Chirrp` class and stateless `render_recipe` and `render_mix` functions. Build from the source directory with:

```sh
rustup target add wasm32-unknown-unknown
cargo install wasm-bindgen-cli --version 0.2.127 --locked
bash scripts/build-wasm.sh
```

Generated JavaScript, TypeScript declarations, and WASM go in `wasm/pkg/`. `WASM_BINDGEN=/path/to/wasm-bindgen bash scripts/build-wasm.sh` selects a matching binding executable.

| File | Purpose |
| --- | --- |
| `wasm/pkg/chirrp_bg.wasm` | Compiled WebAssembly module to deploy |
| `wasm/pkg/chirrp.js` | JavaScript loader and API; deploy beside the `.wasm` file |
| `wasm/pkg/chirrp.d.ts` | TypeScript declarations for the JavaScript API |
| `wasm/pkg/chirrp_bg.wasm.d.ts` | TypeScript declarations for the raw WASM exports |

Re-run the build script after changing Rust code. `wasm/pkg/` is generated and ignored by Git. The intermediate `target/wasm32-unknown-unknown/release/chirrp.wasm` is the input to the binding generator; use the files in `wasm/pkg/` on your website.

### Try it in a browser

After building, run this from the repository root:

```sh
python3 -m http.server 8080 --bind 127.0.0.1 --directory wasm
```

Open [http://localhost:8080](http://localhost:8080), choose a preset, click **Generate sound**, and use the audio player's play button. You can also download the generated WAV. The example uses [a module worker](wasm/worker.js) to render without blocking the page.

To deploy the complete example, copy `wasm/index.html`, `wasm/example.js`, `wasm/worker.js`, and `wasm/pkg/` into the same directory on your website. For your own integration, copy `wasm/pkg/` to a public assets directory and import its `chirrp.js` as an ES module. Keep `chirrp.js` and `chirrp_bg.wasm` together: `await init()` loads the binary relative to the JavaScript file. Serve over HTTP(S), with `.wasm` served as `application/wasm` and `.js` as JavaScript; opening the HTML through `file://` will not work.

### Use the API

The optional `wasm/tools.js` adapter adds named arguments and parsed results. Copy it too if using this example:

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

To mix saved recipes in JavaScript, pass a JSON array to the stateless export:

```js
import { render_mix } from './wasm/pkg/chirrp.js';

// After await init(); each entry is a parsed recipe object.
const pcm = render_mix(JSON.stringify(recipes), 48_000); // Float32Array, stereo
```

## Procedural sound-design skill

The repository includes [the `chirrp-sound-design` skill](.agents/skills/chirrp-sound-design/SKILL.md) for agents designing or improving game sounds. It explains the actual stereo, reverb, saturation, fade, and mastering chain; distinguishes peak protection from peak/loudness normalization; and covers mono compatibility, listening evaluation, and runtime mix headroom. Invoke it as `$chirrp-sound-design` in an agent that discovers repository skills, or point your agent to the file.

## Examples and development

```sh
cargo run --release --example render -- explosion /tmp/explosion.wav 42
cargo run --release --example audition -- /tmp/chirrp-audition
cargo run --release --features bevy --example headless
cargo test --manifest-path examples/bevy-app/Cargo.toml --locked

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
