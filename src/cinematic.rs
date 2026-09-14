//! Versioned Foley and environmental scenes. Independently seeded layers
//! share a physical gesture, with moving detail around a mono-compatible body.
mod space;
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

    fn gull_partials(
        &mut self,
        hz: f32,
        at: f32,
        decay: f32,
        level: f32,
        pan: f32,
        sweep: f32,
        harmonic: f32,
    ) {
        let g = &self.recipe.genome;
        let attack = g.envelope.attack_s.clamp(0.005, 0.012);
        let amp = g.tone.amplitude * level;
        let fund = hz.clamp(35., 4000.);
        let nasal = (1800. * (fund / 1100.).sqrt()).clamp(1600., 2200.);
        let second = (fund * 2.007).min(4000.);
        let third = (fund * 2.964).min(4000.);
        let glide = if sweep.abs() > 1e-4 { decay * 0.8 } else { 0. };
        let low = ((fund - 780.) / 180.).clamp(0., 1.);
        self.add(
            Voice::tone(fund, at, attack, decay, amp * low)
                .sweep(sweep)
                .glide(glide),
            pan,
            pan,
        );
        self.add(
            Voice::tone(
                nasal,
                at,
                attack,
                decay * 0.8,
                amp * (0.14 + g.texture * 0.06) * harmonic,
            )
            .sweep(sweep * 0.35)
            .glide(glide)
            .band(nasal, 0.55),
            pan,
            pan,
        );
        self.add(
            Voice::tone(
                second,
                at,
                attack,
                decay * 0.75,
                amp * (0.16 + g.texture * 0.06) * harmonic,
            )
            .sweep(sweep * 0.6)
            .glide(glide)
            .band(second.clamp(1400., 2600.), 0.55),
            pan,
            pan,
        );
        self.add(
            Voice::tone(
                third,
                at,
                attack,
                decay * 0.55,
                amp * (0.05 + g.texture * 0.03) * harmonic,
            )
            .sweep(sweep * 0.4)
            .glide(glide)
            .band(third.clamp(2400., 3800.), 0.5),
            pan,
            pan,
        );
    }

    fn gull_rasp(&mut self, at: f32, decay: f32, level: f32, pan: f32, formants: bool) {
        let g = &self.recipe.genome;
        let n = level * g.noise.gain;
        self.add(
            Voice::noise(at, 0.003, decay, n, 4500.).band(2000., 0.5),
            pan,
            pan,
        );
        self.add(
            Voice::noise(at, 0.002, decay * 0.4, n * 0.32, 5000.).band(3400., 0.55),
            pan,
            pan,
        );
        if formants {
            let mv = self.rng.random_range(0.96..1.04);
            for (hz, gain) in [(1700., 0.32), (2500., 0.2), (3500., 0.12)] {
                self.add(
                    Voice::noise(at, 0.003, decay * 0.7, n * gain, 5000.).band(hz * mv, 0.5),
                    pan,
                    pan,
                );
            }
        }
    }
}

fn cry_gain(t: f32) -> f32 {
    const PTS: [(f32, f32); 11] = [
        (0.00, 0.45),
        (0.08, 0.60),
        (0.14, 0.35),
        (0.18, 0.30),
        (0.22, 1.00),
        (0.30, 0.90),
        (0.38, 0.72),
        (0.44, 0.82),
        (0.52, 0.60),
        (0.60, 0.25),
        (0.66, 0.00),
    ];
    if t <= PTS[0].0 {
        return PTS[0].1;
    }
    if t >= PTS[PTS.len() - 1].0 {
        return 0.;
    }
    for pair in PTS.windows(2) {
        if t <= pair[1].0 {
            let u = (t - pair[0].0) / (pair[1].0 - pair[0].0);
            return pair[0].1 + (pair[1].1 - pair[0].1) * u;
        }
    }
    0.
}

fn tone_tail(t: f32) -> f32 {
    if t < 0.52 {
        1.
    } else if t >= 0.66 {
        0.
    } else {
        (1. - (t - 0.52) / 0.14).powi(2)
    }
}

fn gull_hz(base: f32, ps: f32, drift: f32) -> f32 {
    base * ps * (1. + drift)
}

pub(crate) fn dry(recipe: &Recipe, sr: u32) -> Result<(Vec<f32>, Vec<f32>)> {
    if recipe.kind.is_space() {
        return Ok(space::dry(recipe, sr));
    }
    use SoundKind::*;
    if recipe.version >= 5 {
        match recipe.kind {
            Seagull => return Ok((crate::natural::gull(recipe, sr), Vec::new())),
            CarEngineRumble => return Ok((crate::natural::engine(recipe, sr), Vec::new())),
            _ => {}
        }
    }
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
        CarEngineRumble => {
            // Classic V8 idle: lumpy exhaust burble. No sustained sine drone
            // (that reads as a spaceship) and no long air pad.
            let interval_scale = (80. / f).clamp(0.75, 1.35);
            let gaps = [
                0.092 * interval_scale,
                0.055 * interval_scale,
                0.108 * interval_scale,
                0.05 * interval_scale,
            ];
            let lumps = [1.0, 0.58, 0.86, 0.42];
            let mut t = a.max(0.02);
            let pulses = ((d * 0.92) / 0.075).floor().clamp(18., 36.) as usize;
            for i in 0..pulses {
                let lump = lumps[i % 4];
                let low = 70. * s.rng.random_range(0.92..1.08);
                let mid = 190. * s.rng.random_range(0.88..1.12);
                let thud = f * s.rng.random_range(0.96..1.04);
                s.add(
                    Voice::noise(t, 0.012, 0.07, 1.05 * lump * g.noise.gain, 260.).band(low, 1.05),
                    0.,
                    0.,
                );
                s.add(
                    Voice::noise(t, 0.01, 0.05, (0.4 + x * 0.25) * lump * g.noise.gain, 900.)
                        .band(mid, 0.85),
                    0.,
                    0.,
                );
                s.add(
                    Voice::tone(thud, t, 0.008, 0.045, 0.32 * lump * g.body.gain),
                    0.,
                    0.,
                );
                t += gaps[i % 4] + s.rng.random_range(-0.004..0.004);
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
        Seagull => {
            // One short kya, then a single AAH with two irregular pressure
            // changes. Rasp is concentrated on the main onset and the tail.
            let ts = (d / 0.62).clamp(0.85, 1.15);
            let ps = (f / 1100. * s.rng.random_range(0.94..1.07)).clamp(0.9, 1.12);
            let pan = s.rng.random_range(-0.06..0.06);
            let phrase = [
                (0.00, 1250., 0.14, 950. / 1250. - 1., 1.0, 0.01),
                (0.08, 950., 0.16, 1250. / 950. - 1., 1.0, 0.01),
                (0.22, 1050., 0.24, 1100. / 1050. - 1., 1.0, 0.018),
                (0.44, 900., 0.16, 0.08, 0.7, 0.02),
            ];
            for (at, hz, decay, sweep, harmonic, jitter) in phrase {
                let t = at * ts;
                let drift = s.rng.random_range(-jitter..jitter);
                s.gull_partials(
                    gull_hz(hz, ps, drift),
                    t,
                    decay * ts,
                    cry_gain(at) * tone_tail(at),
                    pan,
                    sweep,
                    harmonic,
                );
            }
            s.gull_rasp(0.00 * ts, 0.055 * ts, 0.55, pan, false);
            s.gull_rasp(0.18 * ts, 0.12 * ts, 1.45, pan, true);
            s.gull_rasp(0.24 * ts, 0.14 * ts, 1.55, pan, true);
            s.gull_rasp(0.32 * ts, 0.1 * ts, 1.15, pan, false);
            s.gull_rasp(0.56 * ts, 0.12 * ts, 0.95, pan, false);
            s.gull_rasp(0.61 * ts, 0.09 * ts, 0.7, pan, false);
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

pub(crate) fn space_bed(recipe: &Recipe, sr: u32, frames: usize) -> (Vec<f32>, Vec<f32>) {
    space::bed(recipe, sr, frames, true)
}
