use crate::{
    AudioBuffer, AudioMetrics, CatalogEntry, Error, Recipe, Result, SessionSummary, SoundEdits,
    SoundKind, catalog, range, render,
};
use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha8Rng;
use serde::{Deserialize, Serialize};
use symbios_genetics::Genotype;

const MAX_JSON_BYTES: usize = 256 * 1024;
/// A small interactive population. Candidate zero is the unchanged elite after
/// each generation. Selection is explicit; changing a preview does not evolve it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Session {
    version: u32,
    generation: u32,
    selected: usize,
    candidates: Vec<Recipe>,
}
impl Session {
    pub fn new(kind: SoundKind, seed: u32, population: usize) -> Result<Self> {
        if !(2..=12).contains(&population) {
            return Err(Error("population must be in [2, 12]".into()));
        }
        let favorite = Recipe::new(kind, seed);
        let mut rng = ChaCha8Rng::seed_from_u64(seed as u64);
        let mut candidates = vec![favorite.clone()];
        for _ in 1..population {
            candidates.push(offspring(&favorite, &favorite, 0.45, &mut rng));
        }
        Ok(Self {
            version: 1,
            generation: 0,
            selected: 0,
            candidates,
        })
    }
    pub fn candidates(&self) -> &[Recipe] {
        &self.candidates
    }
    pub fn generation(&self) -> u32 {
        self.generation
    }
    pub fn selected(&self) -> usize {
        self.selected
    }
    pub fn favorite(&self) -> &Recipe {
        &self.candidates[self.selected]
    }
    pub fn candidate(&self, index: usize) -> Result<&Recipe> {
        self.candidates
            .get(index)
            .ok_or_else(|| Error("candidate index out of range".into()))
    }
    pub fn select(&mut self, index: usize) -> Result<()> {
        self.candidate(index)?;
        self.selected = index;
        Ok(())
    }
    /// One-button (1 + lambda) evolution around the selected favorite.
    /// Strength is the probability each gene mutates, in [0, 1]. At zero the
    /// generation is an exact copy, including stochastic seeds.
    pub fn randomize(&mut self, strength: f32, seed: u32) -> Result<()> {
        range("strength", strength, 0., 1.)?;
        let next = self
            .generation
            .checked_add(1)
            .ok_or_else(|| Error("generation limit reached".into()))?;
        let favorite = self.favorite().clone();
        let mut rng = ChaCha8Rng::seed_from_u64(seed as u64);
        self.candidates = std::iter::once(favorite.clone())
            .chain(
                (1..self.candidates.len())
                    .map(|_| offspring(&favorite, &favorite, strength, &mut rng)),
            )
            .collect();
        self.selected = 0;
        self.generation = next;
        Ok(())
    }
    /// Rated genetic generation: size-three tournament selection, Symbios
    /// crossover/mutation, and one unchanged elite. Higher ratings win.
    /// Provide a score for every current candidate; scores are not inherited.
    pub fn evolve(&mut self, ratings: &[f32], strength: f32, seed: u32) -> Result<()> {
        range("strength", strength, 0., 1.)?;
        if ratings.len() != self.candidates.len() {
            return Err(Error("one rating is required per candidate".into()));
        }
        for &score in ratings {
            range("rating", score, 0., 1.)?;
        }
        let next = self
            .generation
            .checked_add(1)
            .ok_or_else(|| Error("generation limit reached".into()))?;
        // Stable ties prefer the current favorite.
        let mut elite = self.selected;
        for i in 0..ratings.len() {
            if ratings[i] > ratings[elite] {
                elite = i;
            }
        }
        let mut rng = ChaCha8Rng::seed_from_u64(seed as u64);
        let mut candidates = vec![self.candidates[elite].clone()];
        for _ in 1..self.candidates.len() {
            let a = tournament(ratings, &mut rng);
            let b = tournament(ratings, &mut rng);
            candidates.push(offspring(
                &self.candidates[a],
                &self.candidates[b],
                strength,
                &mut rng,
            ));
        }
        self.candidates = candidates;
        self.selected = 0;
        self.generation = next;
        Ok(())
    }
    /// Replaces the selected recipe after validation. Category changes start a
    /// new session so crossover never mixes incompatible semantic categories.
    pub fn edit(&mut self, recipe: Recipe) -> Result<()> {
        recipe.validate()?;
        if recipe.kind != self.favorite().kind {
            return Err(Error("use create to change category".into()));
        }
        self.candidates[self.selected] = recipe;
        Ok(())
    }
    pub fn render(&self, index: usize, sample_rate: u32) -> Result<AudioBuffer> {
        render(self.candidate(index)?, sample_rate)
    }
    pub fn validate(&self) -> Result<()> {
        if self.version != 1 {
            return Err(Error("unsupported session version".into()));
        }
        if !(2..=12).contains(&self.candidates.len()) {
            return Err(Error("population must be in [2, 12]".into()));
        }
        let kind = self.candidate(self.selected)?.kind;
        for recipe in &self.candidates {
            recipe.validate()?;
            if recipe.kind != kind {
                return Err(Error("all candidates must have the same category".into()));
            }
        }
        Ok(())
    }
    pub fn to_json(&self) -> Result<String> {
        self.validate()?;
        Ok(serde_json::to_string_pretty(self)?)
    }
    pub fn from_json(json: &str) -> Result<Self> {
        check_size(json)?;
        let session: Self = serde_json::from_str(json)?;
        session.validate()?;
        Ok(session)
    }
}

fn offspring(a: &Recipe, b: &Recipe, strength: f32, rng: &mut impl Rng) -> Recipe {
    let mut child = a.clone();
    child.genome = a.genome.crossover(&b.genome, rng);
    child.genome.mutate(rng, strength);
    if !child.kind.is_bed() {
        child.genome.envelope.sustain_level = 0.;
        child.genome.envelope.decay_s = child.genome.envelope.decay_s.min(2.);
    }
    if child.version >= 2 && strength > 0. {
        // Upstream ADSR mutations use absolute time steps suitable for long
        // notes. Keep short UI/foley variations proportional to their parent.
        let g = &mut child.genome;
        let parent = &a.genome;
        g.envelope.attack_s = g.envelope.attack_s.clamp(
            (parent.envelope.attack_s * 0.75).max(0.001),
            (parent.envelope.attack_s * 1.3).min(0.5),
        );
        g.envelope.decay_s = g.envelope.decay_s.clamp(
            (parent.envelope.decay_s * 0.8).max(0.025),
            (parent.envelope.decay_s * 1.25).min(if child.kind.is_bed() { 16. } else { 2. }),
        );
        g.sweep = parent.sweep + (g.sweep - parent.sweep) * 0.25;
        g.texture = parent.texture + (g.texture - parent.texture) * 0.35;
        g.noise.gain = parent.noise.gain + (g.noise.gain - parent.noise.gain) * 0.4;
        g.body.gain = parent.body.gain + (g.body.gain - parent.body.gain) * 0.4;
        if child.kind == SoundKind::UiHover {
            g.envelope.attack_s = g.envelope.attack_s.clamp(0.006, 0.012);
            g.envelope.decay_s = g.envelope.decay_s.clamp(0.025, 0.045);
            g.tone.freq_hz = g.tone.freq_hz.clamp(400., 950.);
            g.room = g.room.min(0.025);
        }
    }
    if strength > 0. {
        child.seed = rng.random();
    }
    child
}
fn tournament(ratings: &[f32], rng: &mut impl Rng) -> usize {
    let mut best = rng.random_range(0..ratings.len());
    for _ in 1..3 {
        let i = rng.random_range(0..ratings.len());
        if ratings[i] > ratings[best] {
            best = i;
        }
    }
    best
}
fn check_size(json: &str) -> Result<()> {
    if json.len() > MAX_JSON_BYTES {
        return Err(Error("JSON exceeds 256 KiB".into()));
    }
    Ok(())
}

/// Transport-neutral commands for tools, web GUIs, or JSON-lines agents.
#[derive(Debug, Deserialize, Serialize)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
pub enum Command {
    Catalog {},
    Create {
        kind: SoundKind,
        seed: u32,
        population: usize,
    },
    Select {
        index: usize,
    },
    Randomize {
        strength: f32,
        seed: u32,
    },
    Evolve {
        ratings: Vec<f32>,
        strength: f32,
        seed: u32,
    },
    Edit {
        recipe: Recipe,
    },
    Snapshot {},
    Restore {
        session: Session,
    },
    Analyze {
        index: usize,
        sample_rate: u32,
    },
}

/// Stateful sound generation and evolution, also usable as a headless Bevy resource.
#[derive(Default)]
#[cfg_attr(feature = "bevy", derive(bevy_ecs::prelude::Resource))]
pub struct Engine {
    session: Option<Session>,
}
impl Engine {
    /// Choose a category and a fresh variation, replacing the active session.
    /// Explicit seeds reproduce both category choice and sound DNA.
    pub fn random_sound(&mut self, seed: u32, population: usize) -> Result<SessionSummary> {
        let mut rng = ChaCha8Rng::seed_from_u64(seed as u64);
        let sounds = catalog();
        let kind = sounds[rng.random_range(0..sounds.len())].kind;
        let mut session = Session::new(kind, rng.random(), population)?;
        session.select(1)?;
        self.session = Some(session);
        self.list_candidates()
    }
    pub fn list_sounds(&self) -> Vec<CatalogEntry> {
        catalog()
    }
    pub fn create_sound(
        &mut self,
        kind: SoundKind,
        seed: u32,
        population: usize,
    ) -> Result<SessionSummary> {
        self.session = Some(Session::new(kind, seed, population)?);
        self.list_candidates()
    }
    pub fn list_candidates(&self) -> Result<SessionSummary> {
        Ok(self.session()?.into())
    }
    pub fn select_candidate(&mut self, index: usize) -> Result<SessionSummary> {
        self.session_mut()?.select(index)?;
        self.list_candidates()
    }
    pub fn randomize(&mut self, strength: f32, seed: u32) -> Result<SessionSummary> {
        self.session_mut()?.randomize(strength, seed)?;
        self.list_candidates()
    }
    pub fn evolve(&mut self, ratings: &[f32], strength: f32, seed: u32) -> Result<SessionSummary> {
        self.session_mut()?.evolve(ratings, strength, seed)?;
        self.list_candidates()
    }
    pub fn edit_sound(&mut self, edits: SoundEdits) -> Result<SessionSummary> {
        let mut recipe = self.session()?.favorite().clone();
        edits.apply(&mut recipe)?;
        self.replace_recipe(recipe)
    }
    pub fn get_recipe(&self, index: usize) -> Result<Recipe> {
        Ok(self.session()?.candidate(index)?.clone())
    }
    pub fn replace_recipe(&mut self, recipe: Recipe) -> Result<SessionSummary> {
        self.session_mut()?.edit(recipe)?;
        self.list_candidates()
    }
    pub fn snapshot(&self) -> Result<Session> {
        Ok(self.session()?.clone())
    }
    pub fn restore_session(&mut self, session: Session) -> Result<SessionSummary> {
        session.validate()?;
        self.session = Some(session);
        self.list_candidates()
    }
    pub fn analyze(&self, index: usize, sample_rate: u32) -> Result<AudioMetrics> {
        Ok(self.render_audio(index, sample_rate)?.metrics())
    }
    pub fn render_audio(&self, index: usize, sample_rate: u32) -> Result<AudioBuffer> {
        self.session()?.render(index, sample_rate)
    }
    pub fn export_wav(&self, index: usize, sample_rate: u32) -> Result<Vec<u8>> {
        Ok(self.render_audio(index, sample_rate)?.wav_bytes())
    }
    pub fn session(&self) -> Result<&Session> {
        self.session
            .as_ref()
            .ok_or_else(|| Error("create or restore a session first".into()))
    }
    fn session_mut(&mut self) -> Result<&mut Session> {
        self.session
            .as_mut()
            .ok_or_else(|| Error("create or restore a session first".into()))
    }
    pub fn execute(&mut self, command: Command) -> Result<serde_json::Value> {
        match command {
            Command::Catalog {} => return Ok(serde_json::to_value(catalog())?),
            Command::Create {
                kind,
                seed,
                population,
            } => {
                self.create_sound(kind, seed, population)?;
            }
            Command::Select { index } => {
                self.select_candidate(index)?;
            }
            Command::Randomize { strength, seed } => {
                self.randomize(strength, seed)?;
            }
            Command::Evolve {
                ratings,
                strength,
                seed,
            } => {
                self.evolve(&ratings, strength, seed)?;
            }
            Command::Edit { recipe } => {
                self.replace_recipe(recipe)?;
            }
            Command::Snapshot {} => {}
            Command::Restore { session } => {
                self.restore_session(session)?;
            }
            Command::Analyze { index, sample_rate } => {
                return Ok(serde_json::to_value(
                    self.session()?.render(index, sample_rate)?.metrics(),
                )?);
            }
        }
        Ok(serde_json::to_value(self.session()?)?)
    }
    /// Legacy command adapter. Prefer individual typed methods and tool definitions.
    /// Always returns a JSON envelope: {ok:true,result:...} or
    /// {ok:false,error:"..."}. Invalid commands leave the session unchanged.
    pub fn invoke(&mut self, json: &str) -> String {
        let result = check_size(json)
            .and_then(|_| Ok(serde_json::from_str::<Command>(json)?))
            .and_then(|cmd| self.execute(cmd));
        let envelope = match result {
            Ok(result) => serde_json::json!({"ok":true,"result":result}),
            Err(error) => serde_json::json!({"ok":false,"error":error.to_string()}),
        };
        envelope.to_string()
    }
}
