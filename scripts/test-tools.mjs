import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import init, { Chirrp } from '../wasm/pkg/chirrp.js';
import { createChirrpTools } from '../wasm/tools.js';

await init({ module_or_path: await readFile(new URL('../wasm/pkg/chirrp_bg.wasm', import.meta.url)) });
const engine = new Chirrp();
// The new integration must never fall back to the legacy dispatcher.
engine.invoke = () => { throw new Error('Legacy invoke used'); };
const tools = createChirrpTools(engine);
assert.equal(Object.keys(tools).length, 15);
assert.equal(tools.list_sounds.execute().length, 66);
assert.throws(() => tools.list_candidates.execute());
const summary = tools.create_sound.execute({ kind: 'laser' });
assert.equal(summary.candidates.length, 6);
assert.equal(summary.candidates[0].genome, undefined);
const original = tools.get_recipe.execute({ index: 0 });
tools.edit_sound.execute({ pitch_hz: 440, stereo_width: 1.2 });
const favorite = tools.get_recipe.execute({ index: 0 });
assert.equal(favorite.genome.tone.freq_hz, 440);
assert.equal(favorite.genome.width, 1.2);
assert.equal(favorite.genome.room, original.genome.room);
tools.randomize.execute({ seed: 17 });
assert.deepEqual(tools.get_recipe.execute({ index: 0 }), favorite);
tools.select_candidate.execute({ index: 2 });
const winner = tools.get_recipe.execute({ index: 2 });
tools.evolve.execute({ ratings: [0, 0, 1, 0, 0, 0], seed: 44 });
assert.deepEqual(tools.get_recipe.execute({ index: 0 }), winner);
assert.equal(tools.list_candidates.execute().generation, 2);
const before = engine.snapshot();
for (const [name, args] of [
  ['create_sound', { kind: 'typo' }], ['create_sound', {}],
  ['create_sound', { kind: 'laser', seed: -1 }],
  ['create_sound', { kind: 'laser', population: 6.5 }],
  ['randomize', { seed: 4294967296 }], ['randomize', { strength: NaN }],
  ['select_candidate', { index: 1.5 }], ['select_candidate', { index: 11 }],
  ['edit_sound', { room: Infinity }], ['edit_sound', { pitch_hz: 600, room: 2 }],
  ['edit_sound', { unknown: 1 }], ['edit_sound', {}], ['edit_sound', { room: null }],
  ['evolve', { ratings: [1, 0] }], ['evolve', { ratings: [1, 0, 0, 0, 0, NaN] }],
  ['list_candidates', { op: 'snapshot' }], ['analyze', { index: 0, sample_rate: '48000' }],
]) {
  assert.throws(() => tools[name].execute(args), `${name}: ${JSON.stringify(args)}`);
  assert.equal(engine.snapshot(), before, `Failed ${name} changed state`);
}
const pcm = tools.render_audio.execute({ index: 0 });
const wav = tools.export_wav.execute({ index: 0 });
assert.ok(pcm.data instanceof Float32Array);
assert.ok(wav.data instanceof Uint8Array);
assert.equal(pcm.channels, 2); assert.equal(wav.sample_rate, 48000);
assert.equal(wav.data.length, 44 + pcm.data.length * 2);
assert.ok(tools.analyze.execute({ index: 0 }).peak > 0);
let published;
const withAttachments = createChirrpTools(engine, {
  publishAudio: async audio => { published = audio; return { artifact_id: 'local-test' }; },
});
assert.deepEqual(await withAttachments.export_wav.execute({ index: 0 }), { artifact_id: 'local-test' });
assert.ok(published.data instanceof Uint8Array);
assert.equal(published.format, 'wav');
engine.restore_session(before);
engine.replace_recipe(JSON.stringify(original));
assert.deepEqual(JSON.parse(engine.get_recipe(0)), original);
const random = tools.random_sound.execute({ seed: 237 });
assert.equal(random.candidates.length, 6);
assert.equal(random.selected, 1);
const randomState = engine.snapshot();
tools.random_sound.execute({ seed: 237 });
assert.equal(engine.snapshot(), randomState);
assert.throws(() => tools.random_sound.execute({ population: 1 }));
assert.equal(engine.snapshot(), randomState);
engine.free();
console.log('All 15 named tools passed against real WASM, including atomic errors, defaults, and artifact delivery.');

// Additive stateless exports share validation and keep the active session.
const assetEngine = new Chirrp();
const assets = createChirrpTools(assetEngine);
assets.create_sound.execute({ kind: 'heavy_slug' });
const assetBefore = assetEngine.snapshot();
const slug = assets.get_recipe.execute({ index: 0 });
const bed = assets.generate_loop.execute({ kind: 'hull_rumble', loop_s: 0.5, sample_rate: 24000, mono: true, dry_mid: true });
assert.equal(bed.channels, 1);
assert.equal(bed.data.length, 44 + 12000 * 2 + 68);
assert.equal(new TextDecoder().decode(bed.data.slice(44 + 24000, 48 + 24000)), 'smpl');
const layer = assets.mix_layers.execute({ layers: [{ recipe: slug, gain: 0.25, delay_s: 0.1 }], mono: true });
assert.equal(layer.channels, 1);
assert.ok(layer.data instanceof Uint8Array);
assert.ok(assets.render_sound.execute({ recipe: slug, mono: true, dry_mid: true }).data instanceof Uint8Array);
for (const [tool, args] of [
  ['generate_loop', { kind: 'hull_rumble', loop_s: 0 }],
  ['generate_loop', { kind: 'hull_rumble', loop_s: 1, mono: 'yes' }],
  ['mix_layers', { layers: [{ recipe: slug, delay_s: -1 }] }],
  ['mix_layers', { layers: [{ recipe: slug, gain: NaN }] }],
  ['render_sound', { recipe: { ...slug, seed: -1 } }],
]) assert.throws(() => assets[tool].execute(args));
assert.equal(assetEngine.snapshot(), assetBefore);
assetEngine.free();
console.log('Loop, dry mono, and timed-layer exports passed against real WASM.');
