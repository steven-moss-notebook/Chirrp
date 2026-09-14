use crate::{Error, Recipe, Result, SoundKind};
use rand::SeedableRng;
use rand_chacha::ChaCha8Rng;
use serde::{Deserialize, Serialize};
use std::f32::consts::PI;
use symbios_audio::{
    AdsrEnvelope, AntiAlias, AudioPatch, BakeContext, BiquadLowpass, Connection as C, Gain,
    GraphNode, Mix, Node, NodeGraph, NodeId, NodeKind as N, PinkNoise, SawtoothOsc, SineOsc,
    WhiteNoise,
};

#[derive(Debug, Clone, Serialize)]
pub struct AudioMetrics {
    pub peak: f32,
    pub rms: f32,
    pub stereo_correlation: f32,
    pub duration_seconds: f32,
}

/// Interleaved stereo f32 PCM: L, R, L, R. Peak ceiling is 0.89 (~−1 dBFS).
#[derive(Debug, Clone)]
pub struct AudioBuffer {
    sample_rate: u32,
    samples: Vec<f32>,
    looped: bool,
}
impl AudioBuffer {
    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }
    pub fn channels(&self) -> u16 {
        2
    }
    pub fn frames(&self) -> usize {
        self.samples.len() / 2
    }
    pub fn samples(&self) -> &[f32] {
        &self.samples
    }
    pub fn into_samples(self) -> Vec<f32> {
        self.samples
    }
    pub fn metrics(&self) -> AudioMetrics {
        let (mut ll, mut rr, mut lr) = (0f64, 0f64, 0f64);
        let mut peak = 0f32;
        for pair in self.samples.chunks_exact(2) {
            let (l, r) = (pair[0] as f64, pair[1] as f64);
            ll += l * l;
            rr += r * r;
            lr += l * r;
            peak = peak.max(pair[0].abs()).max(pair[1].abs());
        }
        AudioMetrics {
            peak,
            rms: ((ll + rr) / self.samples.len() as f64).sqrt() as f32,
            stereo_correlation: if ll * rr > 1e-20 {
                (lr / (ll * rr).sqrt()).clamp(-1., 1.) as f32
            } else {
                1.
            },
            duration_seconds: self.frames() as f32 / self.sample_rate as f32,
        }
    }
    /// Metrics of the mono WAV downmix, before PCM16 quantization.
    pub fn mono_metrics(&self) -> AudioMetrics {
        let mut peak = 0f32;
        let mut energy = 0f64;
        for frame in self.samples.chunks_exact(2) {
            let mid = (frame[0] + frame[1]) * 0.5;
            peak = peak.max(mid.abs());
            energy += (mid as f64).powi(2);
        }
        AudioMetrics {
            peak,
            rms: (energy / self.frames() as f64).sqrt() as f32,
            stereo_correlation: 1.,
            duration_seconds: self.frames() as f32 / self.sample_rate as f32,
        }
    }
    /// Whether the complete buffer is a baked forward loop.
    pub fn is_loop(&self) -> bool {
        self.looped
    }

    /// Widely supported RIFF PCM16 stereo WAV, little endian.
    pub fn wav_bytes(&self) -> Vec<u8> {
        self.encode_wav(false)
    }
    /// PCM16 mono WAV for game spatializers; loop metadata is retained.
    pub fn wav_bytes_mono(&self) -> Vec<u8> {
        self.encode_wav(true)
    }
    fn encode_wav(&self, mono: bool) -> Vec<u8> {
        let channels = if mono { 1u16 } else { 2 };
        let size = (self.frames() * channels as usize * 2) as u32;
        let extra = if self.looped { 68 } else { 0 };
        let mut out = Vec::with_capacity(44 + size as usize + extra);
        out.extend_from_slice(b"RIFF");
        out.extend_from_slice(&(36 + size + extra as u32).to_le_bytes());
        out.extend_from_slice(b"WAVEfmt ");
        out.extend_from_slice(&16u32.to_le_bytes());
        out.extend_from_slice(&1u16.to_le_bytes());
        out.extend_from_slice(&channels.to_le_bytes());
        out.extend_from_slice(&self.sample_rate.to_le_bytes());
        out.extend_from_slice(&(self.sample_rate * channels as u32 * 2).to_le_bytes());
        out.extend_from_slice(&(channels * 2).to_le_bytes());
        out.extend_from_slice(&16u16.to_le_bytes());
        out.extend_from_slice(b"data");
        out.extend_from_slice(&size.to_le_bytes());
        for frame in self.samples.chunks_exact(2) {
            if mono {
                out.extend_from_slice(
                    &(((frame[0] + frame[1]) * 0.5 * 32767.).round() as i16).to_le_bytes(),
                );
            } else {
                for sample in frame {
                    out.extend_from_slice(&((sample * 32767.).round() as i16).to_le_bytes());
                }
            }
        }
        if self.looped {
            // RIFF smpl: one infinite forward loop, inclusive end frame.
            out.extend_from_slice(b"smpl");
            out.extend_from_slice(&60u32.to_le_bytes());
            for value in [
                0,
                0,
                (1_000_000_000. / self.sample_rate as f64).round() as u32,
                60,
                0,
                0,
                0,
                1,
                0,
                0,
                0,
                0,
                self.frames() as u32 - 1,
                0,
                0,
            ] {
                out.extend_from_slice(&value.to_le_bytes());
            }
        }
        out
    }
}

/// Mix any nonempty collection of rendered sounds using Symbios's [`Mix`] node.
///
/// All sounds start at frame zero. Stereo channels are summed independently,
/// and shorter sounds are padded with silence to the longest input's duration.
/// The sum retains unity gain unless its peak exceeds 0.89, in which case one
/// gain is applied to both channels across the entire output. A single input
/// is unchanged. Input buffers are borrowed and never modified.
///
/// Returns an error for an empty collection or mismatched sample rates.
/// Like [`render`], this allocates and is intended for offline use.
pub fn mix(sounds: &[&AudioBuffer]) -> Result<AudioBuffer> {
    let first = sounds
        .first()
        .ok_or_else(|| Error("mix requires at least one sound".into()))?;
    let sample_rate = first.sample_rate;
    if sounds.iter().any(|sound| sound.sample_rate != sample_rate) {
        return Err(Error(
            "all mixed sounds must have the same sample_rate".into(),
        ));
    }
    if sounds.len() == 1 {
        return Ok((*first).clone());
    }
    let frames = sounds.iter().map(|sound| sound.frames()).max().unwrap();
    let mixer = Mix::default();
    // Mix is stateless and does not use the RNG required by BakeContext.
    let mut rng = ChaCha8Rng::seed_from_u64(0);
    let mut inputs = vec![("in", 0.); sounds.len()];
    let mut samples = Vec::with_capacity(frames * 2);
    let mut peak = 0f32;
    for i in 0..frames * 2 {
        for (input, sound) in inputs.iter_mut().zip(sounds) {
            input.1 = sound.samples.get(i).copied().unwrap_or(0.);
        }
        // The same stateless node handles left and right independently.
        let mut ctx = BakeContext::new(
            sample_rate,
            (i / 2) as u64,
            frames as u64,
            &mut rng,
            &inputs,
            None,
        );
        let sample = mixer.sample(&mut ctx);
        if !sample.is_finite() {
            return Err(Error("mix produced non-finite samples".into()));
        }
        peak = peak.max(sample.abs());
        samples.push(sample);
    }
    if peak > 0.89 {
        let gain = 0.89 / peak;
        for sample in &mut samples {
            *sample *= gain;
        }
    }
    Ok(AudioBuffer {
        sample_rate,
        samples,
        looped: false,
    })
}

/// Render any nonempty collection of recipes at a common rate, then [`mix`]
/// their audio. Each recipe retains its own seed, sound design, and stereo room
/// tail. Supports the same sample rates and validation as [`render`].
pub fn render_mix(recipes: &[Recipe], sample_rate: u32) -> Result<AudioBuffer> {
    if recipes.is_empty() {
        return Err(Error("mix requires at least one sound".into()));
    }
    // Reject invalid recipes before doing any synthesis.
    for recipe in recipes {
        recipe.validate()?;
    }
    let sounds = recipes
        .iter()
        .map(|recipe| render(recipe, sample_rate))
        .collect::<Result<Vec<_>>>()?;
    mix(&sounds.iter().collect::<Vec<_>>())
}

fn node(id: u32, kind: N) -> GraphNode {
    GraphNode {
        id: NodeId(id),
        kind,
        ..Default::default()
    }
}
fn link(id: u32) -> C {
    C::from_node(NodeId(id))
}
fn modulate(id: u32, amount: f32) -> C {
    C::modulation(NodeId(id), amount)
}

/// Builds the actual Symbios DAG: pitched body, band-limited edge, upper
/// partial, sub impact, and independently enveloped noise/transient layers.
fn patch(recipe: &Recipe) -> AudioPatch {
    let g = &recipe.genome;
    let env = node(0, N::Adsr(g.envelope.clone())).with_input("gate", C::constant(1.));
    let sweep_env = node(
        1,
        N::Adsr(AdsrEnvelope {
            attack_s: 0.001,
            decay_s: g.envelope.decay_s * 0.7,
            ..g.envelope.clone()
        }),
    )
    .with_input("gate", C::constant(1.));
    let freq = g.tone.freq_hz;
    let tone = node(2, N::Sine(g.tone.clone())).with_input("freq", modulate(1, freq * g.sweep));
    let edge = node(
        3,
        N::Sawtooth(SawtoothOsc {
            freq_hz: freq * 1.006,
            amplitude: g.texture * 0.32,
            anti_alias: AntiAlias::PolyBlep,
            ..Default::default()
        }),
    )
    .with_input("freq", modulate(1, freq * g.sweep));
    let shimmer = node(
        4,
        N::Sine(SineOsc {
            freq_hz: freq * 2.003,
            amplitude: g.texture * 0.24,
            phase_offset: 0.17,
        }),
    )
    .with_input("freq", modulate(1, freq * g.sweep * 2.));
    let body = node(
        5,
        N::Sine(SineOsc {
            freq_hz: (freq * 0.45).clamp(35., 100.),
            amplitude: g.body.gain,
            phase_offset: 0.,
        }),
    )
    .with_input("freq", modulate(1, 70. * g.body.gain));
    let mix = node(6, N::Mix(Mix { gain: 0.7 }))
        .with_input("a", link(2))
        .with_input("b", link(3))
        .with_input("c", link(4))
        .with_input("d", link(5));
    let shaped = node(7, N::Gain(Gain { gain: 0. }))
        .with_input("in", link(6))
        .with_input("gain", link(0));
    let noise = node(
        8,
        N::PinkNoise(PinkNoise {
            amplitude: g.noise.gain,
        }),
    );
    let filtered = node(9, N::BiquadLowpass(g.filter.clone()))
        .with_input("in", link(8))
        .with_input("cutoff_hz", modulate(0, g.filter.cutoff_hz * 0.2));
    let air = node(10, N::Gain(Gain { gain: 0. }))
        .with_input("in", link(9))
        .with_input("gain", link(0));
    let crack_env = node(
        11,
        N::Adsr(AdsrEnvelope {
            attack_s: 0.001,
            decay_s: if recipe.kind == SoundKind::Footstep {
                0.055
            } else {
                0.018
            },
            ..g.envelope.clone()
        }),
    )
    .with_input("gate", C::constant(1.));
    let crack = node(
        12,
        N::WhiteNoise(WhiteNoise {
            amplitude: g.noise.gain * 0.4,
        }),
    );
    let crack_vca = node(13, N::Gain(Gain { gain: 0. }))
        .with_input("in", link(12))
        .with_input("gain", link(11));
    let out = node(14, N::Mix(Mix { gain: 0.7 }))
        .with_input("tone", link(7))
        .with_input("air", link(10))
        .with_input("crack", link(13));
    // A final filter also controls tonal brightness and catches hot upper partials.
    let out_filter = node(
        15,
        N::BiquadLowpass(BiquadLowpass {
            cutoff_hz: g.filter.cutoff_hz,
            q: 0.707,
        }),
    )
    .with_input("in", link(14));
    AudioPatch {
        seed: recipe.seed,
        graph: NodeGraph {
            nodes: vec![
                env, sweep_env, tone, edge, shimmer, body, mix, shaped, noise, filtered, air,
                crack_env, crack, crack_vca, out, out_filter,
            ],
            output: NodeId(15),
        },
    }
}

struct Delay {
    data: Vec<f32>,
    pos: usize,
    damp: f32,
}
impl Delay {
    fn new(seconds: f32, sr: u32) -> Self {
        Self {
            data: vec![0.; (seconds * sr as f32).round().max(1.) as usize],
            pos: 0,
            damp: 0.,
        }
    }
    fn read(&self) -> f32 {
        self.data[self.pos]
    }
    fn push(&mut self, x: f32) {
        self.data[self.pos] = x;
        self.pos = (self.pos + 1) % self.data.len();
    }
}
struct Highpass {
    coefficient: f32,
    input: f32,
    output: f32,
}

/// All-pass diffusion smooths the discrete reflections before the room network.
fn diffuse(delay: &mut Delay, input: f32) -> f32 {
    let output = delay.read() - input * 0.6;
    delay.push(input + output * 0.6);
    output
}
impl Highpass {
    fn new(hz: f32, sr: u32) -> Self {
        Self {
            coefficient: (-2. * PI * hz / sr as f32).exp(),
            input: 0.,
            output: 0.,
        }
    }
    fn tick(&mut self, x: f32) -> f32 {
        let y = self.coefficient * (self.output + x - self.input);
        self.input = x;
        self.output = y;
        y
    }
}

/// Offline render at 22.05–96 kHz. Bed recipes support up to 16 seconds of decay. No device, filesystem, clock, global RNG, or threads are used.
pub fn render(recipe: &Recipe, sample_rate: u32) -> Result<AudioBuffer> {
    render_with_options(recipe, sample_rate, RenderOptions::default())
}

/// Additive export controls; default options preserve the original renderer.
#[derive(Debug, Default, Clone, Copy, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct RenderOptions {
    /// Bypass room and all side detail, returning identical dry mid channels.
    pub dry_mid: bool,
}

pub fn render_with_options(
    recipe: &Recipe,
    sample_rate: u32,
    options: RenderOptions,
) -> Result<AudioBuffer> {
    recipe.validate()?;
    if !(22_050..=96_000).contains(&sample_rate) {
        return Err(Error("sample_rate must be in [22050, 96000]".into()));
    }
    let g = &recipe.genome;
    let modern = recipe.version >= 2;
    let (dry, direct_side) = if modern {
        crate::design::dry(recipe, sample_rate)?
    } else {
        let dry_seconds = g.envelope.attack_s + g.envelope.decay_s + 0.035;
        (
            symbios_audio::try_bake(&patch(recipe), sample_rate, dry_seconds)
                .map_err(|e| Error(e.to_string()))?,
            Vec::new(),
        )
    };
    finish(recipe, sample_rate, &dry, &direct_side, false, options)
}

fn finish(
    recipe: &Recipe,
    sample_rate: u32,
    dry: &[f32],
    direct_side: &[f32],
    continuous: bool,
    options: RenderOptions,
) -> Result<AudioBuffer> {
    let g = &recipe.genome;
    let modern = recipe.version >= 2;
    let cinema = recipe.version >= 7 && recipe.kind.is_space();
    let mut cinema_room = cinema.then(|| crate::cinematic::SpaceRoom::new(sample_rate, g.room));
    let tail = if continuous || options.dry_mid {
        0.
    } else if cinema {
        0.05 + g.room * 4.
    } else if modern {
        0.035 + g.room * 1.6
    } else {
        0.12 + g.room * 2.4
    };
    let frames = dry.len() + (tail * sample_rate as f32).ceil() as usize;
    let mut samples = Vec::with_capacity(frames * 2);
    let mut dc = Highpass::new(25., sample_rate);
    let mut side_hp = [
        Highpass::new(250., sample_rate),
        Highpass::new(250., sample_rate),
    ];
    let mut early = [
        Delay::new(0.0097, sample_rate),
        Delay::new(0.0173, sample_rate),
    ];
    let times = [0.0297, 0.0371, 0.0411, 0.0437];
    let mut lines = times.map(|t| Delay::new(t, sample_rate));
    let rt60 = if modern {
        0.1 + g.room * 0.95
    } else {
        0.18 + g.room * 1.6
    };
    let feedback = times.map(|t| 0.001_f32.powf(t / rt60));
    let damping = 1. - (-2. * PI * if modern { 3200. } else { 5500. } / sample_rate as f32).exp();
    let mut diffusion = [
        Delay::new(0.0073, sample_rate),
        Delay::new(0.0117, sample_rate),
    ];
    let fade_frames = (0.03 * sample_rate as f32) as usize;
    for i in 0..frames {
        let saturated = (dry.get(i).copied().unwrap_or(0.) * g.drive).tanh();
        let input = dc.tick(if modern {
            saturated / g.drive
        } else {
            saturated
        });
        if options.dry_mid {
            let fade = if continuous {
                1.
            } else {
                ((frames - 1 - i) as f32 / fade_frames as f32).min(1.)
                    * (i as f32 / (sample_rate as f32 * 0.0008)).min(1.)
            };
            samples.extend_from_slice(&[input * fade, input * fade]);
            continue;
        }
        if let Some(room) = &mut cinema_room {
            let (wet_mid, wet_side) = room.tick(input);
            let mid = input + wet_mid * g.room * 0.9;
            let direct = direct_side.get(i).copied().unwrap_or(0.);
            let mut side = wet_side * g.room * 1.2 + (direct * g.drive).tanh() / g.drive;
            for hp in &mut side_hp {
                side = hp.tick(side);
            }
            side *= g.width;
            let fade = if continuous {
                1.
            } else {
                ((frames - 1 - i) as f32 / fade_frames as f32).min(1.)
                    * (i as f32 / (sample_rate as f32 * 0.0008)).min(1.)
            };
            samples.extend_from_slice(&[(mid + side) * fade, (mid - side) * fade]);
            continue;
        }
        let mut room_input = input;
        if modern {
            for diffuser in &mut diffusion {
                room_input = diffuse(diffuser, room_input);
            }
        }
        let a = early[0].read();
        let b = early[1].read();
        early[0].push(input);
        early[1].push(input);
        let reads = lines.each_ref().map(|line| line.read());
        // Orthogonal Hadamard feedback network: energy-preserving mixing with
        // frequency-dependent damping and per-line RT60 attenuation.
        let [a0, a1, a2, a3] = reads;
        let mix = [
            a0 + a1 + a2 + a3,
            a0 - a1 + a2 - a3,
            a0 + a1 - a2 - a3,
            a0 - a1 - a2 + a3,
        ];
        for j in 0..4 {
            let line = &mut lines[j];
            line.damp += damping * (mix[j] * 0.5 - line.damp);
            line.push(room_input * 0.22 + line.damp * feedback[j]);
        }
        let mid = input + g.room * (a0 + a1 + a2 + a3) * 0.22;
        let early_level = if modern { 0.12 } else { 0.22 };
        let mut side = (a - b) * early_level + g.room * (a0 - a1 + a2 - a3) * 0.3;
        if let Some(direct) = direct_side.get(i) {
            side += (direct * g.drive).tanh() / g.drive;
        }
        for hp in &mut side_hp {
            side = hp.tick(side);
        }
        side *= g.width;
        let fade_out = ((frames - 1 - i) as f32 / fade_frames as f32).min(1.);
        let fade_in = (i as f32 / (sample_rate as f32 * 0.0008)).min(1.);
        let fade = if continuous { 1. } else { fade_in * fade_out };
        samples.push((mid + side) * fade);
        samples.push((mid - side) * fade);
    }
    if samples.iter().any(|x| !x.is_finite()) {
        return Err(Error("synthesis produced non-finite samples".into()));
    }
    // V2 preserves designed loudness and only attenuates peaks. V1 keeps its
    // original mastering so saved recipes still reproduce their old audio.
    let peak = samples.iter().fold(0f32, |p, x| p.max(x.abs()));
    let gain = if modern {
        let (level, ceiling) = crate::design::mastering(recipe);
        if peak > 1e-8 {
            level.min(ceiling / peak)
        } else {
            level
        }
    } else if peak > 1e-8 {
        (0.89 / peak).min(2.)
    } else {
        1.
    };
    for x in &mut samples {
        *x *= gain;
    }
    Ok(AudioBuffer {
        sample_rate,
        samples,
        looped: false,
    })
}

/// A timed recipe layer. Gain is linear; delay is measured from the mix start.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MixLayer {
    pub recipe: Recipe,
    #[serde(default = "unity")]
    pub gain: f32,
    #[serde(default)]
    pub delay_s: f32,
}
fn unity() -> f32 {
    1.
}
impl MixLayer {
    pub fn new(recipe: Recipe, gain: f32, delay_s: f32) -> Self {
        Self {
            recipe,
            gain,
            delay_s,
        }
    }
}

/// Render 1–32 layers, preserving delayed tails and protecting the sum at 0.89.
/// Delay is rounded to the nearest frame. Gain is 0–8; delay is 0–16 seconds.
pub fn render_mix_layers(layers: &[MixLayer], sample_rate: u32) -> Result<AudioBuffer> {
    render_mix_layers_with_options(layers, sample_rate, RenderOptions::default())
}

pub fn render_mix_layers_with_options(
    layers: &[MixLayer],
    sample_rate: u32,
    options: RenderOptions,
) -> Result<AudioBuffer> {
    if layers.is_empty() || layers.len() > 32 {
        return Err(Error("mix requires 1–32 layers".into()));
    }
    validate_rate(sample_rate)?;
    for layer in layers {
        layer.recipe.validate()?;
        crate::range("gain", layer.gain, 0., 8.)?;
        crate::range("delay_s", layer.delay_s, 0., 16.)?;
    }
    // Render and accumulate sequentially to avoid holding 32 long bed buffers.
    let mut samples = Vec::<f32>::new();
    for layer in layers {
        let audio = render_with_options(&layer.recipe, sample_rate, options)?;
        let offset = (layer.delay_s * sample_rate as f32).round() as usize * 2;
        samples.resize(samples.len().max(offset + audio.samples.len()), 0.);
        for (out, input) in samples[offset..].iter_mut().zip(audio.samples) {
            *out += input * layer.gain;
        }
    }
    protect(&mut samples);
    Ok(AudioBuffer {
        sample_rate,
        samples,
        looped: false,
    })
}

fn validate_rate(sample_rate: u32) -> Result<()> {
    if !(22_050..=96_000).contains(&sample_rate) {
        return Err(Error("sample_rate must be in [22050, 96000]".into()));
    }
    Ok(())
}
fn protect(samples: &mut [f32]) {
    let peak = samples.iter().fold(0f32, |p, s| p.max(s.abs()));
    if peak > 0.89 {
        for s in samples {
            *s *= 0.89 / peak;
        }
    }
}

/// Bake an exact-length 0.1–16 second loop, rounded to the nearest frame.
/// Beds use continuous synthesis with a one-second filter/room preroll.
/// Other kinds repeat their one-shot at its natural duration (e.g. UI alarms).
/// A 50 ms (at most one quarter loop) complementary smooth crossfade replaces
/// the end with preroll leading into the start. No boundary fade to silence is
/// applied. WAV exports include a forward, infinite `smpl` loop over all frames.
pub fn render_loop(recipe: &Recipe, sample_rate: u32, loop_s: f32) -> Result<AudioBuffer> {
    render_loop_with_options(recipe, sample_rate, loop_s, RenderOptions::default())
}

pub fn render_loop_with_options(
    recipe: &Recipe,
    sample_rate: u32,
    loop_s: f32,
    options: RenderOptions,
) -> Result<AudioBuffer> {
    recipe.validate()?;
    validate_rate(sample_rate)?;
    crate::range("loop_s", loop_s, 0.1, 16.)?;
    let frames = (loop_s * sample_rate as f32).round() as usize;
    let cross = ((0.05 * sample_rate as f32).round() as usize).min(frames / 4);
    let (source, start) = if recipe.kind.is_bed() {
        let warm = sample_rate as usize;
        let (dry, side) = if recipe.version >= 7 {
            crate::cinematic::space_bed(recipe, sample_rate, warm + frames + cross)
        } else {
            (
                crate::cinematic::bed(recipe, sample_rate, warm + frames + cross, true),
                Vec::new(),
            )
        };
        (
            finish(recipe, sample_rate, &dry, &side, true, options)?.samples,
            warm,
        )
    } else {
        let shot = render_with_options(recipe, sample_rate, options)?;
        let mut source = Vec::with_capacity((frames + cross) * 2);
        for i in 0..frames + cross {
            let at = (i % shot.frames()) * 2;
            source.extend_from_slice(&shot.samples[at..at + 2]);
        }
        (source, 0)
    };
    let mut samples = source[(start + cross) * 2..(start + cross + frames) * 2].to_vec();
    for i in 0..cross {
        let u = i as f32 / (cross - 1) as f32;
        let w = u * u * (3. - 2. * u);
        for ch in 0..2 {
            let end = (frames - cross + i) * 2 + ch;
            samples[end] = samples[end] * (1. - w) + source[(start + i) * 2 + ch] * w;
        }
    }
    protect(&mut samples);
    Ok(AudioBuffer {
        sample_rate,
        samples,
        looped: true,
    })
}
