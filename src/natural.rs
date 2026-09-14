//! Continuous source models for version-five gull calls and combustion idle.
//! Pitch shares one phase across harmonics; exhaust events share a crank clock.
use crate::Recipe;
use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha8Rng;
use std::f32::consts::TAU;

struct Lowpass {
    amount: f32,
    state: f32,
}
impl Lowpass {
    fn new(hz: f32, sr: f32) -> Self {
        Self {
            amount: 1. - (-TAU * hz.min(sr * 0.4) / sr).exp(),
            state: 0.,
        }
    }
    fn tick(&mut self, x: f32) -> f32 {
        self.state += self.amount * (x - self.state);
        self.state
    }
}

fn smooth(x: f32) -> f32 {
    let x = x.clamp(0., 1.);
    x * x * (3. - 2. * x)
}

fn contour(t: f32, points: &[(f32, f32)]) -> f32 {
    for pair in points.windows(2) {
        if t <= pair[1].0 {
            let u = smooth((t - pair[0].0) / (pair[1].0 - pair[0].0));
            return pair[0].1 + (pair[1].1 - pair[0].1) * u;
        }
    }
    points.last().unwrap().1
}

pub(crate) fn gull(recipe: &Recipe, sr: u32) -> Vec<f32> {
    let g = &recipe.genome;
    let rate = sr as f32 * 2.;
    let mut rng = ChaCha8Rng::seed_from_u64(recipe.seed as u64);
    let stretch = (g.envelope.decay_s / 0.62).clamp(0.35, 2.5);
    let pitch = g.tone.freq_hz / 1100. * rng.random_range(0.96..1.04);
    let mut phase = g.tone.phase_offset * TAU;
    let flutter_phase = rng.random_range(0.0..TAU);
    let mut jitter = Lowpass::new(95., rate);
    let mut breath_low = Lowpass::new(3200., rate);
    let mut breath_body = Lowpass::new(850., rate);
    let mut low1 = Lowpass::new(g.filter.cutoff_hz, rate);
    let mut low2 = Lowpass::new(g.filter.cutoff_hz, rate);
    let frames = ((0.84 * stretch + 0.02) * sr as f32).ceil() as usize;
    let mut out = Vec::with_capacity(frames);
    // A small introductory ke followed by an open, falling aaa. A continuous
    // pressure contour avoids retriggered oscillator attacks inside the cry.
    let pressure = [
        (0., 0.),
        (0.025, 0.48),
        (0.095, 0.55),
        (0.16, 0.06),
        (0.205, 0.22),
        (0.265, 1.),
        (0.38, 0.88),
        (0.52, 0.72),
        (0.66, 0.47),
        (0.76, 0.16),
        (0.84, 0.),
    ];
    let frequency = [
        (0., 880.),
        (0.045, 1190.),
        (0.14, 1010.),
        (0.2, 920.),
        (0.27, 1230.),
        (0.39, 1160.),
        (0.54, 1020.),
        (0.69, 870.),
        (0.84, 740.),
    ];
    for i in 0..frames {
        let mut sum = 0.;
        for sub in 0..2 {
            let time = (i * 2 + sub) as f32 / rate;
            let t = time / stretch;
            let random = rng.random_range(-1.0..1.0);
            let drift = jitter.tick(random);
            let flutter =
                (TAU * 47.3 * time + flutter_phase + 0.6 * (TAU * 6.7 * time).sin()).sin();
            let rough = smooth((t - 0.24) / 0.32) * g.texture;
            let hz =
                (contour(t, &frequency) * pitch * (1. + 0.028 * drift + 0.004 * flutter * rough))
                    .min(sr as f32 * 0.18);
            phase = (phase + TAU * hz / rate).rem_euclid(TAU);
            let mut voiced = 0.;
            // Harmonics stay locked to the vibrating source. A broad vocal
            // tract emphasis moves smoothly from a closed to an open beak.
            let formant = 2300. - 550. * smooth((t - 0.25) / 0.5);
            for harmonic in 1..=10 {
                let h = harmonic as f32;
                let partial_hz = hz * h;
                let bandlimit = 1. - smooth((partial_hz / sr as f32 - 0.32) / 0.1);
                let resonance = (-0.5 * ((partial_hz - formant) / 950.).powi(2)).exp();
                let weight = (0.24 + 0.9 * resonance) / h.powf(1.15);
                voiced += (phase * h + 0.12 * h * rough * flutter).sin() * weight * bandlimit;
            }
            let air = breath_low.tick(random);
            let air = air - breath_body.tick(air);
            let envelope = contour(t, &pressure) * smooth(time / g.envelope.attack_s.max(0.004));
            let pressure_flutter = 1. + rough * (0.16 * flutter + 0.22 * drift);
            let sample = envelope
                * pressure_flutter
                * (voiced * g.tone.amplitude * 0.9 + air * g.noise.gain * (0.08 + 0.24 * rough));
            sum += low2.tick(low1.tick(sample));
        }
        out.push(sum * 0.5);
    }
    out
}

pub(crate) fn engine(recipe: &Recipe, sr: u32) -> Vec<f32> {
    let g = &recipe.genome;
    let rate = sr as f32 * 2.;
    let mut rng = ChaCha8Rng::seed_from_u64(recipe.seed as u64);
    let duration = (g.envelope.decay_s + g.envelope.attack_s).clamp(0.2, 2.5);
    let frames = ((duration + 0.08) * sr as f32).ceil() as usize;
    let mut source = vec![0.; frames * 2];
    // Four firings per revolution: 750 RPM yields 50 Hz, not a sequence of
    // isolated 12 Hz thuds. Uneven bank contribution repeats every 720 degrees.
    let firing_hz = (50. * g.tone.freq_hz / 80.).clamp(22., 180.);
    let cylinder = [1., 0.73, 0.88, 0.66, 0.94, 0.79, 0.69, 0.86];
    let mut at = -0.12;
    let mut index = 0;
    let wander_phase = rng.random_range(0.0..TAU);
    while at < duration {
        let strength = cylinder[index % 8] * rng.random_range(0.94..1.06);
        let bank_delay = if index % 4 == 1 || index % 4 == 2 {
            0.0024
        } else {
            0.
        };
        // Each firing excites the same exhaust pipe modes. Overlap preserves
        // the motor's harmonic structure; cycle variation provides roughness.
        let first = ((at + bank_delay) * rate).floor() as isize;
        for j in 0..(0.11 * rate) as usize {
            let dest = first + j as isize;
            if dest < 0 || dest >= source.len() as isize {
                continue;
            }
            let t = j as f32 / rate;
            let onset = smooth(t / 0.0015);
            let exhaust = (TAU * 74. * t).sin() * (-t / 0.029).exp()
                + 0.42 * (TAU * 156. * t).sin() * (-t / 0.018).exp()
                + 0.16 * (TAU * 340. * t).sin() * (-t / 0.009).exp();
            let compression = (TAU * 49. * t).sin() * (-t / 0.025).exp();
            source[dest as usize] += onset
                * strength
                * (exhaust * g.noise.gain * 0.27 + compression * g.body.gain * 0.18);
        }
        let wander = 0.012 * (TAU * 1.3 * at + wander_phase).sin() + 0.005 * (TAU * 3.7 * at).sin();
        at += (1. + wander + rng.random_range(-0.004..0.004)) / firing_hz;
        index += 1;
    }
    let mut mechanical = Lowpass::new(1100., rate);
    let mut mechanical_low = Lowpass::new(180., rate);
    let mut low1 = Lowpass::new(g.filter.cutoff_hz, rate);
    let mut low2 = Lowpass::new(g.filter.cutoff_hz, rate);
    let mut out = Vec::with_capacity(frames);
    for i in 0..frames {
        let mut sum = 0.;
        for sub in 0..2 {
            let n = i * 2 + sub;
            let t = n as f32 / rate;
            let air = mechanical.tick(rng.random_range(-1.0..1.0));
            let air = air - mechanical_low.tick(air);
            let load = 0.65 + 0.35 * (TAU * firing_hz * t).sin().powi(2);
            let noise = air * load * (0.018 + 0.07 * g.texture) * g.noise.gain;
            let env = smooth(t / g.envelope.attack_s.max(0.015))
                * (1. - smooth((t - duration + 0.1) / 0.18));
            sum += low2.tick(low1.tick(source[n] + noise)) * env;
        }
        out.push(sum * 0.5);
    }
    out
}
