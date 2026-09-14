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
    CarEngineRumble,
    Dodge,
    Slide,
    Swing,
    Stomp,
    BirdChirps,
    Seagull,
    PlasmaPulse,
    HeavySlug,
    BeamIgnite,
    BeamLoop,
    ArcZap,
    RocketLaunch,
    EnergyShield,
    Ricochet,
    WeakPoint,
    ExpandingRing,
    GravityDrone,
    HullRumble,
    Thruster,
    VacuumLoop,
    DebrisClatter,
    MagnetPulse,
    FreezeCone,
    IceShatter,
    FurnaceBed,
    AnvilPulse,
    ScrapCreature,
    VoidHowl,
    NaniteHiss,
    SirenLock,
    ChoirInterval,
}

// Serialization marker for the sole supported space renderer.
const SPACE_RECIPE_VERSION: u32 = 8;

impl SoundKind {
    /// Continuous space-bank sources with extended decay and sustain controls.
    pub fn is_bed(self) -> bool {
        matches!(
            self,
            Self::BeamLoop
                | Self::GravityDrone
                | Self::HullRumble
                | Self::VacuumLoop
                | Self::MagnetPulse
                | Self::FurnaceBed
                | Self::NaniteHiss
                | Self::ChoirInterval
        )
    }

    /// Whether this kind belongs to the 25-preset space bank.
    pub fn is_space(self) -> bool {
        matches!(
            self,
            Self::PlasmaPulse
                | Self::HeavySlug
                | Self::BeamIgnite
                | Self::BeamLoop
                | Self::ArcZap
                | Self::RocketLaunch
                | Self::EnergyShield
                | Self::Ricochet
                | Self::WeakPoint
                | Self::ExpandingRing
                | Self::GravityDrone
                | Self::HullRumble
                | Self::Thruster
                | Self::VacuumLoop
                | Self::DebrisClatter
                | Self::MagnetPulse
                | Self::FreezeCone
                | Self::IceShatter
                | Self::FurnaceBed
                | Self::AnvilPulse
                | Self::ScrapCreature
                | Self::VoidHowl
                | Self::NaniteHiss
                | Self::SirenLock
                | Self::ChoirInterval
        )
    }

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
            CarEngineRumble,
            "Car engine rumble",
            "A steady V8 idle with overlapping exhaust pulses and quiet mechanical vibration",
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
            Seagull,
            "Seagull",
            "A harsh nasal kee-yaaah from a lone coastal gull",
        ),
        (
            Stomp,
            "Stomp",
            "A massive heel strike, floor resonance, and settling grit",
        ),
        (
            PlasmaPulse,
            "Plasma pulse",
            "An ionized plasma burst: charging hiss, bright ion core, stereo sizzle, not a kinetic punch",
        ),
        (
            HeavySlug,
            "Heavy slug",
            "A dense magnetic cannon punch with sabot chatter and steel recoil",
        ),
        (
            BeamIgnite,
            "Beam ignite",
            "A plasma blade ignition that rises into the loop's hum and crackle",
        ),
        (
            BeamLoop,
            "Beam loop",
            "A burning plasma blade bed: beating saber hum with electrical crackle",
        ),
        (
            ArcZap,
            "Arc zap",
            "A tesla-coil charge that blooms into branching lightning and residual arc",
        ),
        (
            RocketLaunch,
            "Rocket launch",
            "A wide motor ignition and bay roar with steel harmonics, not a high broom sweep",
        ),
        (
            EnergyShield,
            "Energy shield",
            "A rising plasma field wall with electrical shimmer, not a kinetic hit",
        ),
        (
            Ricochet,
            "Ricochet",
            "A multi-bounce steel deflection that travels across the stereo field",
        ),
        (
            WeakPoint,
            "Weak point",
            "A heavy structural collision that blooms into a reward fracture",
        ),
        (
            ExpandingRing,
            "Expanding ring",
            "A radially swelling shock front with a low pressure wake",
        ),
        (
            GravityDrone,
            "Gravity drone",
            "A gravitational well: beating sub fundamentals and a strained orbit tone",
        ),
        (
            HullRumble,
            "Hull rumble",
            "Ship structure: driven steel plate modes, hull flex, and irregular knocks",
        ),
        (
            Thruster,
            "Thruster",
            "A fast plasma exhaust shove with hot grit and a falling motor core",
        ),
        (
            VacuumLoop,
            "Vacuum loop",
            "A hollow Helmholtz cavity with slow suction, almost no sub rumble",
        ),
        (
            DebrisClatter,
            "Debris clatter",
            "Asteroid rockfall: dull stone impacts, dust, and pebble cascade, not ringing hull plates",
        ),
        (
            MagnetPulse,
            "Magnet pulse",
            "A 1 Hz coil charge cycle with harmonic buzz and falling pole slaps",
        ),
        (
            FreezeCone,
            "Freeze cone",
            "A cryo particle stream that seizes into a freeze crack, no rising arpeggio",
        ),
        (
            IceShatter,
            "Ice shatter",
            "An ice-comet smash that cascades into a storm of short crystal shards",
        ),
        (
            FurnaceBed,
            "Furnace bed",
            "A thermal roar of brown combustion, moving mid formant, and irregular embers",
        ),
        (
            AnvilPulse,
            "Anvil pulse",
            "A forged anvil strike: heavy steel body and long ringing plate",
        ),
        (
            ScrapCreature,
            "Scrap creature",
            "A living scrap beast: throat growl, junk gait, and a hostile ram",
        ),
        (
            VoidHowl,
            "Void howl",
            "A massive alien inhale through detuned throat formants, then inward collapse",
        ),
        (
            NaniteHiss,
            "Nanite hiss",
            "A circling nanite swarm: machine grains, close flybys, and metallic micro-contacts",
        ),
        (
            SirenLock,
            "Siren lock",
            "A tough-enemy target lock: converging scan tones that slam into a lock strike",
        ),
        (
            ChoirInterval,
            "Choir interval",
            "A dense voiced hymn: stacked throats without a shared noise rumble",
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
            ("envelope.decay_s", self.envelope.decay_s, 0.025, 16.),
            (
                "envelope.sustain_level",
                self.envelope.sustain_level,
                0.,
                1.,
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
        self.envelope.decay_s = self.envelope.decay_s.clamp(0.025, 16.);
        self.envelope.sustain_level = self.envelope.sustain_level.clamp(0., 1.);
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

/// Portable recipe. Version is a serialization detail; space kinds always use
/// their current design. Same recipe and rate reproduce a render on a
/// given target; floating-point DSP may differ slightly across architectures.
#[derive(Debug, Clone, PartialEq)]
pub struct Recipe {
    pub version: u32,
    pub kind: SoundKind,
    pub seed: u32,
    pub genome: Genome,
}
impl Serialize for Recipe {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error> {
        use serde::ser::SerializeStruct;
        let mut fields = serializer.serialize_struct("Recipe", 4)?;
        fields.serialize_field("version", &self.serialization_version())?;
        fields.serialize_field("kind", &self.kind)?;
        fields.serialize_field("seed", &self.seed)?;
        fields.serialize_field("genome", &self.genome)?;
        fields.end()
    }
}
// Normalize at the serde boundary so sessions, asset requests, MCP, and WASM
// all load the same current space recipe without a separate migration API.
impl<'de> Deserialize<'de> for Recipe {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> std::result::Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            version: u32,
            kind: SoundKind,
            seed: u32,
            genome: Genome,
        }
        let fields = Fields::deserialize(deserializer)?;
        let mut recipe = Self {
            version: fields.version,
            kind: fields.kind,
            seed: fields.seed,
            genome: fields.genome,
        };
        recipe.version = recipe.serialization_version();
        Ok(recipe)
    }
}

impl Recipe {
    fn serialization_version(&self) -> u32 {
        if self.kind.is_space() && (1..=SPACE_RECIPE_VERSION).contains(&self.version) {
            SPACE_RECIPE_VERSION
        } else {
            self.version
        }
    }

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
            CarEngineRumble => (80.0, 0.04, 1.7, 1.05, 0.45, 0.0, 0.4, 0.04, 1400.0),
            Dodge => (140.0, 0.045, 0.38, 1.05, 0.75, -0.3, 0.35, 0.48, 6500.0),
            Slide => (180.0, 0.025, 1.05, 0.95, 0.85, 0.0, 0.65, 0.4, 5800.0),
            Swing => (115.0, 0.19, 0.62, 1.1, 0.9, -0.25, 0.45, 0.55, 6500.0),
            BirdChirps => (2400., 0.003, 1.35, 0.2, 0.1, 0., 0.45, 0.42, 12000.),
            Seagull => (1100., 0.008, 0.62, 0.48, 0.0, 0.7, 0.7, 0.08, 4500.),
            PlasmaPulse => (320.0, 0.008, 0.7, 1.05, 0.38, 0.0, 0.75, 0.38, 12000.0),
            HeavySlug => (52.0, 0.003, 1.3, 1.2, 1.2, 0.0, 0.5, 0.4, 9500.0),
            BeamIgnite => (92.0, 0.03, 1.45, 1.05, 0.4, 0.0, 0.65, 0.26, 11000.0),
            BeamLoop => (92.0, 0.08, 4.0, 1.05, 0.35, 0.0, 0.75, 0.22, 10500.0),
            ArcZap => (58.0, 0.08, 1.55, 0.85, 0.85, 0.0, 0.65, 0.32, 10000.0),
            RocketLaunch => (48.0, 0.04, 1.85, 1.15, 1.2, 0.0, 0.55, 0.58, 7200.0),
            EnergyShield => (128.0, 0.12, 1.5, 0.85, 0.35, 0.0, 0.55, 0.42, 9500.0),
            Ricochet => (540.0, 0.002, 0.78, 0.85, 0.4, 0.0, 0.55, 0.52, 9500.0),
            WeakPoint => (72.0, 0.002, 1.05, 0.9, 1.15, 0.0, 0.65, 0.5, 10500.0),
            ExpandingRing => (48.0, 0.28, 1.8, 1.2, 1.15, 0.0, 0.6, 0.48, 9500.0),
            GravityDrone => (35.0, 0.4, 6.0, 0.22, 1.15, 0.0, 0.25, 0.5, 750.0),
            HullRumble => (55.0, 0.25, 6.0, 0.55, 1.15, 0.0, 0.7, 0.38, 2600.0),
            Thruster => (65.0, 0.015, 0.95, 1.2, 1.05, 0.0, 0.6, 0.3, 9000.0),
            VacuumLoop => (95.0, 0.3, 4.0, 1.15, 0.2, 0.0, 0.35, 0.22, 3200.0),
            DebrisClatter => (52.0, 0.002, 1.85, 1.15, 1.05, 0.0, 0.65, 0.4, 3800.0),
            MagnetPulse => (48.0, 0.06, 4.0, 0.4, 1.05, 0.0, 0.85, 0.28, 4200.0),
            FreezeCone => (420.0, 0.2, 1.2, 1.1, 0.75, 0.0, 0.7, 0.4, 11500.0),
            IceShatter => (140.0, 0.002, 1.25, 0.95, 0.9, 0.0, 0.55, 0.4, 9000.0),
            FurnaceBed => (46.0, 0.4, 6.0, 1.35, 0.75, 0.0, 0.8, 0.32, 5200.0),
            AnvilPulse => (85.0, 0.002, 1.6, 1.1, 1.2, 0.0, 0.65, 0.46, 10000.0),
            ScrapCreature => (62.0, 0.05, 1.75, 1.05, 1.15, 0.0, 0.72, 0.42, 8000.0),
            VoidHowl => (55.0, 0.25, 1.9, 1.1, 1.1, 0.0, 0.6, 0.55, 6500.0),
            NaniteHiss => (780.0, 0.15, 4.0, 1.25, 0.18, 0.0, 0.9, 0.28, 9000.0),
            SirenLock => (155.0, 0.05, 1.35, 0.7, 0.9, 0.0, 0.5, 0.4, 7500.0),
            ChoirInterval => (87.0, 0.45, 6.0, 0.18, 0.35, 0.0, 0.2, 0.62, 6000.0),
            Stomp => (60.0, 0.003, 0.85, 0.95, 1.2, 0.65, 0.55, 0.6, 4200.0),
        };
        Self {
            version: match kind {
                k if k.is_space() => SPACE_RECIPE_VERSION,
                Explosion => 1,
                Footstep | Laser => 3,
                Rattle | Calculator | Wind | Leaves | Rustling | Seagull | CarEngineRumble => 5,
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
                        Seagull => 0.7,
                        _ => 0.65,
                    },
                    ..Default::default()
                },
                envelope: AdsrEnvelope {
                    attack_s: attack,
                    decay_s: decay,
                    sustain_level: if kind.is_bed() { 0.72 } else { 0. },
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
                width: if matches!(kind, Ricochet | RocketLaunch) {
                    1.5
                } else if matches!(kind, IceShatter | WeakPoint | PlasmaPulse) {
                    1.43
                } else if kind.is_space() {
                    1.25
                } else if kind == CarEngineRumble {
                    0.28
                } else if kind == Seagull {
                    0.18
                } else if kind.is_cinematic() {
                    1.25
                } else if kind == Explosion {
                    0.9
                } else if matches!(kind, UiHover | Footstep) {
                    0.35
                } else {
                    0.8
                },
                room: if kind.is_space() {
                    0.25 + room * 0.8
                } else {
                    room
                },
                drive: if kind.is_space() {
                    1.12
                } else if matches!(kind, Wind | Leaves | Rustling | Calculator) {
                    1.05
                } else if kind == CarEngineRumble {
                    1.2
                } else if kind == Seagull {
                    1.15
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
        if !matches!(self.version, 1..=8) {
            return Err(Error("unsupported recipe version".into()));
        }
        if !self.kind.is_space() && self.kind.is_cinematic() && self.version < 4 {
            return Err(Error(
                "cinematic categories require recipe version 4 or later".into(),
            ));
        }
        if self.kind == SoundKind::Calculator && self.version < 5 {
            return Err(Error(
                "calculator requires recipe version 5 or later".into(),
            ));
        }
        if !self.kind.is_bed() {
            range("envelope.decay_s", self.genome.envelope.decay_s, 0.025, 2.)?;
            range(
                "envelope.sustain_level",
                self.genome.envelope.sustain_level,
                0.,
                0.,
            )?;
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
