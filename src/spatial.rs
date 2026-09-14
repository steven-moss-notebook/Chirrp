//! Space theater field. No source synthesis or pitch processing lives
//! here: the direct voice stays intact, with a separate stereo reflection field.
use std::f32::consts::TAU;

struct Lowpass {
    c: f32,
    y: f32,
}
impl Lowpass {
    fn new(hz: f32, sr: u32) -> Self {
        Self {
            c: 1. - (-TAU * hz / sr as f32).exp(),
            y: 0.,
        }
    }
    fn tick(&mut self, x: f32) -> f32 {
        self.y += self.c * (x - self.y);
        self.y
    }
}
struct Delay {
    data: Vec<f32>,
    pos: usize,
}
impl Delay {
    fn new(seconds: f32, sr: u32) -> Self {
        Self {
            data: vec![0.; (seconds * sr as f32).round().max(1.) as usize],
            pos: 0,
        }
    }
    fn read(&self) -> f32 {
        self.data[self.pos]
    }
    fn tap(&self, offset: usize) -> f32 {
        self.data[(self.pos + self.data.len() - offset) % self.data.len()]
    }
    fn push(&mut self, x: f32) {
        self.data[self.pos] = x;
        self.pos = (self.pos + 1) % self.data.len();
    }
    fn diffuse(&mut self, x: f32) -> f32 {
        let y = self.read() - x * 0.63;
        self.push(x + y * 0.63);
        y
    }
}

/// Reflection gains are amplitude gains; output is unmastered wet mid/side.
/// Feedback is an orthogonal Householder transform with damped, bounded gains.
pub(crate) struct Theater {
    early: Delay,
    taps: [[usize; 5]; 2],
    pre: Delay,
    diffusion: [Delay; 4],
    lines: [Delay; 12],
    damp: [Lowpass; 12],
    feedback: [f32; 12],
    low: Lowpass,
    envelope: Lowpass,
    room: f32,
}
impl Theater {
    pub(crate) fn new(sr: u32, room: f32) -> Self {
        // Staggered wall and ceiling paths surround the unchanged direct onset.
        let taps = [
            [0.0131, 0.0277, 0.0473, 0.0739, 0.1123],
            [0.0193, 0.0361, 0.0593, 0.0899, 0.1277],
        ]
        .map(|ts| ts.map(|t| (t * sr as f32).round() as usize));
        let times = [
            0.0371, 0.0437, 0.0533, 0.0613, 0.0719, 0.0839, 0.0973, 0.1097, 0.1279, 0.1393, 0.1511,
            0.1637,
        ];
        let rt60 = 0.65 + room * 3.6;
        Self {
            early: Delay::new(0.14, sr),
            taps,
            pre: Delay::new(0.0271, sr),
            diffusion: [0.0083, 0.0149, 0.0239, 0.0347].map(|t| Delay::new(t, sr)),
            lines: times.map(|t| Delay::new(t, sr)),
            damp: std::array::from_fn(|i| Lowpass::new(3600. - i as f32 * 95., sr)),
            feedback: times.map(|t| 0.001f32.powf(t / rt60)),
            low: Lowpass::new(180., sr),
            envelope: Lowpass::new(12., sr),
            room,
        }
    }
    pub(crate) fn tail(room: f32) -> f32 {
        if room == 0. { 0.035 } else { 0.1 + room * 5. }
    }
    pub(crate) fn tick(&mut self, input: f32) -> (f32, f32) {
        let high = input - self.low.tick(input);
        let env = self.envelope.tick(input.abs());
        let duck = 1. / (1. + env * 1.4);
        let gain = [0.52, 0.37, 0.27, 0.19, 0.13];
        let early = self.taps.map(|taps| {
            taps.iter()
                .zip(gain)
                .map(|(t, g)| self.early.tap(*t) * g)
                .sum::<f32>()
        });
        self.early.push(high);
        let mut x = self.pre.read();
        self.pre.push(high);
        for d in &mut self.diffusion {
            x = d.diffuse(x);
        }
        let reads = self.lines.each_ref().map(|d| d.read());
        let mean = reads.iter().sum::<f32>() / 6.;
        // Distinct injection and pickup vectors distribute energy through all
        // delay paths. Neither output is a delayed copy of the complete voice.
        let injection = [1., -1., 1., 1., -1., 1., -1., -1., 1., -1., 1., 1.];
        for (j, read) in reads.iter().enumerate() {
            let feedback = self.damp[j].tick(*read - mean) * self.feedback[j];
            self.lines[j].push(x * 0.32 * injection[j] + feedback);
        }
        let left = (reads[0] + reads[2] - reads[4] + reads[6] - reads[8] - reads[10]) * 0.55;
        let right = (reads[1] - reads[3] + reads[5] - reads[7] + reads[9] - reads[11]) * 0.55;
        let early_gain = self.room.sqrt() * 0.65 * duck;
        let late_gain = self.room * 1.4 * duck;
        let l = early[0] * early_gain + left * late_gain;
        let r = early[1] * early_gain + right * late_gain;
        ((l + r) * 0.5, (l - r) * 0.5)
    }
}
