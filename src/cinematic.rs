//! Versioned Foley and environmental scenes. Independently seeded layers
//! share a physical gesture, with moving detail around a mono-compatible body.
use crate::design::{Voice, bake_voice};
use crate::{Recipe, Result, SoundKind};
use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha8Rng;

struct Layer {
    voice: Voice,
    pan: (f32, f32),
}
struct Scene<'a> {
    recipe: &'a Recipe,
    layers: Vec<Layer>,
    rng: ChaCha8Rng,
}
impl Scene<'_> {
    fn add(&mut self, voice: Voice, from: f32, to: f32) {
        self.layers.push(Layer {
            voice,
            pan: (from, to),
        });
    }
    fn tone(&mut self, ratio: f32, at: f32, decay: f32, level: f32, sweep: f32, pan: f32) {
        let g = &self.recipe.genome;
        self.add(
            Voice::tone(
                g.tone.freq_hz * ratio,
                at,
                g.envelope.attack_s.min(0.012),
                decay,
                level * g.tone.amplitude,
            )
            .sweep(sweep),
            pan,
            pan,
        );
    }
    fn contact(&mut self, at: f32, decay: f32, level: f32, hz: f32, pan: f32) {
        let g = &self.recipe.genome;
        self.add(
            Voice::noise(
                at,
                g.envelope.attack_s.min(0.008),
                decay,
                level * g.noise.gain,
                g.filter.cutoff_hz,
            )
            .band(hz, 0.7),
            pan,
            pan,
        );
    }
    fn air(&mut self, at: f32, attack: f32, decay: f32, level: f32, hz: f32, pan: (f32, f32)) {
        let g = &self.recipe.genome;
        self.add(
            Voice::noise(at, attack, decay, level * g.noise.gain, hz).air(),
            pan.0,
            pan.1,
        );
    }
    fn body(&mut self, at: f32, decay: f32, level: f32) {
        let g = &self.recipe.genome;
        self.add(
            Voice::noise(at, 0.008, decay, level * g.body.gain, 320.)
                .band(g.tone.freq_hz.clamp(45., 150.), 0.65)
                .air(),
            0.,
            0.,
        );
    }
    // Separate excitation for each grain avoids a repeated noise sample or
    // mechanically identical spacing. Fixed seed preserves saved recipes.
    fn grains(&mut self, count: usize, span: f32, decay: f32, hz: f32, level: f32) {
        for i in 0..count {
            let at = (i as f32 + self.rng.random_range(0.05..0.85)) / count as f32 * span;
            let color = hz * self.rng.random_range(0.65..1.45);
            let gain = level * self.rng.random_range(0.45..1.);
            let pan = self.rng.random_range(-0.85..0.85);
            let grain_decay = decay * self.rng.random_range(0.6..1.3);
            self.contact(at, grain_decay, gain, color, pan);
        }
    }

    fn bleeps(&mut self) {
        let d = self.recipe.genome.envelope.decay_s;
        let x = self.recipe.genome.texture;
        for i in 0..9 {
            let ratio = self.rng.random_range(0.7..3.5);
            let pan = self.rng.random_range(-0.8..0.8);
            self.tone(ratio, i as f32 * d * 0.1, d * 0.16, 0.2 + x * 0.2, 0., pan);
        }
    }

    // Broad, overlapping leaf flutter. Events may cluster or leave gaps:
    // unlike contact grains, they have no clocked slots or sharp attacks.
    fn foliage(&mut self, count: usize, span: f32, hz: f32, level: f32) {
        let g = &self.recipe.genome;
        for _ in 0..count {
            let position = self.rng.random_range(0.0..1.0);
            let at = position * span;
            let attack =
                (g.envelope.attack_s * self.rng.random_range(0.4..0.95)).clamp(0.025, 0.12);
            let decay = (span * self.rng.random_range(0.035..0.1)).clamp(0.035, 0.2);
            let color = hz * self.rng.random_range(0.65..1.15);
            let pan = self.rng.random_range(-0.85f32..0.85);
            let drift = (pan + self.rng.random_range(-0.15..0.15)).clamp(-0.9, 0.9);
            let swell = (std::f32::consts::PI * position).sin().powi(2);
            let gain = level * self.rng.random_range(0.4..1.) * (0.25 + 0.75 * swell);
            self.add(
                Voice::noise(at, attack, decay, gain * g.noise.gain, color)
                    .air()
                    .band(color * 0.45, 0.55),
                pan,
                drift,
            );
        }
    }
}

pub(crate) fn dry(recipe: &Recipe, sr: u32) -> Result<(Vec<f32>, Vec<f32>)> {
    use SoundKind::*;
    let g = &recipe.genome;
    let d = g.envelope.decay_s;
    let a = g.envelope.attack_s;
    let x = g.texture;
    let f = g.tone.freq_hz;
    let bend = g.sweep.clamp(-0.7, 1.5);
    let mut s = Scene {
        recipe,
        layers: Vec::new(),
        rng: ChaCha8Rng::seed_from_u64(recipe.seed as u64),
    };
    match recipe.kind {
        Drop => {
            s.air(0., 0.09, d * 0.2, 0.28, 2400., (-0.6, 0.));
            for (at, gain) in [(0.14, 1.), (0.14 + d * 0.34, 0.42), (0.14 + d * 0.54, 0.18)] {
                s.body(at, d * gain, 4. * gain);
                s.contact(at, 0.04, 1.4 * gain, 1700., gain * 0.2);
                s.tone(0.7, at, d * 0.35 * gain, 0.4 * gain, bend, 0.);
            }
        }
        Tap => {
            s.contact(0., 0.025, 1.5, 2400., 0.);
            s.body(0., d, 1.8);
            for (ratio, gain) in [(1., 0.55), (2.71, 0.2), (4.13, x * 0.2)] {
                s.tone(ratio, 0., d / ratio.sqrt(), gain, 0., 0.15);
            }
        }
        Shake => {
            for i in 0..6 {
                let at = i as f32 * d * 0.15;
                let pan = if i % 2 == 0 { -0.7 } else { 0.7 };
                s.air(at, 0.012, d * 0.18, 0.5, 5200., (pan, -pan));
                s.contact(at + 0.02, d * 0.12, 1.6, f * 4., -pan);
            }
            s.grains(15, d, 0.025, 4200., x * 0.9);
        }
        Wiggle => {
            for i in 0..5 {
                let at = i as f32 * d * 0.18;
                let sign = if i % 2 == 0 { -1. } else { 1. };
                s.tone(
                    1. + i as f32 * 0.06,
                    at,
                    d * 0.26,
                    0.5,
                    sign * 0.38 + bend * 0.2,
                    sign * 0.55,
                );
                s.contact(at, d * 0.2, 0.8, f * 3., sign * 0.55);
            }
        }
        Rattle => {
            s.grains(24, d, 0.04, 3100., 1.6);
            if recipe.version < 5 {
                s.bleeps();
            }
            s.body(0., d * 0.5, 0.8);
        }
        Calculator => s.bleeps(),
        Squish => {
            s.body(0., d * 0.65, 2.8);
            s.air(0., a, d * 0.7, 0.9, 1900., (-0.3, 0.3));
            for i in 0..7 {
                let at = i as f32 * d * 0.09;
                s.tone(
                    1. + i as f32 * 0.22,
                    at,
                    d * 0.12,
                    0.35,
                    0.9 + bend * 0.2,
                    (i as f32 - 3.) * 0.17,
                );
                s.contact(at, 0.025, x * 0.8, 1800., 0.);
            }
        }
        Squeeze => {
            s.air(0., a, d * 0.65, 0.8, 1600., (-0.4, 0.4));
            s.tone(1.6, 0., d * 0.75, 0.5, -0.65, -0.2);
            s.tone(2.15, d * 0.2, d * 0.55, x * 0.35, -0.5, 0.3);
            s.body(d * 0.55, d * 0.35, 2.4);
            s.contact(d * 0.55, 0.06, 1.4, 1500., 0.);
            s.tone(1., d * 0.58, d * 0.2, 0.4, 1., 0.2);
        }
        Turn => {
            s.air(0., a, d, 0.5, 2100., (-0.7, 0.7));
            for i in 0..7 {
                let at = i as f32 * d * 0.12;
                s.contact(at, 0.028, 1.2, f * 5., i as f32 * 0.2 - 0.6);
                s.tone(1.8, at, 0.065, 0.16, 0., 0.);
            }
            s.body(d * 0.82, 0.12, 1.8);
        }
        Tear => {
            s.air(0., a, d * 0.8, 0.85, 6500., (-0.75, 0.75));
            s.grains(28, d * 0.85, 0.018, 4600., 1.5);
            s.contact(0., 0.05, 1.4, 900., -0.6);
            s.contact(d * 0.85, 0.045, 1.6, 2300., 0.7);
        }
        Twist => {
            s.air(0., a, d, 0.6, 1800., (-0.6, 0.6));
            for i in 0..4 {
                let at = i as f32 * d * 0.2;
                s.tone(
                    1. + i as f32 * 0.28,
                    at,
                    d * 0.32,
                    0.38,
                    -0.45,
                    i as f32 * 0.3 - 0.45,
                );
                s.contact(at, d * 0.15, 1.1, f * (3. + i as f32), 0.);
            }
            s.contact(d * 0.84, 0.06, 1.6, 2300., 0.5);
            s.body(d * 0.84, d * 0.2, 2.);
        }
        Tighten => {
            for i in 0..12 {
                let progress = i as f32 / 11.;
                let at = d * 0.78 * (1. - (1. - progress).powi(2));
                s.contact(
                    at,
                    0.02,
                    0.6 + progress,
                    1700. + progress * f * 4.,
                    progress - 0.5,
                );
                s.tone(2. + progress, at, 0.035, 0.14 + x * 0.1, 0., 0.);
            }
            s.body(d * 0.84, d * 0.3, 3.);
            s.contact(d * 0.84, 0.05, 1.8, 1100., 0.);
        }
        Poke => {
            s.contact(0., 0.018, 1.6, 2200., 0.);
            s.tone(1., 0.008, d * 0.65, 0.65, bend, 0.);
            s.body(0.008, d * 0.5, 1.8);
            s.contact(d * 0.35, d * 0.15, x * 0.5, 800., 0.25);
        }
        Grab => {
            s.air(0., 0.025, d * 0.3, 0.65, 3800., (-0.65, 0.));
            s.grains(6, d * 0.22, 0.026, 1700., x);
            s.body(d * 0.2, d * 0.7, 3.4);
            s.contact(d * 0.2, 0.045, 1.5, 850., 0.);
            s.tone(0.8, d * 0.2, d * 0.3, 0.35, bend, 0.);
        }
        Ring => {
            s.contact(0., 0.018, 1.6, 3800., 0.);
            // Inharmonic bell modes: a deep hum, prime, tierce and bright rim.
            for (ratio, gain, decay, pan) in [
                (0.5, 0.55, 1., 0.),
                (1., 0.65, 1., 0.),
                (1.19, 0.3, 0.8, -0.4),
                (2.71, 0.32, 0.6, 0.45),
                (4.07, x * 0.3, 0.4, -0.65),
                (5.43, x * 0.18, 0.25, 0.65),
            ] {
                s.tone(ratio, 0., d * decay, gain, 0., pan);
            }
        }
        Droplets => {
            for i in 0..10 {
                let at = i as f32 * d * 0.11 + s.rng.random_range(0.0..0.025);
                let ratio = s.rng.random_range(0.65..1.8);
                let pan = s.rng.random_range(-0.85..0.85);
                s.tone(ratio, at, 0.085, 0.48, -0.55, pan);
                s.tone(ratio * 1.9, at, 0.04, x * 0.2, 0.8, pan);
                s.contact(at, 0.024, 0.8, 3300., pan);
            }
        }
        Rain => {
            for pan in [-0.75, 0., 0.75] {
                s.air(0., a, d, 0.4, 8500., (pan, pan));
            }
            s.grains(48, d, 0.035, 4800., 0.75);
            s.grains(12, d, 0.07, 1200., 0.65);
        }
        Wind if recipe.version >= 5 => {
            // Gentle broad-band air, without the former sub-bass swell or
            // resonant whistle that made a breeze resemble breaking surf.
            s.air(0., a.max(0.3), d, 0.42, 1500., (-0.45, -0.15));
            let at = d * s.rng.random_range(0.25..0.4);
            s.air(at, a.max(0.35), d * 0.7, 0.28 + x * 0.15, 2300., (0.5, 0.2));
        }
        Wind => {
            s.body(0., d, 2.2);
            for (at, pan) in [(0., -0.8), (d * 0.3, 0.7), (d * 0.62, -0.5)] {
                s.air(at, a, d * 0.65, 0.85, 2800., (pan, -pan));
                s.add(
                    Voice::noise(at, a, d * 0.6, x * 0.65, 3500.)
                        .air()
                        .band(f * 4., 1.6),
                    -pan,
                    pan,
                );
            }
        }
        Leaves if recipe.version >= 5 => {
            s.add(
                Voice::noise(0., a.max(0.12), d, 0.24 * g.noise.gain, 2600.)
                    .air()
                    .band(900., 0.55),
                -0.35,
                -0.2,
            );
            s.foliage(24, d, 3800. * (f / 540.).sqrt(), 0.6 + x * 0.5);
        }
        Leaves => {
            s.air(0., a, d, 0.35, 7000., (-0.8, 0.8));
            s.grains(32, d, 0.045, 5500., 1.2);
            s.grains(7, d, 0.018, 1500., x * 0.7);
        }
        Waves => {
            s.body(a * 0.7, d, 3.8);
            s.air(0., a, d, 1.1, 3400., (-0.5, 0.6));
            s.air(a * 0.8, 0.07, d * 0.8, 0.75, 7500., (0.75, -0.7));
            s.air(d * 0.65, 0.2, d * 0.55, 0.45, 5200., (-0.6, 0.6));
            s.grains(22, d * 1.1, 0.04, 4600., x * 0.65);
        }
        Rustling if recipe.version >= 5 => {
            // Overlapping canopy swells carry lighter individual leaves.
            // Motion stays local to each tree instead of flipping left/right.
            for (at, pan, level) in [(0., -0.65, 0.65), (d * 0.32, 0.6, 0.5)] {
                s.add(
                    Voice::noise(at, a.max(0.18), d * 0.8, level * g.noise.gain, 3400.)
                        .air()
                        .band(f * 2.5, 0.55),
                    pan,
                    pan * 0.65,
                );
            }
            // Distant canopy detail is darker than individual nearby leaves.
            s.foliage(42, d * 1.1, 3400. * (f / 360.).sqrt(), 0.65 + x * 0.7);
        }
        Rustling => {
            for i in 0..5 {
                let pan = if i % 2 == 0 { -0.65 } else { 0.65 };
                s.air(i as f32 * d * 0.17, a, d * 0.24, 0.65, 3200., (pan, -pan));
            }
            s.grains(22, d, 0.023, 2400., 0.9);
        }
        Thunder => {
            s.contact(0., 0.07, 2.3, 2400., -0.2);
            s.air(0.015, 0.008, d * 0.35, 1.1, 4800., (-0.7, 0.8));
            s.tone(1., 0.015, d * 0.65, 0.7, bend, 0.);
            for (at, gain, pan) in [
                (0.025, 4.5, -0.7),
                (d * 0.22, 3.5, 0.8),
                (d * 0.48, 2.8, -0.5),
                (d * 0.75, 1.8, 0.5),
            ] {
                s.body(at, d * 0.72, gain);
                s.air(at, 0.08, d * 0.65, gain * 0.18, 650., (pan, -pan));
            }
        }
        Dodge => {
            s.air(0., a, d, 1.3, 5200., (-0.9, 0.9));
            s.body(a, d * 0.6, 2.2);
            s.air(a * 0.8, 0.008, d * 0.24, 0.7, 8000., (-0.5, 0.8));
        }
        Slide => {
            s.air(0., a, d * 0.85, 0.8, 3200., (-0.8, 0.7));
            s.grains(19, d * 0.85, 0.03, 1900., x * 1.1);
            s.tone(1., 0., d * 0.7, 0.2, 0.45, -0.3);
            s.body(d * 0.88, d * 0.25, 3.);
            s.contact(d * 0.88, 0.035, 1.6, 1200., 0.7);
        }
        Swing => {
            s.air(0., a, d, 1.2, 3500., (-0.85, 0.85));
            s.air(a * 0.85, 0.018, d * 0.28, 1., 7800., (-0.6, 0.9));
            s.body(a * 0.75, d * 0.6, 2.8);
            s.tone(1.2, a * 0.65, d * 0.3, 0.3, bend, 0.);
        }
        Stomp => {
            s.body(0., d, 5.5);
            s.tone(0.85, 0., d * 0.65, 0.8, bend, 0.);
            s.contact(0., 0.045, 2., 1100., 0.);
            s.body(0.045, d * 0.5, 2.);
            s.grains(9, d * 0.45, 0.028, 2200., x * 0.8);
        }
        BirdChirps => {
            for i in 0..8 {
                let at = i as f32 * d * 0.13 + s.rng.random_range(0.0..0.035);
                let ratio = s.rng.random_range(0.8..1.35);
                let pan = if i < 4 { -0.6 } else { 0.65 };
                let sweep = if i % 3 == 0 { 0.5 } else { -0.45 };
                s.tone(ratio, at, 0.085 + (i % 3) as f32 * 0.025, 0.6, sweep, pan);
                s.tone(ratio * 2., at, 0.055, x * 0.18, sweep, pan);
                s.air(at, 0.005, 0.045, 0.12, 6500., (pan, pan));
            }
        }
        _ => unreachable!("only cinematic categories use the scene renderer"),
    }
    let duration = s
        .layers
        .iter()
        .map(|l| l.voice.at + l.voice.attack + l.voice.decay + 0.015)
        .fold(0f32, f32::max);
    // Event arrangements remain under six seconds even at maximum user edits.
    let mut mid = vec![0.; (duration * sr as f32).ceil() as usize];
    let mut side = vec![0.; mid.len()];
    for (index, layer) in s.layers.into_iter().enumerate() {
        let mut v = layer.voice;
        v.cutoff = v.cutoff.min(g.filter.cutoff_hz).min(sr as f32 * 0.42);
        if let Some((hz, q)) = v.band {
            v.band = Some((hz.min(v.cutoff), (q * g.filter.q / 0.707).clamp(0.5, 2.)));
        }
        let mut excitation = recipe.clone();
        excitation.seed = recipe
            .seed
            .wrapping_add((index as u32 + 1).wrapping_mul(0x9e3779b9));
        let samples = bake_voice(v, &excitation, sr)?;
        let offset = (v.at * sr as f32).round() as usize;
        let len = samples.len().max(2) - 1;
        for (i, input) in samples.into_iter().enumerate().take(mid.len() - offset) {
            let progress = i as f32 / len as f32;
            let pan = layer.pan.0 + (layer.pan.1 - layer.pan.0) * progress;
            mid[offset + i] += input;
            // Mid + side is left: negative pan therefore adds to side.
            side[offset + i] -= input * pan * 0.65;
        }
    }
    Ok((mid, side))
}
