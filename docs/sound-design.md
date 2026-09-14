# Evolution and sound design

Chirrp uses **`symbios-audio` 0.2.1**, the exact DSP core re-exported by **`bevy_symbios_audio` 0.4.2**, and its `symbios-genetics::Genotype` implementations for oscillator, envelope, filter, and gain mutation/crossover. The original wrapper unconditionally enables Bevy desktop defaults and installs a Rayon bake pool. Using its shared core with `bevy_app` / `bevy_ecs` keeps this library headless and browser-compatible.

The serial interactive evolutionary loop lives in Chirrp: Randomize implements **(1 + lambda) selection around a chosen favorite**; rated evolution uses **size-three tournaments, crossover, mutation, and one unchanged elite**. This uses the same node-level genetic operators as the upstream editor, not its egui widgets or threaded search runners. No Rayon work is dispatched (the upstream dependency still brings Rayon into the dependency tree).

Strength is a per-field mutation probability, not a promise of improvement. A zero-strength Randomize produces exact copies. Higher ratings in `[0,1]` steer `evolve`; provide one rating per candidate. User choices or an agent's evaluator define “better.” Unrated randomization does not learn taste or automatically assess cinematic quality. An agent can audition renders, use `analyze` as signal diagnostics, and supply its own ratings. The chosen favorite / highest-rated elite is preserved exactly, including its noise seed. Save snapshots and explicit future seeds to replay an evolutionary sequence.

The 25 catalogued space-bank arrangements now use version 7; eight have continuous bed synthesis. See [space-bank.md](space-bank.md). Fresh footstep and laser recipes use sound design version 3. The explosion default restores the original version-1 sound; the other original categories keep their version-2 designs. The additional scenes use version 4, except fresh rattle, calculator, wind, leaves, rustling, seagull, and car engine rumble recipes, which use version 5. Saved recipes keep the renderer specified by their version. Each category has a dedicated voice arrangement rather than the original shared oscillator/noise stack:

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

Version-2/3/4/5/6/7 mastering uses role-specific fixed gain with **attenuation-only peak protection**. The restored version-1 explosion retains its original mastering. It never boosts a quiet hover up to the level of a game impact. Category ceilings range from 0.045 for hover to **0.89 (~−1 dBFS)** for explosion. Saturation is gain-compensated. Stereo reflections feed a damped four-delay room with two all-pass diffusers, and tails are shorter. A two-pole 250 Hz high-pass keeps the side signal out of the deep bass. Width zero gives identical channels; mono downmix preserves the mid signal up to linked mastering gain. Start/end fades suppress boundary clicks. These are sample-peak ceilings, not true-peak or loudness normalization.

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


The cinema revision applies only to fresh space recipes (version 7). Its source
arrangements live in `src/cinematic/space.rs`: two-times sampling, four-pole
output filtering, pressure noise plus restrained sub fundamentals, 22 damped
inharmonic modes for material contacts, and voiced harmonic excitation through
formant filters for throat/choir sounds. These replace the exposed sine bends
and simple dyads of version 6. Source gain stays controlled before the existing
linked peak protection. Space one-shots can run up to nine seconds at extreme
edits, including the room tail; original one-shot bounds remain unchanged.

Version 7 has a dedicated room: 21 ms predelay, four all-pass input diffusers,
and an eight-line Householder feedback network with damping. Its RT60 control
is approximately 0.45 + 3.2 × room seconds; the allocated tail is 0.05 + 4 × room
seconds. The wet input rejects low bass. This room is bypassed by `dry_mid` and
is not applied to any original or version-six recipe. The previous room and
source code remain available for saved recipes.

The bank exporter now provides dry mono assets at `audition/space/`, stereo
cinematic auditions under `audition/space/cinematic/`, and a composed
`space_battle_showcase.wav` with an exact mix sidecar. Audio checks measure
transient contrast over fixed windows, low-band stereo balance, tail activity,
loop seams and rate limits. The redesign was signal-checked; the generated
previews are supplied for listening review rather than certified as perceptually
finished by automated tests.
