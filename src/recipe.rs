use crate::{Error, Result, range};
use rand::Rng;
use serde::{Deserialize, Serialize};
use symbios_audio::{AdsrCurve, AdsrEnvelope, BiquadLowpass, Mix, SineOsc};
use symbios_genetics::Genotype;

/// Semantic categories remain fixed during evolution.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SoundKind {
    UiClick,
    UiHover,
    UiConfirm,
    UiError,
    Footstep,
    Explosion,
    Laser,
    Impact,
    Pickup,
    Jump,
    Whoosh,
    PowerUp,
    Drop,
    Tap,
    Shake,
    Wiggle,
    Rattle,
    Calculator,
    Squish,
    Squeeze,
    Turn,
    Tear,
    Twist,
    Tighten,
    Poke,
    Grab,
    Ring,
    Droplets,
    Rain,
    Wind,
    Leaves,
    Waves,
    Rustling,
    Thunder,
    Dodge,
    Slide,
    Swing,
    Stomp,
    BirdChirps,
}

impl SoundKind {
    pub(crate) fn is_cinematic(self) -> bool {
        !matches!(
            self,
            Self::UiClick
                | Self::UiHover
                | Self::UiConfirm
                | Self::UiError
                | Self::Footstep
                | Self::Explosion
                | Self::Laser
                | Self::Impact
                | Self::Pickup
                | Self::Jump
                | Self::Whoosh
                | Self::PowerUp
        )
    }
}

#[derive(Debug, Serialize)]
pub struct CatalogEntry {
    pub kind: SoundKind,
    pub label: &'static str,
    pub description: &'static str,
}
pub fn catalog() -> Vec<CatalogEntry> {
    use SoundKind::*;
    [
        (
            UiClick,
            "UI click",
            "A precise tactile tap with a bright edge",
        ),
        (
            UiHover,
            "UI hover",
            "A quiet, brief touch cue for frequent hovering",
        ),
        (
            UiConfirm,
            "UI confirm",
            "A warm major phrase resolving with accomplishment",
        ),
        (
            UiError,
            "UI error",
            "A soft descending phrase with a disappointed resolution",
        ),
        (
            Footstep,
            "Footstep",
            "A muted heel and sole contact with a very light surface scuff",
        ),
        (
            Explosion,
            "Explosion",
            "A sharp blast, deep body, and rolling debris",
        ),
        (
            Laser,
            "Laser",
            "A Shoot11-inspired sine-wave pew with a falling pitch and stereo tail",
        ),
        (
            Impact,
            "Impact",
            "One cohesive struck body with a short contact transient",
        ),
        (
            Pickup,
            "Pickup",
            "A restrained consonant chime for a collected reward",
        ),
        (Jump, "Jump", "A springy upward pitch bend"),
        (Whoosh, "Whoosh", "A swelling rush of air with a wide decay"),
        (
            PowerUp,
            "Power up",
            "A rising energy charge with harmonic shimmer",
        ),
        (
            Drop,
            "Drop",
            "A weighty fall, deep landing, and diminishing bounces",
        ),
        (
            Tap,
            "Tap",
            "A crisp hard-surface strike with a resonant body",
        ),
        (
            Shake,
            "Shake",
            "A vigorous back-and-forth cascade of loose contacts",
        ),
        (
            Wiggle,
            "Wiggle",
            "An elastic wobble with alternating rubbery creaks",
        ),
        (
            Rattle,
            "Rattle",
            "Loose rattling contacts with a low body and no electronic tones",
        ),
        (
            Calculator,
            "Calculator",
            "A sequence of bright electronic calculator-like bleeps",
        ),
        (
            Squish,
            "Squish",
            "A wet collapse with viscous bubbles and a heavy body",
        ),
        (
            Squeeze,
            "Squeeze",
            "A rising rubber strain with a compressed wet release",
        ),
        (
            Turn,
            "Turn",
            "A deliberate mechanism rotation with tactile detents",
        ),
        (
            Tear,
            "Tear",
            "A forceful fibrous rip with ragged snapping strands",
        ),
        (
            Twist,
            "Twist",
            "A torsional creak building to a textured release",
        ),
        (
            Tighten,
            "Tighten",
            "An accelerating ratchet that locks under tension",
        ),
        (
            Poke,
            "Poke",
            "A pointed contact with a short elastic indentation",
        ),
        (
            Grab,
            "Grab",
            "A swift grip, cloth friction, and a weighty catch",
        ),
        (
            Ring,
            "Ring",
            "A struck bell with deep inharmonic bronze overtones",
        ),
        (
            Droplets,
            "Droplets",
            "Scattered resonant water beads across a wide space",
        ),
        (
            Rain,
            "Rain",
            "An enveloping shower with individual close splashes",
        ),
        (
            Wind,
            "Wind",
            "A quiet passing breeze with slow, soft movement",
        ),
        (
            Leaves,
            "Leaves",
            "Soft nearby leaves fluttering with light, irregular detail",
        ),
        (
            Waves,
            "Waves",
            "A swelling surf break with deep water and receding foam",
        ),
        (
            Rustling,
            "Rustling",
            "Wind moving through a canopy of trees and rustling leaves",
        ),
        (
            Thunder,
            "Thunder",
            "A sharp sky crack followed by deep rolling thunder",
        ),
        (
            Dodge,
            "Dodge",
            "A fast lateral air cut with a low pressure rush",
        ),
        (
            Slide,
            "Slide",
            "A sustained surface scrape with a weighted stop",
        ),
        (
            Swing,
            "Swing",
            "A broad accelerating arc with a sharp air edge",
        ),
        (
            BirdChirps,
            "Bird chirps",
            "Bright rising and falling bird calls answering across an open space",
        ),
        (
            Stomp,
            "Stomp",
            "A massive heel strike, floor resonance, and settling grit",
        ),
    ]
    .into_iter()
    .map(|(kind, label, description)| CatalogEntry {
        kind,
        label,
        description,
    })
    .collect()
}

/// Editable sound DNA. Frequencies in Hz, envelope times in seconds;
/// width/room/texture/body/noise are dimensionless. See README for bounds.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Genome {
    pub tone: SineOsc,
    pub envelope: AdsrEnvelope,
    pub filter: BiquadLowpass,
    pub noise: Mix,
    pub body: Mix,
    /// Initial pitch offset for older designs; version-3 laser uses a positive
    /// exponential fall rate per second, following the Shoot11 contour.
    pub sweep: f32,
    pub texture: f32,
    pub width: f32,
    pub room: f32,
    pub drive: f32,
}

impl Genome {
    pub fn validate(&self) -> Result<()> {
        for (name, v, lo, hi) in [
            ("tone.freq_hz", self.tone.freq_hz, 35., 4000.),
            ("tone.phase_offset", self.tone.phase_offset, 0., 1.),
            ("tone.amplitude", self.tone.amplitude, 0., 1.),
            ("envelope.attack_s", self.envelope.attack_s, 0.001, 0.5),
            ("envelope.decay_s", self.envelope.decay_s, 0.025, 2.),
            (
                "envelope.sustain_level",
                self.envelope.sustain_level,
                0.,
                0.,
            ),
            ("envelope.release_s", self.envelope.release_s, 0., 0.5),
            ("filter.cutoff_hz", self.filter.cutoff_hz, 150., 16000.),
            ("filter.q", self.filter.q, 0.5, 2.),
            ("noise.gain", self.noise.gain, 0., 1.5),
            ("body.gain", self.body.gain, 0., 1.2),
            ("sweep", self.sweep, -0.85, 3.),
            ("texture", self.texture, 0., 1.),
            ("width", self.width, 0., 1.5),
            ("room", self.room, 0., 1.),
            ("drive", self.drive, 1., 4.),
        ] {
            range(name, v, lo, hi)?;
        }
        Ok(())
    }

    fn constrain(&mut self) {
        self.tone.freq_hz = self.tone.freq_hz.clamp(35., 4000.);
        self.tone.phase_offset = self.tone.phase_offset.clamp(0., 1.);
        self.tone.amplitude = self.tone.amplitude.clamp(0., 1.);
        self.envelope.attack_s = self.envelope.attack_s.clamp(0.001, 0.5);
        self.envelope.decay_s = self.envelope.decay_s.clamp(0.025, 2.);
        self.envelope.sustain_level = 0.;
        self.envelope.release_s = self.envelope.release_s.clamp(0., 0.5);
        self.filter.cutoff_hz = self.filter.cutoff_hz.clamp(150., 16000.);
        self.filter.q = self.filter.q.clamp(0.5, 2.);
        self.noise.gain = self.noise.gain.clamp(0., 1.5);
        self.body.gain = self.body.gain.clamp(0., 1.2);
    }
}

// The same node-level mutation and crossover used by bevy_symbios_audio's
// editor. A serial interactive selection loop avoids its native Rayon pool.
impl Genotype for Genome {
    fn mutate<R: Rng>(&mut self, rng: &mut R, rate: f32) {
        self.tone.mutate(rng, rate);
        self.envelope.mutate(rng, rate);
        self.filter.mutate(rng, rate);
        self.noise.mutate(rng, rate);
        self.body.mutate(rng, rate);
        for (value, step, lo, hi) in [
            (&mut self.sweep, 0.4, -0.85, 3.),
            (&mut self.texture, 0.16, 0., 1.),
            (&mut self.width, 0.18, 0., 1.5),
            (&mut self.room, 0.12, 0., 1.),
            (&mut self.drive, 0.35, 1., 4.),
        ] {
            if rng.random::<f32>() < rate {
                *value = (*value + rng.random_range(-step..=step)).clamp(lo, hi);
            }
        }
        self.constrain();
    }
    fn crossover<R: Rng>(&self, other: &Self, rng: &mut R) -> Self {
        let mut child = Self {
            tone: self.tone.crossover(&other.tone, rng),
            envelope: self.envelope.crossover(&other.envelope, rng),
            filter: self.filter.crossover(&other.filter, rng),
            noise: self.noise.crossover(&other.noise, rng),
            body: self.body.crossover(&other.body, rng),
            sweep: if rng.random() {
                self.sweep
            } else {
                other.sweep
            },
            texture: if rng.random() {
                self.texture
            } else {
                other.texture
            },
            width: if rng.random() {
                self.width
            } else {
                other.width
            },
            room: if rng.random() { self.room } else { other.room },
            drive: if rng.random() {
                self.drive
            } else {
                other.drive
            },
        };
        child.constrain();
        child
    }
}

/// Versioned, portable recipe. Same recipe and rate reproduce a render on a
/// given target; floating-point DSP may differ slightly across architectures.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Recipe {
    pub version: u32,
    pub kind: SoundKind,
    pub seed: u32,
    pub genome: Genome,
}
impl Recipe {
    pub fn new(kind: SoundKind, seed: u32) -> Self {
        use SoundKind::*;
        // pitch, attack, decay, noise, body, sweep, texture, room, cutoff
        let (hz, attack, decay, noise, body, sweep, texture, room, cutoff) = match kind {
            UiClick => (720., 0.002, 0.045, 0.32, 0., 0., 0.03, 0.035, 3200.),
            UiHover => (640., 0.008, 0.028, 0.035, 0., 0., 0., 0., 1800.),
            UiConfirm => (392., 0.008, 0.38, 0., 0.3, 0., 0.12, 0.22, 4200.),
            UiError => (330., 0.014, 0.28, 0., 0.32, 0., 0.08, 0.08, 1800.),
            Footstep => (110., 0.004, 0.15, 0.2, 0.95, 0., 0.02, 0.008, 800.),
            Explosion => (55., 0.002, 1.65, 1.5, 1.1, 1.5, 0.6, 0.65, 4500.),
            Laser => (
                crate::laser::start_hz(),
                0.001,
                crate::laser::decay_seconds(),
                0.,
                0.12,
                crate::laser::slide_rate(),
                0.08,
                0.16,
                9000.,
            ),
            Impact => (105., 0.003, 0.32, 0.7, 1.0, 0., 0.1, 0.22, 2400.),
            Pickup => (660., 0.006, 0.23, 0., 0., 0., 0.1, 0.12, 3600.),
            Jump => (290., 0.012, 0.2, 0.14, 0.12, -0.28, 0.12, 0.06, 2600.),
            Whoosh => (160., 0.16, 0.34, 0.9, 0.22, 0., 0.08, 0.18, 3600.),
            PowerUp => (220., 0.28, 0.75, 0.14, 0.55, 0., 0.18, 0.28, 4200.),
            Drop => (85.0, 0.003, 0.75, 0.8, 1.1, 0.7, 0.45, 0.55, 5200.0),
            Tap => (380.0, 0.002, 0.22, 0.65, 0.7, 0.15, 0.45, 0.38, 6500.0),
            Shake => (240.0, 0.006, 0.8, 0.95, 0.55, 0.0, 0.65, 0.45, 7800.0),
            Wiggle => (190.0, 0.02, 0.65, 0.45, 0.6, -0.3, 0.55, 0.4, 3200.0),
            Rattle => (620.0, 0.002, 1.1, 0.85, 0.65, 0.2, 0.8, 0.55, 8500.0),
            Calculator => (620., 0.002, 1.1, 0., 0., 0., 0.8, 0.16, 8500.),
            Squish => (155.0, 0.012, 0.62, 1.1, 0.85, 0.8, 0.65, 0.4, 4200.0),
            Squeeze => (210.0, 0.14, 0.85, 0.75, 0.75, -0.4, 0.6, 0.45, 4500.0),
            Turn => (160.0, 0.03, 0.85, 0.7, 0.6, 0.0, 0.55, 0.4, 4200.0),
            Tear => (310.0, 0.006, 0.95, 1.15, 0.55, 0.0, 0.8, 0.4, 9500.0),
            Twist => (180.0, 0.06, 0.8, 0.65, 0.75, -0.3, 0.65, 0.48, 4600.0),
            Tighten => (260.0, 0.015, 0.9, 0.85, 0.8, 0.2, 0.65, 0.48, 6500.0),
            Poke => (260.0, 0.002, 0.24, 0.45, 0.65, 0.65, 0.4, 0.3, 3800.0),
            Grab => (135.0, 0.009, 0.45, 0.85, 0.95, 0.3, 0.55, 0.38, 4200.0),
            Ring => (330.0, 0.002, 1.9, 0.3, 0.75, 0.0, 0.7, 0.75, 11000.0),
            Droplets => (720.0, 0.003, 1.05, 0.55, 0.35, 0.8, 0.55, 0.58, 8000.0),
            Rain => (460.0, 0.22, 1.8, 1.2, 0.5, 0.0, 0.65, 0.55, 9500.0),
            Wind => (130., 0.48, 1.85, 0.7, 0., 0., 0.3, 0.08, 2300.),
            Leaves => (540., 0.09, 1.35, 0.9, 0., 0., 0.55, 0.08, 3800.),
            Waves => (75.0, 0.48, 1.85, 1.3, 1.1, 0.0, 0.65, 0.65, 6500.0),
            Rustling => (360., 0.24, 1.8, 1., 0., 0., 0.65, 0.14, 4600.),
            Thunder => (48.0, 0.002, 1.95, 1.4, 1.2, 0.7, 0.75, 0.85, 7500.0),
            Dodge => (140.0, 0.045, 0.38, 1.05, 0.75, -0.3, 0.35, 0.48, 6500.0),
            Slide => (180.0, 0.025, 1.05, 0.95, 0.85, 0.0, 0.65, 0.4, 5800.0),
            Swing => (115.0, 0.19, 0.62, 1.1, 0.9, -0.25, 0.45, 0.55, 6500.0),
            BirdChirps => (2400., 0.003, 1.35, 0.2, 0.1, 0., 0.45, 0.42, 12000.),
            Stomp => (60.0, 0.003, 0.85, 0.95, 1.2, 0.65, 0.55, 0.6, 4200.0),
        };
        Self {
            version: match kind {
                Explosion => 1,
                Footstep | Laser => 3,
                Rattle | Calculator | Wind | Leaves | Rustling => 5,
                UiClick | UiHover | UiConfirm | UiError | Impact | Pickup | Jump | Whoosh
                | PowerUp => 2,
                _ => 4,
            },
            kind,
            seed,
            genome: Genome {
                tone: SineOsc {
                    freq_hz: hz,
                    amplitude: match kind {
                        UiHover => 0.24,
                        Explosion => 0.25,
                        Laser => 0.8,
                        _ => 0.65,
                    },
                    ..Default::default()
                },
                envelope: AdsrEnvelope {
                    attack_s: attack,
                    decay_s: decay,
                    sustain_level: 0.,
                    release_s: 0.03,
                    curve: AdsrCurve::Exponential,
                },
                filter: BiquadLowpass {
                    cutoff_hz: cutoff,
                    q: 0.707,
                },
                noise: Mix { gain: noise },
                body: Mix { gain: body },
                sweep,
                texture,
                width: if kind.is_cinematic() {
                    1.25
                } else if kind == Explosion {
                    0.9
                } else if matches!(kind, UiHover | Footstep) {
                    0.35
                } else {
                    0.8
                },
                room,
                drive: if matches!(kind, Wind | Leaves | Rustling | Calculator) {
                    1.05
                } else if kind.is_cinematic() {
                    1.3
                } else if kind == Explosion {
                    2.2
                } else if kind == Impact {
                    1.35
                } else {
                    1.05
                },
            },
        }
    }
    pub fn validate(&self) -> Result<()> {
        if !matches!(self.version, 1..=5) {
            return Err(Error("unsupported recipe version".into()));
        }
        if self.kind.is_cinematic() && self.version < 4 {
            return Err(Error(
                "cinematic categories require recipe version 4 or later".into(),
            ));
        }
        if self.kind == SoundKind::Calculator && self.version < 5 {
            return Err(Error(
                "calculator requires recipe version 5 or later".into(),
            ));
        }
        self.genome.validate()
    }
    pub fn from_json(json: &str) -> Result<Self> {
        if json.len() > 32_768 {
            return Err(Error("recipe exceeds 32 KiB".into()));
        }
        let recipe: Self = serde_json::from_str(json)?;
        recipe.validate()?;
        Ok(recipe)
    }
    pub fn to_json(&self) -> Result<String> {
        self.validate()?;
        Ok(serde_json::to_string_pretty(self)?)
    }
}
