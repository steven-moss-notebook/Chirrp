//! Version-two sound direction. Each action has its own excitation, timing,
//! and harmonic structure; Symbios still renders and evolves every voice.
use crate::{Error, Recipe, Result, SoundKind};
use symbios_audio::{
    AdsrCurve, AdsrEnvelope, AudioPatch, BiquadBandpass, BiquadLowpass, Connection as C, Gain,
    GraphNode, NodeGraph, NodeId, NodeKind as N, PinkNoise, SineOsc, WhiteNoise,
};

#[derive(Clone, Copy)]
enum Source {
    Tone(f32),
    Air,
    Contact,
}
#[derive(Clone, Copy)]
pub(super) struct Voice {
    source: Source,
    pub(super) at: f32,
    pub(super) attack: f32,
    pub(super) decay: f32,
    pub(super) level: f32,
    pub(super) cutoff: f32,
    pub(super) band: Option<(f32, f32)>,
    pub(super) sweep: f32,
}
impl Voice {
    pub(super) fn tone(hz: f32, at: f32, attack: f32, decay: f32, level: f32) -> Self {
        Self {
            source: Source::Tone(hz),
            at,
            attack,
            decay,
            level,
            cutoff: 6000.,
            band: None,
            sweep: 0.,
        }
    }
    pub(super) fn noise(at: f32, attack: f32, decay: f32, level: f32, cutoff: f32) -> Self {
        Self {
            source: Source::Contact,
            at,
            attack,
            decay,
            level,
            cutoff,
            band: None,
            sweep: 0.,
        }
    }
    pub(super) fn band(mut self, hz: f32, q: f32) -> Self {
        self.band = Some((hz, q));
        self
    }
    pub(super) fn air(mut self) -> Self {
        self.source = Source::Air;
        self
    }
    pub(super) fn sweep(mut self, amount: f32) -> Self {
        self.sweep = amount;
        self
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

pub(super) fn bake_voice(v: Voice, recipe: &Recipe, sr: u32) -> Result<Vec<f32>> {
    let g = &recipe.genome;
    // Keep all evolving oscillators and their sweeps below Nyquist.
    let kind = match v.source {
        Source::Tone(hz) => N::Sine(SineOsc {
            freq_hz: hz.min(sr as f32 * 0.42 / (1. + v.sweep.max(0.))),
            amplitude: 1.,
            phase_offset: g.tone.phase_offset,
        }),
        Source::Air => N::PinkNoise(PinkNoise { amplitude: 1. }),
        Source::Contact => N::WhiteNoise(WhiteNoise { amplitude: 1. }),
    };
    let envelope = AdsrEnvelope {
        attack_s: v.attack,
        decay_s: v.decay,
        sustain_level: 0.,
        release_s: 0.,
        curve: AdsrCurve::Exponential,
    };
    let env = node(0, N::Adsr(envelope.clone())).with_input("gate", C::constant(1.));
    let mut source = node(1, kind);
    let bend = node(
        2,
        N::Adsr(AdsrEnvelope {
            attack_s: 0.001,
            decay_s: v.decay * 0.32,
            ..envelope
        }),
    )
    .with_input("gate", C::constant(1.));
    if let Source::Tone(hz) = v.source {
        let hz = hz.min(sr as f32 * 0.42 / (1. + v.sweep.max(0.)));
        source = source.with_input("freq", C::modulation(NodeId(2), hz * v.sweep));
    }
    let color = if let Some((center_hz, q)) = v.band {
        N::BiquadBandpass(BiquadBandpass { center_hz, q })
    } else {
        N::BiquadLowpass(BiquadLowpass {
            cutoff_hz: v.cutoff,
            q: 0.707,
        })
    };
    let filter = node(3, color).with_input("in", link(1));
    let vca = node(4, N::Gain(Gain { gain: 0. }))
        .with_input("in", link(3))
        .with_input("gain", C::modulation(NodeId(0), v.level));
    let mut nodes = vec![env, source, bend, filter, vca];
    let output = if recipe.version >= 3 && v.band.is_some() {
        // A broad contact band still leaks high-frequency hiss; apply its
        // material low-pass as a separate stage after the contact envelope.
        nodes.push(
            node(
                5,
                N::BiquadLowpass(BiquadLowpass {
                    cutoff_hz: v.cutoff,
                    q: 0.707,
                }),
            )
            .with_input("in", link(4)),
        );
        NodeId(5)
    } else {
        NodeId(4)
    };
    let patch = AudioPatch {
        seed: recipe.seed,
        graph: NodeGraph { nodes, output },
    };
    symbios_audio::try_bake(&patch, sr, v.attack + v.decay + 0.012)
        .map_err(|e| Error(e.to_string()))
}

/// Gains preserve the intended role of each sound. Ceilings only attenuate;
/// a hover can never be normalized into a loud notification.
pub(crate) fn mastering(recipe: &Recipe) -> (f32, f32) {
    use SoundKind::*;
    match recipe.kind {
        Wind if recipe.version >= 5 => (0.8, 0.18),
        Leaves if recipe.version >= 5 => (1.1, 0.3),
        Rustling if recipe.version >= 5 => (1.2, 0.4),
        Calculator => (1.2, 0.55),
        UiHover => (0.22, 0.045),
        UiClick => (0.65, 0.24),
        UiConfirm => (0.8, 0.46),
        UiError => (0.7, 0.32),
        Footstep => (1.3, 0.52),
        Pickup => (0.7, 0.32),
        Laser => (1., 0.68),
        Impact => (1.4, 0.84),
        Explosion => (1.25, 0.89),
        Jump => (0.8, 0.48),
        Whoosh => (1.1, 0.58),
        PowerUp => (0.85, 0.74),
        Thunder | Stomp | Drop => (1.65, 0.89),
        Ring | Waves | Wind => (1.45, 0.86),
        _ => (1.5, 0.82),
    }
}

pub(crate) fn dry(recipe: &Recipe, sr: u32) -> Result<(Vec<f32>, Vec<f32>)> {
    if recipe.kind == SoundKind::Laser && recipe.version >= 3 {
        return Ok((crate::laser::dry(recipe, sr), Vec::new()));
    }
    use SoundKind::*;
    let g = &recipe.genome;
    let f = g.tone.freq_hz;
    let a = g.envelope.attack_s;
    let d = g.envelope.decay_s;
    let t = g.tone.amplitude;
    let n = g.noise.gain;
    let b = g.body.gain;
    let x = g.texture;
    let cutoff = g.filter.cutoff_hz;
    let mut voices = match recipe.kind {
        UiHover => vec![
            Voice::tone(f, 0., a.max(0.006), d.min(0.065), t * 0.55),
            Voice::noise(0., 0.008, 0.023, n * 0.4, 1500.),
        ],
        UiClick => vec![
            Voice::noise(0., a, 0.026, n, cutoff).band(1100., 0.65),
            Voice::tone(f, 0., 0.002, d, t * 0.3),
        ],
        UiConfirm => {
            // C–E–G resolving into a soft C major voicing; no rising siren.
            vec![
                Voice::tone(f, 0., a, d * 0.6, t * 0.46),
                Voice::tone(f * 1.25, 0.075, a, d * 0.65, t * 0.48),
                Voice::tone(f * 1.5, 0.15, a, d, t * 0.52),
                Voice::tone(f * 2., 0.15, 0.014, d * 0.8, t * 0.16),
                Voice::tone(f * 0.5, 0.15, 0.02, d * 0.9, b * 0.22),
                Voice::tone(f * 3., 0.15, 0.01, d * 0.32, x * 0.06),
            ]
        }
        UiError => vec![
            // A subdued downward minor third, followed by a darker fifth.
            Voice::tone(f, 0., a, d * 0.55, t * 0.5),
            Voice::tone(f * 0.84, 0.11, a * 1.4, d, t * 0.52),
            Voice::tone(f * 0.42, 0.11, 0.018, d * 0.8, b * 0.2),
            Voice::tone(f * 1.68, 0.11, 0.01, d * 0.35, x * 0.06),
        ],
        Footstep if recipe.version >= 3 => {
            let material = (f / 110.).clamp(0.65, 1.6);
            let jitter = (recipe.seed % 101) as f32 / 100.;
            vec![
                Voice::noise(0., a.max(0.003), d * 0.5, b * 2.6, 350.).band(125. * material, 0.6),
                Voice::noise(0.025 + jitter * 0.008, 0.008, d * 0.5, b * 0.55, 300.)
                    .band(190. * material, 0.5),
                Voice::noise(0.035 + jitter * 0.008, 0.005, d * 0.3, n * 0.45, 650.)
                    .band(420. * material, 0.65),
                Voice::noise(0.065 + jitter * 0.01, 0.004, d * 0.15, n * 0.06, 800.)
                    .band(750. * material, 0.55),
            ]
        }
        Footstep => {
            // Broad, damped contact resonances, not ringing pitched oscillators.
            // Heel -> compressed sole -> toe scuff, with seed-dependent timing.
            let material = (f / 110.).clamp(0.65, 1.6);
            let jitter = (recipe.seed % 101) as f32 / 100.;
            vec![
                Voice::noise(0., a.max(0.003), d * 0.43, b * 2.4, 400.).band(125. * material, 0.65),
                Voice::noise(0.022 + jitter * 0.01, 0.008, d * 0.55, n * 1.1, cutoff)
                    .band(620. * material, 0.5),
                Voice::noise(0.065 + jitter * 0.015, 0.014, d * 0.4, n * 0.36, cutoff)
                    .band(1400. * material, 0.55),
                Voice::noise(0.014, 0.006, 0.018, x * 0.18, 1800.),
            ]
        }
        Laser => vec![
            // Weight comes from lower harmonics; a short controlled bend gives
            // the weapon cue without the old multi-octave high-pitched zap.
            Voice::tone(f, 0., a, d, t * 0.55).sweep(g.sweep.clamp(0., 0.65)),
            Voice::tone(f * 1.5, 0., a, d * 0.55, x * 0.2).sweep(g.sweep.clamp(0., 0.65)),
            Voice::tone(f * 0.5, 0., 0.003, d * 0.45, b * 0.45).sweep(0.3),
            Voice::noise(0., 0.002, 0.024, n * 0.5, 1800.),
            Voice::noise(0.012, 0.012, d * 0.45, n * 0.18, cutoff).air(),
        ],
        Impact => vec![
            // These contact layers share the same seeded white excitation and
            // onset, then decay at different rates as one struck object.
            Voice::noise(0., a, d * 0.65, b * 2.6, 500.).band(f.clamp(65., 220.), 0.7),
            Voice::noise(0., a, d * 0.3, n * 0.75, cutoff).band(560., 0.55),
            Voice::noise(0., a, 0.022, n * 0.55, cutoff),
            Voice::tone(f * 1.9, 0., a, d * 0.22, x * 0.06),
        ],
        Pickup => vec![
            // A restrained consonant dyad, avoiding a bouncy octave glissando.
            Voice::tone(f, 0., a, d, t * 0.42),
            Voice::tone(f * 1.5, 0.055, a, d * 0.8, t * 0.28),
            Voice::tone(f * 2., 0.055, 0.012, d * 0.3, x * 0.035),
        ],
        Explosion => vec![
            Voice::tone(f.clamp(35., 90.), 0., a, d * 0.55, b * 0.6).sweep(0.6),
            Voice::noise(0., 0.002, 0.028, n * 0.5, cutoff),
            Voice::noise(0.008, 0.016, d, n * 0.95, cutoff * 0.45).air(),
            Voice::noise(0.04, 0.045, d * 0.8, b * 2., 400.).band(85., 0.55),
            Voice::noise(0.16, 0.025, d * 0.3, x * 0.13, 1500.).air(),
        ],
        Jump => vec![
            Voice::tone(f, 0., a, d, t * 0.5).sweep(g.sweep.clamp(-0.4, 0.)),
            Voice::noise(0., 0.015, d * 0.6, n * 0.5, cutoff).air(),
        ],
        Whoosh => vec![
            Voice::noise(0., a, d, n * 0.85, cutoff).air(),
            Voice::noise(0.025, a * 0.9, d * 0.8, b * 1.2, 800.).band(260., 0.5),
        ],
        PowerUp => vec![
            Voice::tone(f * 0.5, 0., a, d, b * 0.32),
            Voice::tone(f, 0., a, d, t * 0.35),
            Voice::tone(f * 1.5, a * 0.3, a * 0.8, d * 0.75, t * 0.18),
            Voice::tone(f * 2., a * 0.6, a * 0.8, d * 0.65, x * 0.08),
            Voice::noise(0., a, d, n * 0.2, cutoff).air(),
        ],
        _ => return crate::cinematic::dry(recipe, sr),
    };
    let duration = voices
        .iter()
        .map(|v| v.at + v.attack + v.decay + 0.015)
        .fold(0f32, f32::max);
    let mut mix = vec![0.; (duration * sr as f32).ceil() as usize];
    for voice in &mut voices {
        voice.cutoff = voice.cutoff.min(cutoff);
        if let Some((hz, q)) = voice.band {
            voice.band = Some((hz.min(cutoff), q * (g.filter.q / 0.707).clamp(0.8, 1.4)));
        }
        let samples = bake_voice(*voice, recipe, sr)?;
        let offset = (voice.at * sr as f32).round() as usize;
        for (out, input) in mix[offset..].iter_mut().zip(samples) {
            *out += input;
        }
    }
    Ok((mix, Vec::new()))
}
