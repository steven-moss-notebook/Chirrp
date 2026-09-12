use crate::{Error, Recipe, Result, SoundKind};
use serde::Serialize;
use std::f32::consts::PI;
use symbios_audio::{
    AdsrEnvelope, AntiAlias, AudioPatch, BiquadLowpass, Connection as C, Gain, GraphNode, Mix,
    NodeGraph, NodeId, NodeKind as N, PinkNoise, SawtoothOsc, SineOsc, WhiteNoise,
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
    /// Widely supported RIFF PCM16 stereo WAV, little endian.
    pub fn wav_bytes(&self) -> Vec<u8> {
        let size = (self.samples.len() * 2) as u32;
        let mut out = Vec::with_capacity(44 + size as usize);
        out.extend_from_slice(b"RIFF");
        out.extend_from_slice(&(36 + size).to_le_bytes());
        out.extend_from_slice(b"WAVEfmt ");
        out.extend_from_slice(&16u32.to_le_bytes());
        out.extend_from_slice(&1u16.to_le_bytes());
        out.extend_from_slice(&2u16.to_le_bytes());
        out.extend_from_slice(&self.sample_rate.to_le_bytes());
        out.extend_from_slice(&(self.sample_rate * 4).to_le_bytes());
        out.extend_from_slice(&4u16.to_le_bytes());
        out.extend_from_slice(&16u16.to_le_bytes());
        out.extend_from_slice(b"data");
        out.extend_from_slice(&size.to_le_bytes());
        for sample in &self.samples {
            out.extend_from_slice(&((sample * 32767.).round() as i16).to_le_bytes());
        }
        out
    }
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

/// Offline render at 22.05–96 kHz. Maximum recipe duration is bounded to under
/// six seconds. No device, filesystem, clock, global RNG, or threads are used.
pub fn render(recipe: &Recipe, sample_rate: u32) -> Result<AudioBuffer> {
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
    let tail = if modern {
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
        let fade = fade_in * fade_out;
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
    })
}
