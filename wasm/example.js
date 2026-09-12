const worker = new Worker(new URL('./worker.js', import.meta.url), { type: 'module' });
const sound = document.querySelector('#sound');
const generate = document.querySelector('#generate');
const status = document.querySelector('#status');
const audio = document.querySelector('#audio');
const download = document.querySelector('#download');
let audioUrl;

worker.onmessage = ({ data }) => {
  if (data.type === 'ready') {
    for (const { kind } of data.sounds) sound.add(new Option(kind, kind));
    sound.value = 'laser';
    sound.disabled = generate.disabled = false;
    status.textContent = 'Ready. Choose a sound and generate it.';
  } else if (data.type === 'wav') {
    if (audioUrl) URL.revokeObjectURL(audioUrl);
    audioUrl = URL.createObjectURL(new Blob([data.bytes], { type: 'audio/wav' }));
    audio.src = download.href = audioUrl;
    download.download = `${data.kind}.wav`;
    download.hidden = false;
    generate.disabled = false;
    status.textContent = 'Sound generated. Press play to listen.';
  } else if (data.type === 'error') {
    status.textContent = data.message;
    generate.disabled = sound.disabled;
  }
};
worker.onerror = (event) => {
  status.textContent = `Worker failed: ${event.message}. Serve this page over HTTP and build wasm/pkg first.`;
  sound.disabled = generate.disabled = true;
};
generate.onclick = () => {
  generate.disabled = true;
  status.textContent = 'Generating…';
  worker.postMessage({ kind: sound.value, seed: 42 });
};
window.addEventListener('pagehide', () => {
  if (audioUrl) URL.revokeObjectURL(audioUrl);
});
