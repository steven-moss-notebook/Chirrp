// Execute the real browser WASM binary in Node (no native rendering fallback).
import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import { createHash } from 'node:crypto';
import init, { Chirrp, render_recipe, render_mix, render_loop, render_loop_with_options, render_with_options, render_mix_layers, render_mix_layers_with_options } from '../wasm/pkg/chirrp.js';

await init({ module_or_path: await readFile(new URL('../wasm/pkg/chirrp_bg.wasm', import.meta.url)) });
const engine = new Chirrp();
function invoke(command) {
  const response = JSON.parse(engine.invoke(JSON.stringify(command)));
  assert.equal(response.ok, true, response.error);
  return response.result;
}
const catalog = invoke({ op: 'catalog' });
assert.equal(catalog.length, 66);
for (const { kind } of catalog) {
  let session = invoke({ op: 'create', kind, seed: 42, population: 6 });
  const favorite = session.candidates[0];
  const started = performance.now();
  const pcm = engine.render(0, 48000);
  assert.ok(pcm instanceof Float32Array);
  assert.ok(pcm.length > 0 && pcm.length % 2 === 0);
  let peak = 0, difference = 0;
  for (let i = 0; i < pcm.length; i += 2) {
    assert.ok(Number.isFinite(pcm[i]) && Number.isFinite(pcm[i + 1]));
    peak = Math.max(peak, Math.abs(pcm[i]), Math.abs(pcm[i + 1]));
    difference += Math.abs(pcm[i] - pcm[i + 1]);
  }
  assert.ok(peak > (kind === 'ui_hover' ? 0.005 : 0.03) && peak < 0.891);
  assert.ok(difference > 0.001);
  assert.deepEqual(pcm, render_recipe(JSON.stringify(favorite), 48000));
  const wav = engine.wav(0, 48000);
  assert.ok(wav instanceof Uint8Array);
  assert.equal(new TextDecoder().decode(wav.slice(0, 4)), 'RIFF');
  assert.equal(wav.length, 44 + pcm.length * 2);
  session = invoke({ op: 'randomize', strength: 0.65, seed: 17 });
  assert.deepEqual(session.candidates[0], favorite);
  const restored = invoke({ op: 'restore', session });
  assert.deepEqual(restored, session);
  const ratings = [0, 0, 0, 1, 0, 0];
  const winner = session.candidates[3];
  session = invoke({ op: 'evolve', ratings, strength: 0.5, seed: 5 });
  assert.deepEqual(session.candidates[0], winner);
  console.log(`${kind}: ${pcm.length / 2} stereo frames, render/roundtrip/WAV/evolution ${(performance.now() - started).toFixed(0)} ms`);
}
assert.equal(JSON.parse(engine.invoke('{"op":"snapshot","typo":1}')).ok, false);
assert.throws(() => engine.render(99, 48000));
assert.throws(() => engine.render(0, 0));
const legacy = await readFile(new URL('../tests/fixtures/v1-laser.json', import.meta.url), 'utf8');
const oldPcm = render_recipe(legacy, 24000);
assert.equal(createHash('sha256').update(new Uint8Array(oldPcm.buffer)).digest('hex'),
  'da717acfdddaa9b56f8ddf72418b69cfb9f8b38e7f81b2e1616b1bee0e160924',
  'Version-one recipes must retain their original audio');
engine.create_sound('explosion', 42, 6);
assert.equal(JSON.parse(engine.get_recipe(0)).version, 1);
assert.equal(createHash('sha256').update(engine.export_wav(0, 48000)).digest('hex'),
  '6e606b36a32dae6fb19ae1a53ab7d20f852d7da313df657ac02e33c75a5c79f2',
  'Default explosion must be the exact original sound');
const sceneHashes = JSON.parse(await readFile(new URL('../tests/fixtures/v4-scenes.sha256.json', import.meta.url), 'utf8'));
for (const [kind, hash] of Object.entries(sceneHashes)) {
  const recipe = await readFile(new URL(`../tests/fixtures/v4-${kind}.json`, import.meta.url), 'utf8');
  const pcm = render_recipe(recipe, 24000);
  assert.equal(createHash('sha256').update(new Uint8Array(pcm.buffer)).digest('hex'), hash,
    `Saved version-four ${kind} must keep its original audio`);
}
engine.create_sound('waves', 42, 6);
const waves = engine.render_audio(0, 24000);
assert.equal(createHash('sha256').update(new Uint8Array(waves.buffer)).digest('hex'), sceneHashes.waves,
  'The waves default must remain unchanged');
const mixRecipes = [];
for (const kind of ['ui_hover', 'laser', 'impact']) {
  engine.create_sound(kind, 42, 6);
  mixRecipes.push(JSON.parse(engine.get_recipe(0)));
}
const mixInputs = mixRecipes.map(recipe => render_recipe(JSON.stringify(recipe), 24000));
assert.deepEqual(render_mix(JSON.stringify(mixRecipes.slice(0, 1)), 24000), mixInputs[0]);
const mixed = render_mix(JSON.stringify(mixRecipes), 24000);
assert.ok(mixed instanceof Float32Array);
assert.equal(mixed.length, Math.max(...mixInputs.map(pcm => pcm.length)));
const expectedMix = new Float32Array(mixed.length);
let mixPeak = 0;
for (let i = 0; i < expectedMix.length; i++) {
  for (const pcm of mixInputs) expectedMix[i] += pcm[i] ?? 0;
  mixPeak = Math.max(mixPeak, Math.abs(expectedMix[i]));
}
const mixGain = mixPeak > 0.89 ? 0.89 / mixPeak : 1;
for (let i = 0; i < mixed.length; i++) {
  assert.ok(Number.isFinite(mixed[i]) && Math.abs(mixed[i]) < 0.890001);
  assert.ok(Math.abs(mixed[i] - expectedMix[i] * mixGain) < 1e-6);
}
const manyMixed = render_mix(JSON.stringify(Array(17).fill(mixRecipes[0])), 24000);
assert.equal(manyMixed.length, mixInputs[0].length);
assert.throws(() => render_mix('[]', 24000));
assert.throws(() => render_mix('invalid JSON', 24000));
assert.throws(() => render_mix(JSON.stringify(mixRecipes), 0));
assert.throws(() => render_mix(JSON.stringify([{ ...mixRecipes[0], version: 99 }]), 24000));
engine.free();
console.log('WASM integration checks passed.');


const additive = new Chirrp();
additive.create_sound('beam_loop', 42, 2);
const bedJson = additive.get_recipe(0);
const bedPcm = render_loop(bedJson, 24000, 0.5);
assert.equal(bedPcm.length, 24000);
assert.deepEqual(bedPcm, render_loop_with_options(bedJson, 24000, 0.5, '{}'));
const dryLoop = render_loop_with_options(bedJson, 24000, 0.5, '{"dry_mid":true}');
for (let i = 0; i < dryLoop.length; i += 2) assert.equal(dryLoop[i], dryLoop[i+1]);
additive.create_sound('plasma_pulse', 42, 2);
const shotJson = additive.get_recipe(0);
assert.deepEqual(render_recipe(shotJson, 24000), render_with_options(shotJson, 24000, '{}'));
const layersJson = JSON.stringify([{ recipe: JSON.parse(shotJson), gain: 0.5, delay_s: 0.125 }]);
assert.deepEqual(render_mix_layers(layersJson, 24000), render_mix_layers_with_options(layersJson, 24000, '{}'));
const dryShot = render_with_options(shotJson, 24000, '{"dry_mid":true}');
const dryMix = render_mix_layers_with_options(layersJson, 24000, '{"dry_mid":true}');
assert.equal(dryMix.length, dryShot.length + 6000);
for (let i = 0; i < dryShot.length; i++) assert.equal(dryMix[i+6000], dryShot[i]*0.5);
additive.free();
console.log('Additive WASM function exports passed.');

// Frozen before the cinema redesign: every old default and saved v6 voice.
const preCinema = JSON.parse(await readFile(new URL('../tests/fixtures/pre-cinema-bank.json', import.meta.url), 'utf8'));
const preCinemaHashes = JSON.parse(await readFile(new URL('../tests/fixtures/pre-cinema-wasm.sha256.json', import.meta.url), 'utf8'));
for (let i = 0; i < preCinema.length; i++) {
  const pcm = render_recipe(JSON.stringify(preCinema[i]), 24000);
  assert.equal(createHash('sha256').update(Buffer.from(pcm.buffer)).digest('hex'), preCinemaHashes[i].hash, preCinema[i].kind);
}
console.log('All 41 original presets and 26 saved v6 recipes retain exact WASM PCM hashes.');
