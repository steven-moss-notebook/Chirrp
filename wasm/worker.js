import init, { Chirrp } from './pkg/chirrp.js';

// Rendering is synchronous, so keep it off the page's UI thread.
try {
  await init();
  const engine = new Chirrp();
  self.onmessage = ({ data: { kind, seed } }) => {
    try {
      engine.create_sound(kind, seed, 2);
      const bytes = engine.export_wav(0, 48_000);
      self.postMessage({ type: 'wav', kind, bytes }, [bytes.buffer]);
    } catch (error) {
      self.postMessage({ type: 'error', message: String(error) });
    }
  };
  self.postMessage({ type: 'ready', sounds: JSON.parse(engine.list_sounds()) });
} catch (error) {
  self.postMessage({ type: 'error', message: `Could not load WebAssembly: ${error}` });
}
