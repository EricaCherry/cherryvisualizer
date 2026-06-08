import type { AudioFeatures } from './features';

/**
 * An AudioDriver fills the shared AudioFeatures snapshot each frame. Two
 * interchangeable implementations sit behind the bus:
 *  - RealtimeDriver: AnalyserNode-based, for live playback / mic.
 *  - DeterministicDriver: precomputed/synthesized, for frame-perfect export.
 * Modes can't tell which is active, so the same mode code runs live and exports.
 */
export interface AudioDriver {
  readonly kind: 'realtime' | 'deterministic';
  /** Realtime only: attach to a Web Audio source node. */
  connect?(audioContext: AudioContext, source: AudioNode): void;
  /** Compute features for the current frame into `out`. */
  sample(out: AudioFeatures, dt: number): void;
  /** Audio clock in seconds, or null if the driver has no clock yet. */
  readonly currentTime: number | null;
  /** Effective sample rate. */
  readonly sampleRate: number;
}
