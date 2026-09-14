# Evolution and sound design

Chirrp uses **`symbios-audio` 0.2.1**, the exact DSP core re-exported by **`bevy_symbios_audio` 0.4.2**, and its `symbios-genetics::Genotype` implementations for oscillator, envelope, filter, and gain mutation/crossover. The original wrapper unconditionally enables Bevy desktop defaults and installs a Rayon bake pool. Using its shared core with `bevy_app` / `bevy_ecs` keeps this library headless and browser-compatible.

The serial interactive evolutionary loop lives in Chirrp: Randomize implements **(1 + lambda) selection around a chosen favorite**; rated evolution uses **size-three tournaments, crossover, mutation, and one unchanged elite**. This uses the same node-level genetic operators as the upstream editor, not its egui widgets or threaded search runners. No Rayon work is dispatched (the upstream dependency still brings Rayon into the dependency tree).

Strength is a per-field mutation probability, not a promise of improvement. A zero-strength Randomize produces exact copies. Higher ratings in `[0,1]` steer `evolve`; provide one rating per candidate. User choices or an agent's evaluator define “better.” Unrated randomization does not learn taste or automatically assess cinematic quality. An agent can audition renders, use `analyze` as signal diagnostics, and supply its own ratings. The chosen favorite / highest-rated elite is preserved exactly, including its noise seed. Save snapshots and explicit future seeds to replay an evolutionary sequence.

The space bank contains 25 arrangements, each with one current design; eight have continuous bed synthesis. See [the theater field](#space-theater-field). Fresh footstep and laser recipes use sound design version 3. The explosion default restores the original version-1 sound; the other original categories keep their version-2 designs. The additional scenes use version 4, except fresh rattle, calculator, wind, leaves, rustling, seagull, and car engine rumble recipes, which use version 5. Saved recipes for the original 41 kinds keep their original processing. Each category has a dedicated voice arrangement rather than the original shared oscillator/noise stack:

- **Hover:** an 86 ms soft touch cue, nearly dry, with a 0.045 maximum sample peak (about −27 dBFS); repeated cues stay quiet.
- **Confirm / error:** a warm ascending major resolution / a subdued descending minor-third phrase.
- **Footstep:** a low-pass-filtered heel and sole contact with a much quieter, shorter surface scuff; no ringing pitched footstep oscillator. Seeds vary contact texture and timing. This is a synthetic soft-soled contact, not a recording of a specific surface.
- **Laser:** follows the supplied `references/shoot11.bfxr.json` sine-wave contour: about 1030 Hz falling toward 400 Hz, a 31 ms hold, and a 331 ms linear decay. The reference high-pass character is preserved with eightfold oversampling and Chirrp stereo processing. `waveType: 2` is sine; square-duty settings do not apply. This is a reference-based sound design, not a general Bfxr importer or bit-identical emulator.
- **Impact:** differently damped bands of the same seeded contact excitation, sharing an onset so the hit holds together.
- **Pickup:** a restrained consonant dyad without the former cartoon-like pitch slide.
- **Explosion:** restored original patch, gain, and reverb exactly, verified against the original exported WAV.
- **Whoosh / power up / jump / click:** retain their revised envelopes and arrangements.

The additional bank gives each gesture a separate arrangement:

- **Drop / tap / poke / grab / stomp:** falling bounces, hard resonances, elastic indentation, grip friction, and heavy floor contact.
- **Shake / wiggle:** alternating loose contacts and bending rubber tones.
- **Rattle / calculator:** rattle retains the loose noise contacts and low body; calculator isolates the electronic bleep sequence formerly mixed into rattle.
- **Squish / squeeze / turn / tear / twist / tighten:** wet bubble collapses, rising strain, mechanical detents, fibrous ripping, torsional creaks, and an accelerating ratchet lock.
- **Ring / droplets / bird chirps / seagull:** inharmonic bell modes, scattered water resonances, answering rising/falling songbird phrases, and a coastal gull cry with a continuous rising/falling fundamental, phase-locked harmonics, changing vocal resonance, and pressure flutter.
- **Wind:** a quiet breeze with gradual swells, without the heavy body or resonant whistle; its sample peak is capped at 0.18.
- **Leaves / rustling:** soft, irregular nearby leaf flutter / broader overlapping tree-canopy swells carrying finer leaf movement. Longer attacks and filtered pink excitation replace short, regularly spaced contact grains.
- **Rain / waves / thunder / car engine rumble:** dense independent rain grains, breaking/receding surf, a crack with successive low thunder rolls, and a V8 idle at roughly 750 RPM: 50 overlapping combustion pulses per second, unequal exhaust-bank contributions, slight crank-speed drift, and quiet mechanical noise.
- **Dodge / slide / swing:** a lateral air cut, sustained friction with a weighted stop, and a broad accelerating air arc.

Each new scene uses independently seeded excitation and stereo trajectories. The deep body stays centered; direct spatial detail passes through the same bass-protecting side filters as the room. Width zero remains mono, and downmixing preserves the body. These are bounded one-shot environmental scenes, not seamless ambience loops. Old recipe rendering remains unchanged.

Short-envelope mutations are constrained proportionally around the parent, with extra duration/pitch/room bounds for hover offspring. The unchanged favorite remains available in slot 0. Direct edits are still validated against the public control bounds; some material-specific controls are intentionally limited inside the voice arrangement (for example, the laser's pitch bend).

Modern mastering uses role-specific fixed gain with **attenuation-only peak protection**. The restored version-1 explosion retains its original mastering. It never boosts a quiet hover up to the level of a game impact. Category ceilings range from 0.045 for hover to **0.89 (~−1 dBFS)** for explosion. Saturation is gain-compensated. Stereo reflections feed a damped four-delay room with two all-pass diffusers, and tails are shorter. A two-pole 250 Hz high-pass keeps the side signal out of the deep bass. Width zero gives identical channels; mono downmix preserves the mid signal up to linked mastering gain. Start/end fades suppress boundary clicks. These are sample-peak ceilings, not true-peak or loudness normalization.

Existing version-1 recipes retain their original renderer and mastering. Use `Recipe::new` for the current preset design, or deserialize an existing recipe to preserve its version.

To export the full revised bank for listening:

```sh
cargo run --release --example audition -- /tmp/chirrp-audition
```

Acoustic tests verify the hover's short duration/low level across 100 rapid repetitions, bounds after evolution, the direction of the confirmation/error phrases, the reference laser pitch contour, footstep follow-through, and unchanged legacy PCM. These checks support listening; they cannot establish a listener's perception of polish or emotion.

Recipe controls and bounds:

| Field under `genome` | Allowed values |
| --- | --- |
| `tone.freq_hz`, `tone.phase_offset`, `tone.amplitude` | 35–4000 Hz, 0–1 cycles, 0–1 |
| `envelope.attack_s`, `envelope.decay_s` | 0.001–0.5 s, 0.025–2 s (up to 16 s for space beds) |
| `envelope.sustain_level`, `envelope.release_s` | 0 for one-shots / 0–1 for space beds; release 0–0.5 s (applied to bed one-shots) |
| `envelope.curve` | `Linear` or `Exponential` |
| `filter.cutoff_hz`, `filter.q` | 150–16000 Hz, 0.5–2 |
| `noise.gain`, `body.gain` | 0–1.5, 0–1.2 |
| `sweep` | −0.85–3 initial pitch offset for older designs; positive exponential fall rate per second for version-3 laser |
| `texture`, `room` | 0–1 |
| `width` | 0–1.5 |
| `drive` | 1–4 |

Render rates: 22050–96000 Hz. Values must be finite. Original one-shot envelopes/tails remain bounded to under six seconds; space beds allow up to 16 s decay plus attack/release/room, and loop exports are bounded to 16 s; arbitrary graphs are not accepted through the command interface. Identical recipes/rates reproduce identical PCM on the same target; small floating-point differences may occur between native architectures and WASM.


For gull and engine previews, run `cargo run --release --example natural_audition`. This writes 48 kHz stereo WAVs and versioned JSON recipes for seeds 42, 108, and 791 to `audition/`. The version-five sources use continuous phase / combustion timing and two-times internal sampling with output filtering; they still pass through the shared stereo and peak-protection chain. Saved version-four recipes retain their prior arrangements. Signal tests measure vocal periodicity, falling pitch, engine firing rate, steady energy, and sample-rate stability; these are diagnostics, not a substitute for listening.


Space beds use dedicated source engines, not a shared noise-band swell.
`BeamLoop` is a beating blade hum with crackle; `FurnaceBed` is brown
combustion and embers; `GravityDrone` is a sub well; `HullRumble` is driven
steel plates; `VacuumLoop` is a Helmholtz cavity; `MagnetPulse` is a coil
cycle; `NaniteHiss` is a circling swarm of machine grains and close flybys; `ChoirInterval` is stacked throats. Loop baking warms up filters and
room for one second, renders the requested duration plus an overlap, then uses
complementary smooth crossfade weights at the wrap. It does not fade loop edges
to zero. Correlated material avoids an equal-power overlap boost; uncorrelated
noise can lose some energy within the 50 ms overlap. Tests measure seam steps
and windowed energy at supported sample rates; listening remains necessary.
`dry_mid` bypasses room and side detail, and mono WAV encoding averages the
channels once before PCM16 quantization. Timed layers preserve linear gain and
delay before a shared 0.89 peak-protection pass.


## Space theater field

The space bank has one current source design and theater processor per kind.
The voice arrangements in `src/cinematic/space.rs` retain their pitch, timing,
envelopes, material resonances, and direct stereo detail. The original 41 kinds
retain their presets, renders, and saved-recipe support.

The separate theater processor in `src/spatial.rs` uses five reflection paths
per channel, a 27 ms late-field predelay, four all-pass diffusers, and a damped
12-line feedback network. Distinct left/right reflection paths provide space
around the centered onset. Gentle envelope following reduces reflections during
strong attacks. The wet input rejects bass below approximately 180 Hz, and the
existing two 250 Hz side high-passes keep the low body centered. This is a baked
stereo field, not a multichannel theater format.

`room` controls reflection level and decay: nominal RT60 is `0.65 + 3.6 * room`
seconds, with frequency-dependent damping. One-shots allocate `0.1 + 5 * room`
seconds of tail (35 ms at room zero). `width = 0` gives identical channels while
retaining mid-channel room. For space sounds, `room = 0` removes the room reflections
but preserves any direct stereo detail; `dry_mid` bypasses both. Continuous beds
use the same field before the existing loop crossfade and WAV loop metadata.
Linked peak protection remains capped at 0.89.

Rust, WASM, and MCP use this processing through their existing render and asset
functions; their signatures and request fields are unchanged. Use `Recipe::new`
or `create_sound` to obtain the current recipe. `SoundKind::is_space()` identifies
space kinds without inspecting recipe numbers. The serialized `version` field
remains for API compatibility, but it is not a choice between space designs.
Space recipes load automatically into the current design while retaining seed
and genome settings. This applies equally to saved sessions and asset requests;
there is no migration step or older space renderer to select.

Export all 25 space kinds at 48 kHz, seed 42:

```sh
cargo run --release --example spatial_audition -- audition/spatial
```

The root directory contains stereo WAVs (eight-second loops for beds) and JSON
sidecars; each sidecar's `render_sound` object can be passed directly to the MCP
`render_sound` tool or the typed library request. `dry/` contains mono one-shot
source references, and `bank.json` records recipes, PCM hashes, and signal metrics.

Regression checks preserve exact PCM hashes for the current 25 space voices
and eight loops, plus the original 41 presets. Signal checks cover reflected-tail
energy, upper-band stereo spread, mono compatibility, loop seams, sample rates,
and peak protection. These
are signal checks; the WAVs are provided for listening review.
