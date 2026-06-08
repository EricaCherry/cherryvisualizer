/** @cherry/modes — built-in visualizer modes, registered for the host. */

import type { ModeClass } from '@cherry/core';
import { WaveformBreakout } from './breakout/waveform-breakout';
import { Milkdrop } from './milkdrop/milkdrop';
import { SynthwaveGrid } from './synthwave/synthwave-grid';
import { RadialBars } from './spectrum/radial-bars';
import { StarfieldWarp } from './particles/starfield-warp';
import { Lissajous } from './scope/lissajous';
import { SpectrumBars } from './spectrum/spectrum-bars';

export { WaveformBreakout } from './breakout/waveform-breakout';
export { Milkdrop } from './milkdrop/milkdrop';
export { SynthwaveGrid } from './synthwave/synthwave-grid';
export { RadialBars } from './spectrum/radial-bars';
export { StarfieldWarp } from './particles/starfield-warp';
export { Lissajous } from './scope/lissajous';
export { SpectrumBars } from './spectrum/spectrum-bars';

/** The registry the host reads to populate the mode switcher. */
export const MODES: ModeClass[] = [
  WaveformBreakout,
  Milkdrop,
  SynthwaveGrid,
  RadialBars,
  StarfieldWarp,
  Lissajous,
  SpectrumBars,
];
