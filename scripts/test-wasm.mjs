// Execute the real browser WASM binary in Node (no native rendering fallback).
import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import { createHash } from 'node:crypto';
import init, { Chirrp, render_recipe } from '../wasm/pkg/chirrp.js';

await init({ module_or_path: await readFile(new URL('../wasm/pkg/chirrp_bg.wasm', import.meta.url)) });
const engine = new Chirrp();
function invoke(command) {
  const response = JSON.parse(engine.invoke(JSON.stringify(command)));
  assert.equal(response.ok, true, response.error);
  return response.result;
}
const catalog = invoke({ op: 'catalog' });
assert.equal(catalog.length, 39);
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
engine.free();
console.log('WASM integration checks passed.');
