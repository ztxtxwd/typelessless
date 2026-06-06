const { event, core } = window.__TAURI__;
const { invoke } = core;

const overlay = document.getElementById('overlay');
const canvas = document.getElementById('waveform');
const ctx = canvas.getContext('2d');
const cancelBtn = document.getElementById('cancel-btn');
const confirmBtn = document.getElementById('confirm-btn');

const NUM_BARS = 10;
const BAR_WIDTH = 2;
const GAP = 2;

// Each bar oscillates with its own phase / frequency so the silhouette never
// looks like a deterministic bell curve, but the bars stay in their slots
// (no horizontal scrolling). Heights scale with the live audio level.
const PHASE = new Array(NUM_BARS);
const FREQ = new Array(NUM_BARS);
const BIAS = new Array(NUM_BARS);
for (let i = 0; i < NUM_BARS; i++) {
  PHASE[i] = Math.random() * Math.PI * 2;
  FREQ[i] = 1.6 + Math.random() * 2.4;       // 1.6–4.0 Hz per bar
  BIAS[i] = 0.35 + Math.random() * 0.65;     // each bar has its own loudness gain
}

let level = 0;          // current displayed audio level (smoothed)
let targetLevel = 0;    // latest level reported by backend
let startedAt = 0;      // performance.now() at recording start
let canvasW = 0;
let canvasH = 22;
let dpr = window.devicePixelRatio || 1;
let rafId = null;

function sizeCanvas() {
  const rect = canvas.getBoundingClientRect();
  canvasW = rect.width || (NUM_BARS * (BAR_WIDTH + GAP));
  canvasH = rect.height || 22;
  canvas.width = canvasW * dpr;
  canvas.height = canvasH * dpr;
  ctx.setTransform(1, 0, 0, 1, 0, 0);
  ctx.scale(dpr, dpr);
}

function drawWaveform(now) {
  ctx.clearRect(0, 0, canvasW, canvasH);

  const totalBarSpan = NUM_BARS * BAR_WIDTH + (NUM_BARS - 1) * GAP;
  const startX = Math.max(0, (canvasW - totalBarSpan) / 2);
  const centerY = canvasH / 2;
  const maxBarH = canvasH * 0.9;

  const t = (now - startedAt) / 1000; // seconds since recording started

  ctx.fillStyle = '#ffffff';

  for (let i = 0; i < NUM_BARS; i++) {
    // Two layered sines per bar give an irregular, non-symmetric profile.
    const wobble =
      0.55 + 0.45 *
      (0.6 * Math.sin(t * FREQ[i] + PHASE[i]) +
        0.4 * Math.sin(t * FREQ[i] * 0.53 + PHASE[i] * 1.7));
    const h = Math.max(2, BIAS[i] * wobble * level * maxBarH);
    const x = startX + i * (BAR_WIDTH + GAP);
    ctx.beginPath();
    ctx.roundRect(x, centerY - h / 2, BAR_WIDTH, h, BAR_WIDTH / 2);
    ctx.fill();
  }
}

function tick(now) {
  // Smooth toward the target level so volume changes look organic, not jittery.
  level += (targetLevel - level) * 0.25;
  drawWaveform(now || performance.now());
  rafId = requestAnimationFrame(tick);
}

function startAnimation() {
  if (rafId == null) rafId = requestAnimationFrame(tick);
}

function stopAnimation() {
  if (rafId != null) {
    cancelAnimationFrame(rafId);
    rafId = null;
  }
}

function setThinking(on) {
  overlay.classList.toggle('thinking', !!on);
}

function setLoading(on) {
  overlay.classList.toggle('loading', !!on);
}

event.listen('waveform-update', (e) => {
  const rms = e.payload;
  // Map raw RMS (~0.002–0.05 typical speech) into a 0..1 visual range.
  // Floor at 0.15 so the bell shape is always faintly visible while recording.
  const mapped = Math.min(1.0, Math.sqrt(rms) * 4);
  targetLevel = Math.max(0.15, mapped);
});

event.listen('recording-loading', () => {
  setThinking(false);
  setLoading(true);
});

event.listen('recording-started', () => {
  setLoading(false);
  setThinking(false);
  level = 0;
  targetLevel = 0.15;
  startedAt = performance.now();
  for (let i = 0; i < NUM_BARS; i++) {
    PHASE[i] = Math.random() * Math.PI * 2;
  }
  startAnimation();
});

event.listen('recording-stopped', () => {
  setLoading(false);
  targetLevel = 0;
  setTimeout(() => {
    stopAnimation();
    level = 0;
    drawWaveform(performance.now());
  }, 200);
});

event.listen('thinking-started', () => {
  setThinking(true);
});

event.listen('thinking-stopped', () => {
  setThinking(false);
});

cancelBtn.addEventListener('click', () => {
  invoke('cancel_recording').catch(() => {});
});

confirmBtn.addEventListener('click', () => {
  invoke('confirm_recording').catch(() => {});
});

window.addEventListener('resize', () => {
  sizeCanvas();
  drawWaveform(performance.now());
});

sizeCanvas();
drawWaveform(performance.now());
