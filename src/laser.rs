//! Shoot11-inspired laser. The active Bfxr controls specify a sine oscillator,
//! multiplicative downward pitch slide, brief hold, linear decay, and high-pass.
//! This maps that contour onto Symbios's oscillator plus Chirrp's stereo bus;
//! it is not a general Bfxr patch importer or a bit-identical Bfxr emulator.
use crate::Recipe;
use rand::SeedableRng;
use rand_chacha::ChaCha8Rng;
use symbios_audio::{BakeContext, Node, SineOsc};

pub(crate) const START: f32 = 0.5393915;
pub(crate) const SLIDE: f32 = -0.17901497;
pub(crate) const SUSTAIN: f32 = 0.116307765;
pub(crate) const DECAY: f32 = 0.38184735;
pub(crate) fn start_hz() -> f32 {
    (START * START + 0.001) * 44100. * 8. / 100.
}
pub(crate) fn decay_seconds() -> f32 {
    DECAY * DECAY * 100000. / 44100.
}
pub(crate) fn slide_rate() -> f32 {
    (1. - SLIDE.powi(3) * 0.01).ln() * 44100.
}

pub(crate) fn dry(recipe: &Recipe, sr: u32) -> Vec<f32> {
    let g = &recipe.genome;
    let attack = g.envelope.attack_s;
    let decay = g.envelope.decay_s;
    let hold = SUSTAIN * SUSTAIN / (DECAY * DECAY) * decay;
    let frames = ((attack + hold + decay + 0.012) * sr as f32).ceil() as usize;
    let oversample = 8;
    let internal_sr = sr * oversample;
    let mut rng = ChaCha8Rng::seed_from_u64(recipe.seed as u64);
    let sine = SineOsc {
        freq_hz: 0.,
        amplitude: 1.,
        phase_offset: g.tone.phase_offset,
    };
    let mut phase = sine.init_state().expect("SineOsc provides phase state");
    let mut body_phase = sine.init_state().expect("SineOsc provides phase state");
    let mut overtone_phase = sine.init_state().expect("SineOsc provides phase state");
    let mut out = Vec::with_capacity(frames);
    // Bfxr's high-pass coefficient operates inside its eight sub-samples.
    let hp = (1. - 0.29646936_f32.powi(2) * 0.1).powf(44100. / sr as f32);
    let lp = 1. - (-std::f32::consts::TAU * g.filter.cutoff_hz / internal_sr as f32).exp();
    let (mut previous, mut high, mut low) = (0., 0., 0.);
    for frame in 0..frames {
        let time = frame as f32 / sr as f32;
        let env = if time < attack {
            time / attack
        } else if time < attack + hold {
            1.
        } else {
            (1. - (time - attack - hold) / decay).max(0.)
        };
        let mut sum = 0.;
        for sub in 0..oversample {
            let index = frame as u64 * oversample as u64 + sub as u64;
            let time = index as f32 / internal_sr as f32;
            let frequency = (g.tone.freq_hz * (-g.sweep.max(0.) * time).exp())
                .max(g.tone.freq_hz * 0.2)
                .min(sr as f32 * 0.4);
            let mut sample = 0.;
            for (ratio, level, state) in [
                (1., g.tone.amplitude, &mut phase),
                (0.5, g.body.gain * 0.12, &mut body_phase),
                (2., g.texture * 0.06, &mut overtone_phase),
            ] {
                let inputs = [("freq", frequency * ratio)];
                let mut context = BakeContext::new(
                    internal_sr,
                    index,
                    frames as u64 * oversample as u64,
                    &mut rng,
                    &inputs,
                    Some(state.as_mut()),
                );
                sample += sine.sample(&mut context) * level;
            }
            high = hp * (high + sample - previous);
            previous = sample;
            low += lp * (high - low);
            sum += low;
        }
        out.push(sum / oversample as f32 * env);
    }
    out
}
