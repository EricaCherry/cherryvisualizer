/**
 * Cherry Mode ABI — the contract every visualizer mode implements.
 *
 * Semver'd: the host advertises CONTRACT_VERSION and checks each manifest's
 * `apiVersion`. The happy path for a mode author is a single file exporting a
 * class that implements {@link VisualizerMode}.
 *
 * Phase 0 ships a deliberately small surface (one rendering backend, the audio
 * ports the first modes actually use). The full ABI in docs/ARCHITECTURE.md adds
 * more backends, offscreen compositing, and transitions — all additive to this.
 */

import type { AudioFeatures } from './audio/features';

export const CONTRACT_VERSION = '1.0.0' as const;

/** Which rendering backend a mode draws with. */
export type Backend = 'three' | 'canvas2d' | 'milkdrop';

/** Audio channels a mode can declare it needs. Drives (future) lazy extraction. */
export type AudioPort =
  | 'bass' | 'mid' | 'treble'
  | 'rms' | 'peak' | 'loudnessDb'
  | 'centroid' | 'flux'
  | 'beat' | 'beatCount' | 'onset' | 'bpm' | 'beatPhase'
  | 'bands' | 'spectrum' | 'waveform';

export type Difficulty = 'easy' | 'medium' | 'hard';

export type ModeCategory =
  | 'classic-spectral' | 'preset-engine' | 'shader' | 'particles'
  | '3d' | 'arcade' | 'runner-satisfying' | 'generative' | 'experimental';

/** A single tweakable parameter. Auto-renders UI (later) and is keyframeable. */
export interface ParamSpec {
  type: 'float' | 'int' | 'bool' | 'color' | 'enum';
  default: unknown;
  min?: number;
  max?: number;
  step?: number;
  options?: string[];
  label?: string;
  automatable?: boolean;
}

export interface ModeManifest {
  /** Stable id, e.g. "game.breakout", "spectrum.bars". */
  id: string;
  name: string;
  /** Semver, checked against CONTRACT_VERSION. */
  apiVersion: string;
  category: ModeCategory;
  backend: Backend;
  /** The only audio the host wires up for this mode. */
  audioPorts: AudioPort[];
  params?: Record<string, ParamSpec>;
  /** True => safe for frame-perfect video export (pure wrt features + seeded rng). */
  deterministic?: boolean;
  /** SPDX license of this mode's code. */
  license: string;
  description?: string;
  appeal?: number;
  difficulty?: Difficulty;
}

/**
 * Host-owned Three.js handle, passed into a `three`-backend mode's init().
 * Typed loosely so @cherry/core keeps zero runtime dependencies — modes import
 * `three` directly and narrow these as needed.
 */
export interface ThreeHandle {
  /** The `three` module namespace. */
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  THREE: any;
  /** The single shared WebGLRenderer (THREE.WebGLRenderer). */
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  renderer: any;
}

/** Audio access for modes that run their own pipeline (e.g. preset engines). */
export interface AudioHandle {
  /** Create or return the single shared AudioContext. */
  ensureContext(): AudioContext;
  /** The currently connected source node, or null if none yet. */
  readonly source: AudioNode | null;
  /** Subscribe to source (re)connections; returns an unsubscribe fn. */
  onSource(cb: (context: AudioContext, source: AudioNode) => void): () => void;
}

/** Everything a mode receives at init(). Modes never create a canvas/context. */
export interface ModeContext {
  manifest: ModeManifest;
  width: number;
  height: number;
  dpr: number;
  /** Present for backend:'three'. */
  three: ThreeHandle;
  /** Shared audio access for backend:'milkdrop' and similar self-driven modes. */
  audio: AudioHandle;
  /** DOM container a mode may attach its own canvas to (e.g. preset engines). */
  container: HTMLElement;
  /** Seeded RNG. Deterministic modes must use this, never Math.random. */
  rng: () => number;
  /** Live value of a declared param (its default in Phase 0). */
  getParam: (name: string) => unknown;
}

/** The contract every mode implements, identical across backends. */
export interface VisualizerMode {
  readonly manifest: ModeManifest;
  /** Acquire GPU resources, build scene/shaders/world. */
  init(ctx: ModeContext): void | Promise<void>;
  /** Viewport changed. */
  resize(width: number, height: number): void;
  /** Step the sim/automation. Pure wrt (features, seeded state); dt is the only clock. */
  update(features: AudioFeatures, dt: number): void;
  /** Draw exactly one frame. */
  render(): void;
  /** Release every resource acquired in init(). */
  dispose(): void;
}

/** A mode is published as a class with a static manifest. */
export interface ModeClass {
  new (): VisualizerMode;
  readonly manifest: ModeManifest;
}

/** Compare just the major version — minor/patch are backwards compatible. */
export function isApiCompatible(modeApiVersion: string): boolean {
  const major = (v: string) => parseInt(v.split('.')[0] ?? '0', 10);
  return major(modeApiVersion) === major(CONTRACT_VERSION);
}
