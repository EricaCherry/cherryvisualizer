/** Small, dependency-free DSP helpers shared by the audio drivers. */

export const clamp01 = (x: number): number => (x < 0 ? 0 : x > 1 ? 1 : x);
export const clamp = (x: number, lo: number, hi: number): number =>
  x < lo ? lo : x > hi ? hi : x;
export const lerp = (a: number, b: number, t: number): number => a + (b - a) * t;

/** Map a frequency in Hz to an FFT bin index. */
export function hzToBin(hz: number, nyquist: number, binCount: number): number {
  return clamp(Math.round((hz / nyquist) * binCount), 0, binCount - 1);
}

/** Average normalized (0..1) magnitude of byte-FFT bins between two frequencies. */
export function bandEnergy(
  freq: Uint8Array,
  loHz: number,
  hiHz: number,
  nyquist: number,
): number {
  const lo = hzToBin(loHz, nyquist, freq.length);
  const hi = Math.max(lo + 1, hzToBin(hiHz, nyquist, freq.length));
  let sum = 0;
  for (let i = lo; i < hi; i++) sum += freq[i];
  return clamp01(sum / ((hi - lo) * 255));
}

/**
 * Fill `out` with log-spaced band energies from a byte-FFT array.
 * Lower bands are narrow (bass detail), upper bands wide (matches perception).
 */
export function computeLogBands(
  freq: Uint8Array,
  out: Float32Array,
  nyquist: number,
  minHz = 20,
  maxHz = 16000,
): void {
  const n = out.length;
  const logMin = Math.log(minHz);
  const logMax = Math.log(maxHz);
  for (let b = 0; b < n; b++) {
    const f0 = Math.exp(lerp(logMin, logMax, b / n));
    const f1 = Math.exp(lerp(logMin, logMax, (b + 1) / n));
    out[b] = bandEnergy(freq, f0, f1, nyquist);
  }
}

/** Downsample/resample `src` into `dst` (box average), scaling values by `scale`. */
export function resampleInto(
  src: ArrayLike<number>,
  dst: Float32Array,
  scale = 1,
): void {
  const ratio = src.length / dst.length;
  for (let i = 0; i < dst.length; i++) {
    const start = Math.floor(i * ratio);
    const end = Math.max(start + 1, Math.floor((i + 1) * ratio));
    let sum = 0;
    for (let j = start; j < end; j++) sum += src[j];
    dst[i] = (sum / (end - start)) * scale;
  }
}

/** Deterministic seeded PRNG (mulberry32). */
export function mulberry32(seed: number): () => number {
  let a = seed >>> 0;
  return function () {
    a |= 0;
    a = (a + 0x6d2b79f5) | 0;
    let t = Math.imul(a ^ (a >>> 15), 1 | a);
    t = (t + Math.imul(t ^ (t >>> 7), 61 | t)) ^ t;
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}

function median(values: number[]): number {
  if (values.length === 0) return 0;
  const s = [...values].sort((x, y) => x - y);
  const m = s.length >> 1;
  return s.length % 2 ? s[m] : (s[m - 1] + s[m]) / 2;
}

export interface BeatResult {
  beat: 0 | 1;
  onset: number;
  bpm: number | null;
  beatPhase: number;
}

/**
 * Energy-vs-history beat detector with a refractory window, plus a rough BPM
 * estimate and continuous beat phase from the running inter-beat interval.
 * Intentionally simple — good enough to drive visuals; offline analysis can
 * supply precise BPM/downbeats later via the (future) sidecar.
 */
export class BeatDetector {
  private history: number[] = [];
  private readonly historySize = 43; // ~1s at 43fps analysis cadence
  private lastBeatTime = -1;
  private intervals: number[] = [];
  private readonly refractory = 0.12; // 120ms min between beats
  private readonly sensitivity: number;

  constructor(sensitivity = 1.4) {
    this.sensitivity = sensitivity;
  }

  update(energy: number, flux: number, time: number): BeatResult {
    this.history.push(energy);
    if (this.history.length > this.historySize) this.history.shift();

    let avg = 0;
    for (const e of this.history) avg += e;
    avg /= this.history.length || 1;

    const threshold = avg * this.sensitivity;
    let beat: 0 | 1 = 0;
    if (
      energy > threshold &&
      energy > 0.02 &&
      (this.lastBeatTime < 0 || time - this.lastBeatTime > this.refractory)
    ) {
      beat = 1;
      if (this.lastBeatTime > 0) {
        const iv = time - this.lastBeatTime;
        if (iv > 0.25 && iv < 2.0) {
          this.intervals.push(iv);
          if (this.intervals.length > 8) this.intervals.shift();
        }
      }
      this.lastBeatTime = time;
    }

    let bpm: number | null = null;
    let beatPhase = 0;
    if (this.intervals.length >= 3) {
      const med = median(this.intervals);
      if (med > 0) {
        bpm = Math.round(60 / med);
        if (this.lastBeatTime > 0) {
          beatPhase = clamp01((time - this.lastBeatTime) / med);
        }
      }
    }

    return { beat, onset: clamp01(flux * 8), bpm, beatPhase };
  }
}
