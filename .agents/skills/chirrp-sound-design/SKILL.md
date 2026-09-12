---
name: chirrp-sound-design
description: Design, evaluate, and master procedural game sounds with Chirrp, including layered synthesis, stereo widening, reverb, peak protection, and normalization decisions. Use for sound creation or DSP changes, not routine MCP transport or build work.
---

# Chirrp sound design and mastering

Use this skill to turn a gameplay intent into a purposeful, repeatable sound. “High end” means a clear action or material, controlled spectrum and dynamics, convincing timing, useful variation, stable stereo imaging, and an appropriate place in the game's mix. It does not mean maximum width, reverb, brightness, or volume.

## Establish the role and design the source

Use the request's context to choose prominence, duration, repetition rate, material, and perspective. A hover should tolerate frequent repetition; a heavy impact can carry more low-frequency energy and contrast. Prefer the closest preset and a saved seed as a baseline. Through MCP, discover presets with `list_sounds`, use `generate_sound` for an export or `create_sound` / `edit_sound` / `randomize` for exploration, and save recipes with the final assets. Read [the MCP guide](../../../docs/mcp.md) for actual tools and arguments.

Build a coherent onset, body, and decay before adding spatial effects. Align layers to one physical gesture; give tonal resonances, noisy contact, sub impact, and follow-through distinct envelopes and frequency roles. A quieter scuff behind a footstep can suggest material without obscuring the contact. Seed small variations in excitation and timing; randomization is not a quality evaluator. Band-limit bright oscillators, inspect aliasing at the intended sample rate, and retain the attack rather than trying to fix a weak source with saturation or peak gain.

## Understand the mastering chain that actually exists

The renderer does this in order:

1. Render the designed dry mid signal and any direct side detail with deterministic excitation. Category arrangements, filtering, envelopes, and some oversampling happen inside the dry voice design.
2. Apply drive-compensated saturation, `tanh(dry * drive) / drive`, then a 25 Hz high-pass on the mid input. This controls waveform density and DC/very-low-frequency energy; it is not a lookahead limiter.
3. Build early reflections and a damped four-delay room. The modern room input passes through two all-pass diffusers; a Hadamard feedback network spreads energy and frequency-dependent damping darkens decay. `room` influences room contribution and decay/tail duration. It is not a full independently adjustable studio reverb.
4. Combine early-reflection difference, room difference, and direct stereo detail into a side signal. Apply two cascaded 250 Hz high-passes to the side, then multiply it by `width`. Form `L = mid + side`, `R = mid - side`.
5. Apply the shared boundary fades: approximately 0.8 ms in and up to 30 ms out, reaching zero at the boundaries. Allocate the room tail before the fade; inspect short sounds so a fade does not hide the intended transient.
6. Measure one absolute sample peak across both channels. Apply a single fixed gain for the complete buffer, limited by the category ceiling. Validate finite samples and retain the resulting stereo balance.

For peak `p > 1e-8`, the final gain is `min(designed_level, category_ceiling / p)`. The fixed `designed_level` may exceed 1. “Attenuation-only protection” means the peak stage can only reduce that intended level; it does **not** mean all modern raw samples are never amplified. Very small peaks use the designed gain directly.

Example modern ceilings are hover 0.045, click 0.24, laser 0.68, and impact 0.84. The overall maximum ceiling is 0.89, about −1 dBFS. These are role-specific sample peaks, not perceived loudness targets.

## Make stereo and room serve the sound

Keep the low body anchored in the mid channel and let higher-frequency material detail and reflections carry width. `width = 0` produces identical channels. Downmixing `(L + R) / 2` recovers the mid, up to shared mastering gain; widening can still lower that gain by increasing the stereo peak. Compare both mono compatibility and the level change.

A larger width is not automatically better. Check headphones, speakers, mono downmix, transient localization, and repeated playback. Broad noisy tails can be useful for a scene but distracting on a UI tick. Stereo correlation is a diagnostic, not a score to maximize or minimize. Avoid widening by simply delaying or polarity-inverting the entire dry signal: that can smear localization or weaken the mono body.

`room = 0` does not remove all stereo reflections: the early side reflections are still present when width is nonzero. For a centered, minimally spatial cue use both `room = 0` and `width = 0`. Width zero alone keeps any mid-channel room contribution. The current renderer is an offline effect baked into the asset; stacking it with a game's room bus can create an excessively wet sound. For world-positioned emitters, consider mono assets plus the game's spatializer rather than baking broad stereo into every source.

## Distinguish mastering, normalization, and mixing

- **Mastering here:** finishing the designed signal with spatial processing, boundary cleanup, chosen gain, and linked sample-peak protection. The implementation does not perform general-purpose compression, automatic EQ, true-peak limiting, or LUFS normalization.
- **Peak normalization:** scaling a complete buffer toward a chosen maximum sample value. It preserves relative dynamics but can raise quiet noise and flatten the intended level hierarchy across assets. Do not independently normalize every UI cue and impact to the same peak.
- **Loudness normalization:** measuring perceived loudness over a defined window and applying gain to a target. Whole-buffer RMS is not LUFS, and silence/tail length can distort comparisons between short effects. Use an external loudness/true-peak meter if the delivery brief requires those measurements; do not claim Chirrp measures them.
- **Mixing:** `mix` / `render_mix` start all inputs together, sum channels independently, preserve the longest tail, and attenuate the complete mix only if its sample peak exceeds 0.89. A single input is unchanged. This prevents clipping for that exported mix, but does not preserve each category's individual ceiling after layering or protect later simultaneous game playback.

Keep stereo gain linked. Separately normalizing left and right changes the image. Master the combined asset after layering; avoid repeatedly normalizing intermediate layers. Leave headroom and manage runtime voice counts, bus gain, and optional bus limiting for polyphony. The exported ceiling does not guarantee that many simultaneous AudioPlayers will not clip.

Chirrp returns interleaved stereo f32 and exports PCM16 WAV by rounding to integer samples; it adds no dither. Keep f32 during further processing where possible, and quantize once at delivery. Sample-peak protection does not guarantee an intersample true-peak ceiling after conversion or lossy encoding. Add appropriate external true-peak checking or dither only when the delivery workflow calls for it, without changing legacy exports as an incidental cleanup.

## Evaluate the actual deliverable

Compare candidates at comparable listening levels while preserving the final role hierarchy. Listen for onset clarity, harsh resonances, aliasing, noise-floor buildup, low-end control, distracting pitch, tail truncation, and repetitive fatigue. Audition rapid repeated and overlapping triggers in the intended gameplay context. A larger RMS or lower correlation alone does not establish quality.

Use `analyze` or `AudioBuffer::metrics()` for finite samples, peak, RMS, correlation, and duration. Check both channels, mono, start/end behavior, and intended sample rates. For DSP changes, run relevant signal and legacy reproduction tests; add tests for meaningful new acoustic invariants rather than subjective adjectives. Tests support listening, not a claim that an unheard sound is polished.

Deliver the recipe/version/seed, sample rate, WAV or PCM asset, and a short account of the intended role and processing. State what was measured and what was actually auditioned.