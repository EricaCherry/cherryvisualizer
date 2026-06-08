import { DeterministicDriver } from '@cherry/core';
import { MODES } from '@cherry/modes';
import { Engine } from './engine';
import { AudioInput } from './audio-input';
import './styles.css';

const $ = <T extends HTMLElement>(id: string): T => {
  const el = document.getElementById(id);
  if (!el) throw new Error(`#${id} missing`);
  return el as T;
};

const canvas = $<HTMLCanvasElement>('stage');
const engine = new Engine(canvas);
const realtimeDriver = engine.bus.activeDriver; // the default RealtimeDriver
const audio = new AudioInput();
audio.acquireContext = () => engine.bus.ensureContext(); // share one AudioContext app-wide

// --- DOM ---
const modeSelect = $<HTMLSelectElement>('mode');
const fileInput = $<HTMLInputElement>('file');
const micBtn = $<HTMLButtonElement>('mic');
const demoBtn = $<HTMLButtonElement>('demo');
const playBtn = $<HTMLButtonElement>('playpause');
const statusEl = $<HTMLSpanElement>('status');
const hint = $<HTMLDivElement>('hint');
const hudMode = $<HTMLSpanElement>('hud-mode');
const hudBpm = $<HTMLSpanElement>('hud-bpm');
const hudBeat = $<HTMLSpanElement>('hud-beat');
const hudFps = $<HTMLSpanElement>('hud-fps');

// --- mode switcher ---
MODES.forEach((m, i) => {
  const opt = document.createElement('option');
  opt.value = String(i);
  opt.textContent = `${m.manifest.name}  (${m.manifest.category})`;
  modeSelect.appendChild(opt);
});

async function selectMode(index: number): Promise<void> {
  await engine.loadMode(MODES[index]);
  hudMode.textContent = engine.modeManifest?.name ?? '—';
  modeSelect.value = String(index);
}

modeSelect.addEventListener('change', () => void selectMode(Number(modeSelect.value)));

// --- audio wiring ---
audio.onStatus = (s) => (statusEl.textContent = s);
audio.onConnect = (ctx, node) => {
  engine.bus.setDriver(realtimeDriver);
  engine.bus.connectSource(ctx, node);
  hideHint();
};

fileInput.addEventListener('change', () => {
  const file = fileInput.files?.[0];
  if (file) void audio.playFile(file);
});
micBtn.addEventListener('click', () => void audio.useMic());
demoBtn.addEventListener('click', () => {
  engine.bus.setDriver(new DeterministicDriver(60));
  statusEl.textContent = '🎲 demo signal';
  hideHint();
});
playBtn.addEventListener('click', () => audio.togglePlay());

// --- drag & drop anywhere ---
window.addEventListener('dragover', (e) => {
  e.preventDefault();
  document.body.classList.add('dragover');
});
window.addEventListener('dragleave', (e) => {
  if (e.relatedTarget === null) document.body.classList.remove('dragover');
});
window.addEventListener('drop', (e) => {
  e.preventDefault();
  document.body.classList.remove('dragover');
  const file = e.dataTransfer?.files?.[0];
  if (file && file.type.startsWith('audio/')) void audio.playFile(file);
});

let hintHidden = false;
function hideHint(): void {
  if (hintHidden) return;
  hintHidden = true;
  hint.classList.add('hidden');
}

// --- HUD ---
let fps = 60;
let lastBeatAt = -1;
let hudClock = 0;
engine.onFrame = ({ dt, features }) => {
  fps += ((1 / Math.max(dt, 1e-3)) - fps) * 0.1;
  if (features.beat) lastBeatAt = performance.now();
  const beatOn = performance.now() - lastBeatAt < 110;
  hudBeat.classList.toggle('on', beatOn);

  hudClock += dt;
  if (hudClock > 0.1) {
    hudClock = 0;
    hudBpm.textContent = features.bpm ? `${features.bpm} BPM` : '-- BPM';
    hudFps.textContent = `${Math.round(fps)} fps`;
  }
};

// --- go ---
void selectMode(0);
engine.start();

// dev/debug handle — lets tooling drive frames when rAF is throttled (headless)
(window as unknown as { __cherry: unknown }).__cherry = { engine, audio, MODES };
