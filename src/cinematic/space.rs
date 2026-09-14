//! Space voices. Broadband excitation, damped material modes,
//! coherent pressure gestures and continuously moving formants replace bleeps.
//! Source layers run at 2x rate and are low-passed before decimation.
use crate::{Recipe, SoundKind};
use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha8Rng;
use std::f32::consts::TAU;

#[derive(Clone, Copy)]
struct Lowpass {
    c: f32,
    y: f32,
}
impl Lowpass {
    fn new(hz: f32, sr: f32) -> Self {
        Self {
            c: 1. - (-TAU * hz.min(sr * 0.4) / sr).exp(),
            y: 0.,
        }
    }
    fn tick(&mut self, x: f32) -> f32 {
        self.y += self.c * (x - self.y);
        self.y
    }
}
struct Band {
    b: f32,
    a1: f32,
    a2: f32,
    x1: f32,
    x2: f32,
    y1: f32,
    y2: f32,
}
impl Band {
    fn new(hz: f32, q: f32, sr: f32) -> Self {
        let w = TAU * hz.clamp(20., sr * 0.38) / sr;
        let alpha = w.sin() / (2. * q);
        Self {
            b: alpha / (1. + alpha),
            a1: -2. * w.cos() / (1. + alpha),
            a2: (1. - alpha) / (1. + alpha),
            x1: 0.,
            x2: 0.,
            y1: 0.,
            y2: 0.,
        }
    }
    fn tick(&mut self, x: f32) -> f32 {
        let y = self.b * (x - self.x2) - self.a1 * self.y1 - self.a2 * self.y2;
        self.x2 = self.x1;
        self.x1 = x;
        self.y2 = self.y1;
        self.y1 = y;
        y
    }
}
fn smooth(x: f32) -> f32 {
    let x = x.clamp(0., 1.);
    x * x * (3. - 2. * x)
}
fn envelope(t: f32, attack: f32, hold: f32, tail: f32) -> f32 {
    if t < 0. {
        return 0.;
    }
    let release = (t - attack - hold).max(0.);
    smooth(t / attack.max(0.001)) * (-6.9 * release / tail.max(0.01)).exp()
}

#[derive(Clone, Copy)]
enum Material {
    Steel,
    Crystal,
    Electrical,
    Stone,
}
#[derive(Clone, Copy)]
enum Source {
    Air { low: f32, high: f32, flutter: f32 },
    Modes { base: f32, material: Material },
    Pressure { hz: f32 },
    Throat { hz: f32, choir: bool },
    Blade { hz: f32, sweep: f32 },
    Spark { density: f32 },
}
struct Layer {
    source: Source,
    at: f32,
    attack: f32,
    hold: f32,
    tail: f32,
    gain: f32,
    pan: f32,
    motion: f32,
}
struct Scene {
    layers: Vec<Layer>,
    stretch: f32,
    onset: f32,
    pitch: f32,
    noise: f32,
    body: f32,
    tone: f32,
}
impl Scene {
    fn new(r: &Recipe) -> Self {
        let reference = Recipe::new(r.kind, r.seed);
        Self {
            layers: vec![],
            stretch: (r.genome.envelope.decay_s / reference.genome.envelope.decay_s)
                .clamp(0.12, 2.2),
            onset: (r.genome.envelope.attack_s / reference.genome.envelope.attack_s)
                .clamp(0.25, 3.),
            pitch: (r.genome.tone.freq_hz / reference.genome.tone.freq_hz).clamp(0.35, 3.),
            noise: r.genome.noise.gain,
            body: r.genome.body.gain,
            tone: r.genome.tone.amplitude,
        }
    }
    fn add(&mut self, source: Source, timing: (f32, f32, f32, f32), gain: f32, pan: f32) {
        let (at, attack, hold, tail) = timing;
        self.layers.push(Layer {
            source,
            at: at * self.stretch,
            attack: attack * self.stretch * self.onset,
            hold: hold * self.stretch,
            tail: tail * self.stretch,
            gain,
            pan,
            motion: 0.,
        });
    }
    fn band(
        &mut self,
        timing: (f32, f32, f32, f32),
        band: (f32, f32),
        gain: f32,
        pan: f32,
        flutter: f32,
        motion: f32,
    ) {
        let (at, attack, hold, tail) = timing;
        self.add(
            Source::Air {
                low: band.0 * self.pitch.sqrt(),
                high: band.1 * self.pitch.sqrt(),
                flutter,
            },
            (at, attack, hold, tail),
            gain * self.noise,
            pan,
        );
        self.layers.last_mut().unwrap().motion = motion;
    }
    fn air(&mut self, timing: (f32, f32, f32, f32), band: (f32, f32), gain: f32, pan: f32) {
        self.band(timing, band, gain, pan, 0.035, 0.);
    }
    fn wake(&mut self, timing: (f32, f32, f32, f32), band: (f32, f32), gain: f32, pan: f32) {
        self.band(timing, band, gain, pan, 0.16, 0.12);
    }
    fn metal(&mut self, at: f32, tail: f32, hz: f32, gain: f32, pan: f32, material: Material) {
        self.add(
            Source::Modes {
                base: hz * self.pitch,
                material,
            },
            (at, 0.002, 0., tail),
            gain * self.tone,
            pan,
        );
    }
    fn pressure(&mut self, at: f32, hold: f32, tail: f32, hz: f32, gain: f32) {
        self.add(
            Source::Pressure {
                hz: hz * self.pitch.sqrt(),
            },
            (at, 0.008, hold, tail),
            gain * self.body,
            0.,
        );
    }
    fn blade(&mut self, timing: (f32, f32, f32, f32), hz: f32, sweep: f32, gain: f32, pan: f32) {
        self.add(
            Source::Blade {
                hz: hz * self.pitch,
                sweep,
            },
            timing,
            gain * self.tone,
            pan,
        );
    }
    fn spark(&mut self, timing: (f32, f32, f32, f32), density: f32, gain: f32, pan: f32) {
        self.add(Source::Spark { density }, timing, gain * self.noise, pan);
    }
}

pub(super) fn dry(r: &Recipe, sr: u32) -> (Vec<f32>, Vec<f32>) {
    if r.kind.is_bed() {
        let e = &r.genome.envelope;
        let frames = ((e.attack_s + e.decay_s + e.release_s) * sr as f32).ceil() as usize;
        return bed(r, sr, frames, false);
    }
    use Material::*;
    use SoundKind::*;
    let mut s = Scene::new(r);
    let mut rng = ChaCha8Rng::seed_from_u64(r.seed as u64 ^ 0x7381_ae91);
    match r.kind {
        PlasmaPulse => {
            s.blade((0., 0.08, 0.02, 0.05), 420., 0.62, 0.7, 0.);
            s.air((0., 0.05, 0.04, 0.08), (3000., 11000.), 0.45, 0.);
            s.spark((0.04, 0.02, 0.04, 0.08), 20., 0.35, 0.);
            s.blade((0.07, 0.003, 0.08, 0.35), 380., 0.05, 1.15, 0.);
            s.spark((0.07, 0.001, 0.06, 0.28), 40., 0.85, 0.);
            s.air((0.068, 0.006, 0.1, 0.42), (2800., 12000.), 0.9, 0.55);
            s.air((0.072, 0.008, 0.1, 0.4), (2200., 10000.), 0.75, -0.55);
            s.metal(0.07, 0.45, 280., 0.28, 0., Electrical);
            s.metal(0.08, 0.35, 520., 0.16, 0.35, Electrical);
            s.metal(0.09, 0.28, 740., 0.1, -0.32, Electrical);
            s.pressure(0.07, 0.04, 0.32, 72., 0.35);
        }
        HeavySlug => {
            s.blade((0., 0.035, 0.02, 0.08), 46., 0.22, 0.28, 0.);
            s.air((0.045, 0.001, 0.004, 0.04), (160., 3800.), 0.45, 0.);
            s.pressure(0.048, 0.06, 0.85, 32., 1.15);
            s.metal(0.05, 1.25, 64., 0.72, 0., Steel);
            s.metal(0.07, 0.55, 128., 0.28, -0.3, Steel);
            s.metal(0.12, 0.4, 210., 0.14, 0.35, Steel);
        }
        BeamIgnite => {
            s.spark((0., 0.001, 0.012, 0.07), 48., 0.85, 0.);
            s.air((0., 0.001, 0.015, 0.1), (2200., 10000.), 0.4, 0.);
            s.blade((0.02, 0.12, 0.62, 0.55), 92., 0.52, 1.12, 0.);
            s.spark((0.14, 0.02, 0.55, 0.4), 18., 0.42, 0.15);
            s.metal(0.1, 0.18, 184., 0.1, 0., Electrical);
        }
        ArcZap => {
            s.blade((0., 0.18, 0.45, 1.05), 58., 0.3, 0.78, 0.);
            s.pressure(0.16, 0.06, 0.95, 38., 0.82);
            s.spark((0.18, 0.002, 0.05, 0.7), 36., 1.15, 0.);
            s.air((0.18, 0.02, 0.25, 1.0), (700., 7500.), 0.4, 0.);
            for _ in 0..8 {
                let at = rng.random_range(0.2..1.05);
                s.spark(
                    (at, 0.001, 0.008, 0.12),
                    55.,
                    rng.random_range(0.25..0.6),
                    rng.random_range(-0.55..0.55),
                );
                s.metal(
                    at,
                    0.14,
                    rng.random_range(180.0..480.),
                    0.08,
                    0.,
                    Electrical,
                );
            }
        }
        RocketLaunch => {
            s.spark((0., 0.001, 0.012, 0.07), 28., 0.7, 0.);
            s.pressure(0.018, 0.22, 1.9, 28., 1.25);
            s.air((0.02, 0.06, 0.5, 1.7), (24., 380.), 1.7, 0.);
            s.air((0.05, 0.08, 0.45, 1.55), (280., 2200.), 0.85, -0.72);
            s.layers.last_mut().unwrap().motion = 0.22;
            s.air((0.08, 0.09, 0.42, 1.5), (320., 2400.), 0.85, 0.72);
            s.layers.last_mut().unwrap().motion = 0.22;
            s.spark((0.07, 0.04, 0.45, 1.15), 9., 0.32, -0.58);
            s.spark((0.11, 0.04, 0.45, 1.15), 9., 0.32, 0.58);
            s.metal(0.012, 0.7, 62., 0.28, 0., Steel);
            s.metal(0.05, 0.6, 210., 0.2, -0.62, Steel);
            s.metal(0.08, 0.55, 280., 0.18, 0.64, Steel);
        }
        EnergyShield => {
            s.blade((0., 0.14, 0.22, 1.15), 128., 0.42, 0.92, 0.);
            s.metal(0.05, 1.25, 192., 0.2, -0.55, Electrical);
            s.metal(0.09, 1.1, 288., 0.14, 0.55, Electrical);
            s.air((0.08, 0.1, 0.18, 1.05), (2800., 9200.), 0.42, 0.65);
            s.air((0.1, 0.12, 0.16, 1.0), (2200., 8400.), 0.42, -0.65);
            s.pressure(0.06, 0.03, 0.28, 72., 0.16);
        }
        Ricochet => {
            s.spark((0., 0.001, 0.004, 0.04), 26., 0.75, -0.72);
            s.metal(0.001, 0.22, 760., 0.45, -0.65, Steel);
            s.air((0.004, 0.005, 0.02, 0.14), (1100., 6500.), 0.42, -0.4);
            s.layers.last_mut().unwrap().motion = 0.4;
            s.metal(0.032, 0.26, 510., 0.34, 0.12, Steel);
            s.spark((0.034, 0.001, 0.008, 0.06), 18., 0.5, 0.28);
            s.metal(0.068, 0.36, 340., 0.26, 0.72, Steel);
            s.air((0.07, 0.01, 0., 0.22), (700., 4200.), 0.3, 0.58);
            s.layers.last_mut().unwrap().motion = 0.22;
            s.pressure(0., 0., 0.14, 62., 0.28);
        }
        WeakPoint => {
            s.pressure(0., 0.04, 0.95, 30., 1.2);
            s.metal(0., 1.15, 58., 0.7, 0., Steel);
            s.spark((0., 0.001, 0.015, 0.1), 40., 0.85, 0.);
            s.air((0.008, 0.006, 0.05, 0.4), (280., 2400.), 0.5, 0.);
            s.blade((0.03, 0.018, 0.12, 0.75), 220., 0.18, 0.62, 0.);
            s.metal(0.04, 1.05, 420., 0.32, 0.38, Crystal);
            s.metal(0.06, 0.95, 640., 0.22, -0.35, Electrical);
            s.metal(0.09, 0.7, 880., 0.1, 0.22, Electrical);
        }
        ExpandingRing => {
            s.pressure(0.1, 0.22, 1.85, 28., 1.15);
            s.metal(0.12, 1.35, 58., 0.32, 0., Electrical);
            s.wake((0.08, 0.22, 0.05, 1.2), (30., 700.), 0.7, 0.);
        }
        Thruster => {
            s.spark((0., 0.002, 0.015, 0.08), 24., 0.45, 0.);
            s.pressure(0.006, 0.06, 0.7, 42., 0.9);
            s.wake((0.008, 0.018, 0.14, 0.8), (50., 2400.), 1.45, 0.);
            s.air((0.03, 0.02, 0.08, 0.55), (1400., 6500.), 0.22, -0.25);
        }
        DebrisClatter => {
            s.pressure(0., 0.05, 0.55, 26., 0.95);
            s.air((0., 0.01, 0.06, 0.4), (35., 380.), 0.8, 0.);
            for i in 0..7 {
                let at = if i == 0 {
                    0.012
                } else {
                    rng.random_range(0.08..1.35)
                };
                let pan = rng.random_range(-0.62..0.62);
                let fall = (-at * 0.55f32).exp();
                s.metal(
                    at,
                    rng.random_range(0.16..0.42),
                    rng.random_range(36.0..82.),
                    0.4 * fall,
                    pan,
                    Stone,
                );
                s.air((at, 0.008, 0.03, 0.2), (70., 850.), 0.34 * fall, pan);
            }
            for _ in 0..8 {
                let at = rng.random_range(0.14..1.55);
                let pan = rng.random_range(-0.72..0.72);
                s.metal(
                    at,
                    rng.random_range(0.07..0.16),
                    rng.random_range(90.0..170.),
                    0.12 * (-at * 0.5).exp(),
                    pan,
                    Stone,
                );
                s.air(
                    (at, 0.004, 0., 0.1),
                    (180., 1400.),
                    0.16 * (-at * 0.45).exp(),
                    pan,
                );
            }
        }
        IceShatter => {
            s.pressure(0., 0.035, 0.75, 28., 1.15);
            s.air((0., 0.001, 0.025, 0.18), (700., 8500.), 0.85, 0.);
            s.metal(0., 1.05, 92., 0.7, 0., Crystal);
            s.metal(0.01, 0.8, 148., 0.38, -0.22, Crystal);
            s.metal(0.018, 0.65, 210., 0.22, 0.28, Crystal);
            for i in 0..12 {
                let at = i as f32 * 0.012;
                let pan = rng.random_range(-0.75..0.75);
                s.metal(
                    at,
                    rng.random_range(0.12..0.32),
                    rng.random_range(160.0..420.),
                    0.2 * (-at * 2.2).exp(),
                    pan,
                    Crystal,
                );
            }
            for _ in 0..40 {
                let at = rng.random_range(0.02..0.95);
                let pan = rng.random_range(-0.85..0.85);
                s.spark(
                    (at, 0.001, 0., rng.random_range(0.02..0.07)),
                    rng.random_range(36.0..70.),
                    0.22 * (-at * 1.15).exp(),
                    pan,
                );
                if rng.random::<f32>() < 0.35 {
                    s.metal(
                        at,
                        rng.random_range(0.03..0.09),
                        rng.random_range(380.0..980.),
                        0.08 * (-at * 1.2).exp(),
                        pan,
                        Crystal,
                    );
                }
            }
        }
        FreezeCone => {
            s.air((0., 0.04, 0.12, 0.55), (2600., 12000.), 0.7, 0.);
            s.pressure(0.18, 0.02, 0.4, 58., 0.45);
            s.spark((0.18, 0.002, 0.03, 0.16), 50., 0.8, 0.);
            for _ in 0..10 {
                let at = rng.random_range(0.05..0.55);
                s.spark(
                    (at, 0.001, 0., 0.04),
                    60.,
                    0.28 * (-at).exp(),
                    rng.random_range(-0.7..0.7),
                );
            }
        }
        AnvilPulse => {
            s.air((0., 0.001, 0., 0.02), (180., 2800.), 0.32, 0.);
            s.pressure(0.002, 0.05, 1.05, 30., 1.05);
            s.metal(0.001, 1.85, 78., 1.05, 0., Steel);
            s.metal(0.028, 1.25, 148., 0.32, -0.22, Steel);
            s.metal(0.06, 0.7, 236., 0.14, 0.2, Steel);
        }
        ScrapCreature => {
            s.add(
                Source::Throat {
                    hz: 58. * s.pitch.sqrt(),
                    choir: false,
                },
                (0., 0.07, 0.38, 0.9),
                0.78 * s.tone,
                0.,
            );
            s.add(
                Source::Throat {
                    hz: 86. * s.pitch.sqrt(),
                    choir: false,
                },
                (0.06, 0.08, 0.22, 0.7),
                0.32 * s.tone,
                0.18,
            );
            s.air((0., 0.05, 0.22, 0.75), (55., 780.), 0.5, 0.);
            for i in 0..4 {
                let at = 0.05 + i as f32 * 0.12 + rng.random_range(0.0..0.04);
                let pan = if i % 2 == 0 { -0.42 } else { 0.42 };
                s.metal(
                    at,
                    rng.random_range(0.16..0.35),
                    rng.random_range(48.0..130.),
                    0.34,
                    pan,
                    Steel,
                );
                s.spark((at, 0.001, 0., 0.045), 16., 0.22, pan * 0.85);
            }
            for _ in 0..8 {
                let at = rng.random_range(0.08..0.68);
                s.metal(
                    at,
                    0.1,
                    rng.random_range(170.0..430.),
                    0.1,
                    rng.random_range(-0.65..0.65),
                    Steel,
                );
            }
            s.pressure(0.58, 0.05, 1.1, 30., 1.2);
            s.metal(0.58, 1.15, 46., 0.72, 0., Steel);
            s.air((0.58, 0.008, 0.05, 0.42), (45., 650.), 0.5, 0.);
            s.spark((0.58, 0.002, 0., 0.08), 18., 0.32, 0.);
        }
        VoidHowl => {
            s.add(
                Source::Throat {
                    hz: 34. * s.pitch.sqrt(),
                    choir: false,
                },
                (0., 0.28, 0.45, 1.55),
                0.95 * s.tone,
                0.,
            );
            s.air((0.1, 0.35, 0.15, 1.1), (40., 520.), 0.45, 0.);
            s.pressure(1.05, 0., 0.7, 28., 0.85);
        }
        SirenLock => {
            s.blade((0., 0.12, 0.28, 0.16), 210., 0.22, 0.38, -0.55);
            s.blade((0.1, 0.14, 0.24, 0.14), 142., 0.18, 0.42, 0.55);
            s.air((0.06, 0.1, 0.28, 0.28), (700., 2800.), 0.24, 0.);
            s.pressure(0.46, 0.05, 0.65, 40., 0.95);
            s.metal(0.46, 0.95, 72., 0.5, 0., Steel);
            s.spark((0.46, 0.001, 0., 0.07), 24., 0.45, 0.);
            s.metal(0.52, 0.55, 118., 0.22, 0., Electrical);
            s.blade((0.48, 0.02, 0.12, 0.45), 88., 0.08, 0.35, 0.);
        }
        _ => unreachable!("continuous space kinds are handled above"),
    }
    render_scene(r, sr, s)
}

fn render_scene(r: &Recipe, sr: u32, scene: Scene) -> (Vec<f32>, Vec<f32>) {
    let rate = sr as f32 * 2.;
    let seconds = scene
        .layers
        .iter()
        .map(|l| l.at + l.attack + l.hold + l.tail)
        .fold(0f32, f32::max);
    let frames = (seconds * sr as f32).ceil() as usize + 1;
    let mut mid = vec![0.; frames];
    let mut side = vec![0.; frames];
    for (index, l) in scene.layers.iter().enumerate() {
        let mut rng = ChaCha8Rng::seed_from_u64(r.seed as u64 ^ ((index as u64 + 1) * 0x9e3779b9));
        let mut source = Voice::new(l.source, l.tail, rate, &mut rng);
        let cutoff = r.genome.filter.cutoff_hz.min(sr as f32 * 0.38);
        let mut lp = [Lowpass::new(cutoff, rate); 4];
        let offset = (l.at * sr as f32).round() as usize;
        let length = ((l.attack + l.hold + l.tail) * sr as f32).ceil() as usize;
        let drift = rng.random_range(0.0..TAU);
        for i in 0..length.min(frames - offset) {
            let mut sum = 0.;
            for sub in 0..2 {
                let t = (i * 2 + sub) as f32 / rate;
                let mut x = source.tick(t, &mut rng) * envelope(t, l.attack, l.hold, l.tail);
                for f in &mut lp {
                    x = f.tick(x);
                }
                sum += x * 0.5;
            }
            let v = sum * l.gain;
            mid[offset + i] += v;
            let t = i as f32 / sr as f32;
            let pan = (l.pan + l.motion * (t * 2.1 + drift).sin()).clamp(-0.8, 0.8);
            side[offset + i] -= v * pan * 0.45;
        }
    }
    (mid, side)
}

struct Mode {
    phase: f64,
    step: f64,
    amp: f32,
    decay: f32,
}
struct Voice {
    source: Source,
    rate: f32,
    low: Lowpass,
    high: Lowpass,
    bands: Vec<Band>,
    modes: Vec<Mode>,
    phase: f64,
    phase2: f64,
    spark: f32,
    drift: f32,
    weights: [f32; 24],
}
impl Voice {
    fn new(source: Source, tail: f32, rate: f32, rng: &mut ChaCha8Rng) -> Self {
        let (lo, hi) = match source {
            Source::Air { low, high, .. } => (low, high),
            Source::Pressure { .. } => (22., 140.),
            Source::Blade { .. } => (2800., 9500.),
            Source::Spark { .. } => (2200., 12000.),
            Source::Modes {
                material: Material::Stone,
                ..
            } => (45., 1300.),
            _ => (100., 4500.),
        };
        let mut modes = vec![];
        let mut bands = vec![];
        if let Source::Modes { base, material } = source {
            for i in 0..22 {
                let ratio = (1. + i as f32).powf(match material {
                    Material::Steel => 1.13,
                    Material::Crystal => 1.26,
                    Material::Electrical => 0.93,
                    Material::Stone => 1.4,
                });
                let hz = base * ratio * rng.random_range(0.91..1.09);
                if hz > rate * 0.19 {
                    continue;
                }
                let lifetime = tail * rng.random_range(0.35..0.9) / (1. + i as f32 * 0.045)
                    * match material {
                        Material::Crystal => 0.45,
                        Material::Stone => 0.28,
                        _ => 1.,
                    };
                modes.push(Mode {
                    phase: rng.random_range(0.0..TAU) as f64,
                    step: (TAU * hz / rate) as f64,
                    amp: 0.45 / (1. + i as f32 * 0.35),
                    decay: (-6.9 / (lifetime * rate).max(1.)).exp(),
                });
            }
        }
        if let Source::Throat { choir, .. } = source {
            for hz in if choir {
                [560., 1050., 2450.]
            } else {
                [260., 720., 1450.]
            } {
                bands.push(Band::new(hz, if choir { 2.5 } else { 3.5 }, rate));
            }
        }
        Self {
            source,
            rate,
            low: Lowpass::new(lo, rate),
            high: Lowpass::new(hi, rate),
            bands,
            modes,
            phase: 0.,
            phase2: rng.random_range(0.0..TAU) as f64,
            spark: 0.,
            drift: rng.random_range(0.0..TAU),
            weights: std::array::from_fn(|i| {
                1. / ((i + 1) as f32).powf(
                    if matches!(source, Source::Throat { choir: true, .. }) {
                        1.15
                    } else {
                        0.85
                    },
                )
            }),
        }
    }
    fn tick(&mut self, t: f32, rng: &mut ChaCha8Rng) -> f32 {
        let white = rng.random_range(-1.0..1.0) * (self.rate / 48000.).sqrt();
        match self.source {
            Source::Air { flutter, .. } => {
                let noise = self.high.tick(white) - self.low.tick(white);
                noise
                    * (0.86 + flutter * (TAU * 37.1 * t + self.drift + 1.3 * (t * 7.).sin()).sin())
            }
            Source::Pressure { hz } => {
                self.phase +=
                    (TAU * hz.clamp(28., 110.) * (1. + 0.06 * (-t * 22.).exp()) / self.rate) as f64;
                let sub = self.phase.sin() as f32 * 0.85;
                let air = self.high.tick(white) - self.low.tick(white);
                sub + air * 1.15
            }
            Source::Blade { hz, sweep } => {
                let f = (hz * (1. - sweep * (-t * 8.5).exp())).clamp(28., self.rate * 0.18);
                self.phase += (TAU * f / self.rate) as f64;
                self.phase2 += (TAU * f * 1.046 / self.rate) as f64;
                let buzz = |p: f64| {
                    let (sin, cos) = p.sin_cos();
                    let s = sin as f32;
                    s + 0.4 * (2. * cos as f32 * s) + 0.16 * (3. * s - 4. * s.powi(3))
                };
                let core = buzz(self.phase) * 0.58 + buzz(self.phase2) * 0.42;
                let hiss = (self.high.tick(white) - self.low.tick(white)) * 0.2;
                core * 0.9 + hiss
            }
            Source::Spark { density } => {
                if rng.random::<f32>() < density / self.rate {
                    self.spark = rng.random_range(0.55..1.45);
                }
                self.spark *= (-90. / self.rate).exp();
                (self.high.tick(white) - self.low.tick(white)) * self.spark * 3.6
            }
            Source::Modes { material, .. } => {
                let mut sum = 0.;
                for mode in &mut self.modes {
                    sum += mode.phase.sin() as f32 * mode.amp;
                    mode.phase += mode.step;
                    mode.amp *= mode.decay;
                }
                let exciter = self.high.tick(white) - self.low.tick(white);
                let noise = exciter
                    * (-t
                        * match material {
                            Material::Stone => 32.,
                            _ => 120.,
                        })
                    .exp();
                sum * 0.6 + noise * 0.7
            }
            Source::Throat { hz, .. } => {
                self.phase += (TAU * hz * (1. + 0.004 * (TAU * 4.7 * t + self.drift).sin())
                    / self.rate) as f64;
                let mut voiced = 0.;
                let (sin, cos) = self.phase.sin_cos();
                let (mut previous, mut harmonic) = (0., sin as f32);
                for h in 1..=24 {
                    if hz * h as f32 > self.rate * 0.19 {
                        break;
                    }
                    voiced += harmonic * self.weights[h - 1];
                    let next = 2. * cos as f32 * harmonic - previous;
                    previous = harmonic;
                    harmonic = next;
                }
                let mut formant = 0.;
                for b in &mut self.bands {
                    formant += b.tick(voiced * 0.5 + white * 0.12);
                }
                formant * 0.9 + voiced * 0.1 + self.low.tick(white) * 0.35
            }
        }
    }
}

enum BedSynth {
    Beam {
        a: Voice,
        b: Voice,
        spark: Voice,
        hiss: Band,
        hiss_s: Band,
    },
    Gravity {
        osc: [f64; 4],
        mud: Lowpass,
        drift: f32,
    },
    Hull {
        plates: Vec<Band>,
        edge: Vec<Band>,
        flex: Lowpass,
        knock: f32,
    },
    Vacuum {
        helm: Band,
        helm2: Band,
        flow: Band,
        flow_s: Band,
        drift: f32,
    },
    Magnet {
        coil: [f64; 3],
        slap: f32,
        prev: f32,
        tick: Band,
        spark: Voice,
        drift: f32,
    },
    Furnace {
        low: Lowpass,
        mid: Lowpass,
        formant: Band,
        ember: Voice,
        brown: f32,
        drift: f32,
    },
    Nanite {
        dark: Lowpass,
        cloud: Band,
        shell: Band,
        near: Band,
        grit: Voice,
        grit2: Voice,
        flutter: [f64; 6],
        drift: f32,
    },
    Choir {
        voices: Vec<(Voice, f32)>,
    },
}

impl BedSynth {
    fn new(
        kind: SoundKind,
        g: &crate::Genome,
        pitch: f32,
        rate: f32,
        rng: &mut ChaCha8Rng,
        drift: f32,
    ) -> Self {
        use SoundKind::*;
        match kind {
            BeamLoop => Self::Beam {
                a: Voice::new(
                    Source::Blade {
                        hz: g.tone.freq_hz * pitch,
                        sweep: 0.,
                    },
                    16.,
                    rate,
                    rng,
                ),
                b: Voice::new(
                    Source::Blade {
                        hz: g.tone.freq_hz * pitch * 1.046,
                        sweep: 0.,
                    },
                    16.,
                    rate,
                    rng,
                ),
                spark: Voice::new(Source::Spark { density: 18. }, 16., rate, rng),
                hiss: Band::new(4200. * pitch.sqrt(), 0.9, rate),
                hiss_s: Band::new(5100. * pitch.sqrt(), 0.8, rate),
            },
            GravityDrone => Self::Gravity {
                osc: [0.; 4],
                mud: Lowpass::new(48. * pitch.sqrt(), rate),
                drift,
            },
            HullRumble => Self::Hull {
                plates: [55., 71., 96., 128., 173., 231.]
                    .map(|f| Band::new(f * pitch, 8.5, rate))
                    .into(),
                edge: [310., 418.].map(|f| Band::new(f * pitch, 6.5, rate)).into(),
                flex: Lowpass::new(52. * pitch.sqrt(), rate),
                knock: 0.,
            },
            VacuumLoop => Self::Vacuum {
                helm: Band::new(88. * pitch.sqrt(), 5.5, rate),
                helm2: Band::new(132. * pitch.sqrt(), 4.2, rate),
                flow: Band::new(420. * pitch.sqrt(), 1.1, rate),
                flow_s: Band::new(980. * pitch.sqrt(), 0.9, rate),
                drift,
            },
            MagnetPulse => Self::Magnet {
                coil: [0.; 3],
                slap: 0.,
                prev: 0.,
                tick: Band::new(780. * pitch.sqrt(), 3.5, rate),
                spark: Voice::new(Source::Spark { density: 10. }, 16., rate, rng),
                drift,
            },
            FurnaceBed => Self::Furnace {
                low: Lowpass::new(70. * pitch.sqrt(), rate),
                mid: Lowpass::new(280. * pitch.sqrt(), rate),
                formant: Band::new(310. * pitch.sqrt(), 1.15, rate),
                ember: Voice::new(Source::Spark { density: 9. }, 16., rate, rng),
                brown: 0.,
                drift,
            },
            NaniteHiss => Self::Nanite {
                dark: Lowpass::new(900. * pitch.sqrt(), rate),
                cloud: Band::new(780. * pitch.sqrt(), 2.2, rate),
                shell: Band::new(2400. * pitch.sqrt(), 3.4, rate),
                near: Band::new(1650. * pitch.sqrt(), 4.2, rate),
                grit: Voice::new(Source::Spark { density: 38. }, 16., rate, rng),
                grit2: Voice::new(Source::Spark { density: 14. }, 16., rate, rng),
                flutter: [0.; 6],
                drift,
            },
            ChoirInterval => Self::Choir {
                voices: [
                    (1., -0.45),
                    (1.004, 0.45),
                    (1.498, -0.3),
                    (1.505, 0.35),
                    (0.501, 0.),
                ]
                .into_iter()
                .map(|(ratio, pan)| {
                    (
                        Voice::new(
                            Source::Throat {
                                hz: g.tone.freq_hz * ratio,
                                choir: true,
                            },
                            16.,
                            rate,
                            rng,
                        ),
                        pan,
                    )
                })
                .collect(),
            },
            _ => unreachable!("only space beds construct BedSynth"),
        }
    }

    fn tick(
        &mut self,
        t: f32,
        w: f32,
        ws: f32,
        rng: &mut ChaCha8Rng,
        g: &crate::Genome,
        pitch: f32,
        rate: f32,
    ) -> (f32, f32) {
        let n = g.noise.gain;
        let body = g.body.gain;
        let tone = g.tone.amplitude;
        let x = g.texture;
        match self {
            Self::Beam {
                a,
                b,
                spark,
                hiss,
                hiss_s,
            } => {
                let blade = a.tick(t, rng) * 0.7 + b.tick(t, rng) * 0.55;
                let crackle = spark.tick(t, rng);
                (
                    tone * blade * 0.9 + n * (hiss.tick(w) * 0.35 + crackle * 0.7),
                    n * (hiss_s.tick(ws) * 0.12 + crackle * 0.08),
                )
            }
            Self::Gravity { osc, mud, drift } => {
                for (j, f) in [31.2, 32.55, 15.55, 148.].iter().enumerate() {
                    let wobble = if j == 3 {
                        1. + 0.012 * (TAU * 0.07 * t + *drift).sin()
                    } else {
                        1.
                    };
                    osc[j] += (TAU * f * pitch * wobble / rate) as f64;
                }
                let well = osc[0].sin() as f32 * 0.55 + osc[1].sin() as f32 * 0.5;
                let mill = 0.78 + 0.22 * (TAU * 0.11 * t + *drift).sin();
                let strain = osc[3].sin() as f32 * 0.07;
                (
                    body * (well * mill + osc[2].sin() as f32 * 0.4)
                        + n * mud.tick(w) * 0.08
                        + strain * tone,
                    strain * 0.12,
                )
            }
            Self::Hull {
                plates,
                edge,
                flex,
                knock,
            } => {
                if rng.random::<f32>() < 1.35 / rate {
                    *knock = rng.random_range(0.5..1.3);
                }
                *knock *= (-14. / rate).exp();
                let flexed = flex.tick(w);
                let drive = flexed * 0.35 + *knock * 3.2;
                let mut plates_sum = 0.;
                for (i, p) in plates.iter_mut().enumerate() {
                    plates_sum += p.tick(drive) / (1. + i as f32 * 0.28);
                }
                let mut edge_sum = 0.;
                for (i, p) in edge.iter_mut().enumerate() {
                    edge_sum += p.tick(w * 0.15 + *knock) / (1. + i as f32);
                }
                (
                    body * flexed * 0.85 + tone * plates_sum * 1.6 + n * edge_sum * x * 0.7,
                    n * edge_sum * 0.18,
                )
            }
            Self::Vacuum {
                helm,
                helm2,
                flow,
                flow_s,
                drift,
            } => {
                let suction = 0.72 + 0.28 * (TAU * 0.21 * t + *drift).sin();
                let cavity = helm.tick(w) * 1.6 + helm2.tick(w) * 1.1;
                let air = flow.tick(w);
                (
                    n * (cavity * suction + air * 0.45) + body * cavity * 0.12,
                    n * flow_s.tick(ws) * 0.16,
                )
            }
            Self::Magnet {
                coil,
                slap,
                prev,
                tick,
                spark,
                drift,
            } => {
                let charge = 0.2 + 0.8 * (0.5 + 0.5 * (TAU * t + *drift).sin()).powi(2);
                if *prev - charge > 0.04 {
                    *slap = 1.;
                }
                *prev = charge;
                *slap *= (-22. / rate).exp();
                for (j, f) in [48., 96., 144.].iter().enumerate() {
                    coil[j] += (TAU * f * pitch / rate) as f64;
                }
                let buzz = coil[0].sin() as f32 * 0.55
                    + coil[1].sin() as f32 * 0.28
                    + coil[2].sin() as f32 * 0.12;
                let click = tick.tick(w) * *slap + spark.tick(t, rng) * *slap;
                (
                    tone * buzz * charge * 0.85
                        + body * coil[0].sin() as f32 * charge * 0.35
                        + click * 0.9,
                    n * (coil[2].sin() as f32 * charge * 0.1 + click * 0.12),
                )
            }
            Self::Furnace {
                low,
                mid,
                formant,
                ember,
                brown,
                drift,
            } => {
                *brown += 0.018 * (w - *brown);
                let thermal = 0.92 + 0.08 * (TAU * 0.06 * t + *drift).sin();
                let roar = low.tick(*brown) * 2.1 + mid.tick(w) * 0.45 + formant.tick(w) * 0.35;
                let pop = ember.tick(t, rng);
                (
                    n * roar * thermal + x * pop * 0.85 + body * *brown * 0.45,
                    n * (ws * 0.05 + pop * x * 0.12),
                )
            }
            Self::Nanite {
                dark,
                cloud,
                shell,
                near,
                grit,
                grit2,
                flutter,
                drift,
            } => {
                for (j, f) in [7., 14., 29., 73., 0.37, 1.12].iter().enumerate() {
                    flutter[j] += (TAU * f / rate) as f64;
                }
                let surround = 0.5 + 0.5 * (flutter[4].sin() as f32 + 0.12 * drift.sin());
                let cluster = (0.5 + 0.5 * flutter[5].sin() as f32).powi(5);
                let swarm = 0.42
                    + 0.2 * surround
                    + 0.1 * flutter[0].sin() as f32
                    + 0.08 * flutter[1].sin() as f32
                    + 0.08 * flutter[2].sin() as f32
                    + 0.06 * flutter[3].sin() as f32
                    + 0.28 * cluster;
                let hiss = (w - dark.tick(w)) * 0.14;
                let shell_m = shell.tick(w);
                let near_m = near.tick(w);
                let machines =
                    cloud.tick(w) * 0.8 + shell_m * 0.62 + near_m * (0.28 + 0.7 * cluster);
                let grains = grit.tick(t, rng) * 0.55 + grit2.tick(t, rng) * (0.22 + 0.7 * cluster);
                (
                    n * (machines + hiss + grains) * swarm,
                    n * (shell_m * 0.2 + near_m * 0.16 + grains * 0.2) * (surround * 2. - 1.),
                )
            }
            Self::Choir { voices } => {
                let (mut voice, mut spread) = (0., 0.);
                for (v, pan) in voices {
                    let a = v.tick(t, rng);
                    voice += a;
                    spread += a * *pan;
                }
                (voice * tone * 0.85, spread * tone * 0.32)
            }
        }
    }
}

pub(super) fn bed(r: &Recipe, sr: u32, frames: usize, continuous: bool) -> (Vec<f32>, Vec<f32>) {
    let g = &r.genome;
    let reference = Recipe::new(r.kind, r.seed);
    let pitch = (g.tone.freq_hz / reference.genome.tone.freq_hz).clamp(0.4, 2.5);
    let rate = sr as f32 * 2.;
    let mut rng = ChaCha8Rng::seed_from_u64(r.seed as u64 ^ 0xad73_8819);
    let drift = rng.random_range(0.0..TAU);
    let mut synth = BedSynth::new(r.kind, g, pitch, rate, &mut rng, drift);
    let mut lp_mid = [Lowpass::new(g.filter.cutoff_hz.min(sr as f32 * 0.38), rate); 4];
    let mut lp_side = lp_mid;
    let mut mid = Vec::with_capacity(frames);
    let mut side = Vec::with_capacity(frames);
    for i in 0..frames {
        let (mut m, mut s) = (0., 0.);
        for sub in 0..2 {
            let t = (i * 2 + sub) as f32 / rate;
            let w = rng.random_range(-1.0..1.0) * (rate / 48000.).sqrt();
            let ws = rng.random_range(-1.0..1.0) * (rate / 48000.).sqrt();
            let (mut vm, mut vs) = synth.tick(t, w, ws, &mut rng, g, pitch, rate);
            for f in &mut lp_mid {
                vm = f.tick(vm);
            }
            for f in &mut lp_side {
                vs = f.tick(vs);
            }
            m += vm * 0.5;
            s += vs * 0.5;
        }
        let t = i as f32 / sr as f32;
        let e = &g.envelope;
        let env = if continuous {
            e.sustain_level
        } else {
            let decay = e.sustain_level
                + (1. - e.sustain_level) * (-5. * ((t - e.attack_s) / e.decay_s).max(0.)).exp();
            let release = if e.release_s > 0. {
                smooth((frames - 1 - i) as f32 / (e.release_s * sr as f32))
            } else {
                1.
            };
            smooth(t / e.attack_s) * decay * release
        };
        mid.push(m * env);
        side.push(s * env);
    }
    (mid, side)
}
