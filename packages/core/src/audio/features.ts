/**
 * AudioFeatures — the one per-frame snapshot every mode reads via update().
 *
 * The bus reuses a single object each frame for performance; modes read it
 * synchronously inside update() and must not hold a reference across frames.
 * Versioned at 1.0.0 alongside the Mode ABI.
 */

export const FEATURES_VERSION = '1.0.0' as const;

/** Number of log-spaced frequency bands exposed in `bands`. */
export const BAND_COUNT = 32;
/** Length of the downsampled spectrum array. */
export const SPECTRUM_SIZE = 256;
/** Length of the time-domain waveform array. */
export const WAVEFORM_SIZE = 1024;

export interface AudioFeatures {
  // --- Clock (single source of truth; visuals never drift) ---
  /** audio.currentTime (realtime) or frameIndex/fps (deterministic), seconds. */
  time: number;
  frameIndex: number;
  /** Seconds since the previous frame. */
  dt: number;
  sampleRate: number;

  // --- Scalars (band energies log-binned, normalized 0..1) ---
  /** ~20–250 Hz. */
  bass: number;
  /** ~250–2000 Hz. */
  mid: number;
  /** ~2k–11k Hz. */
  treble: number;
  /** Overall loudness 0..1. */
  rms: number;
  /** Peak absolute sample 0..1. */
  peak: number;
  /** dBFS, ~[-90..0]. */
  loudnessDb: number;
  /** Spectral centroid (Hz) — brightness/timbre. */
  centroid: number;
  /** Spectral flux — onset-strength proxy 0..1. */
  flux: number;

  // --- Rhythm ---
  /** 1 for exactly one frame on a detected beat, else 0. */
  beat: 0 | 1;
  /** Monotonic count of beats since start. */
  beatCount: number;
  /** Continuous onset envelope 0..1. */
  onset: number;
  /** Estimated tempo; null until confident. */
  bpm: number | null;
  /** Position within the current beat, [0..1). */
  beatPhase: number;

  // --- Vectors (reused buffers) ---
  /** Log-spaced band energies, length {@link BAND_COUNT}, 0..1. */
  bands: Float32Array;
  /** Downsampled magnitude spectrum, length {@link SPECTRUM_SIZE}, 0..1. */
  spectrum: Float32Array;
  /** Time-domain samples, length {@link WAVEFORM_SIZE}, -1..1. */
  waveform: Float32Array;
}

/** Allocate a zeroed AudioFeatures snapshot. */
export function createEmptyFeatures(): AudioFeatures {
  return {
    time: 0,
    frameIndex: 0,
    dt: 0,
    sampleRate: 44100,
    bass: 0,
    mid: 0,
    treble: 0,
    rms: 0,
    peak: 0,
    loudnessDb: -90,
    centroid: 0,
    flux: 0,
    beat: 0,
    beatCount: 0,
    onset: 0,
    bpm: null,
    beatPhase: 0,
    bands: new Float32Array(BAND_COUNT),
    spectrum: new Float32Array(SPECTRUM_SIZE),
    waveform: new Float32Array(WAVEFORM_SIZE),
  };
}
