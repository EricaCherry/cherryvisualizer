import type {
  AudioFeatures,
  ModeContext,
  ModeManifest,
  VisualizerMode,
} from '@cherry/core';

/**
 * Milkdrop (Butterchurn) — the Winamp Milkdrop visualizer, in WebGL, via the
 * MIT-licensed Butterchurn engine. This single mode unlocks thousands of
 * community Milkdrop presets.
 *
 * Butterchurn runs its own WebGL2 context and its own audio analyser, so this
 * mode (backend:'milkdrop') manages its own overlay canvas and taps the shared
 * AudioContext rather than drawing through the host's Three renderer. Both
 * butterchurn packages are dynamically imported so they stay out of the main
 * bundle until this mode is selected.
 */
export class Milkdrop implements VisualizerMode {
  static readonly manifest: ModeManifest = {
    id: 'preset.milkdrop',
    name: 'Milkdrop (Butterchurn)',
    apiVersion: '1.0.0',
    category: 'preset-engine',
    backend: 'milkdrop',
    audioPorts: ['beat'],
    deterministic: false,
    license: 'MIT',
    appeal: 5,
    difficulty: 'medium',
    description: 'Winamp Milkdrop presets rendered in WebGL by Butterchurn (MIT).',
  };
  readonly manifest = Milkdrop.manifest;

  private readonly switchEvery = 16; // seconds between preset changes
  private canvas: HTMLCanvasElement | null = null;
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  private viz: any = null;
  private presets: [string, unknown][] = [];
  private index = 0;
  private since = 0;
  private unsub: (() => void) | null = null;

  async init(ctx: ModeContext): Promise<void> {
    const canvas = document.createElement('canvas');
    Object.assign(canvas.style, {
      position: 'fixed',
      inset: '0',
      width: '100%',
      height: '100%',
      zIndex: '1',
    });
    ctx.container.appendChild(canvas);
    this.canvas = canvas;

    const audioCtx = ctx.audio.ensureContext();
    const butterchurn = (await import('butterchurn')).default;
    // butterchurn-presets exposes getPresets() (older builds default-export the map)
    const presetsMod = (await import('butterchurn-presets')) as {
      default?: { getPresets?: () => Record<string, unknown> } & Record<string, unknown>;
      getPresets?: () => Record<string, unknown>;
    };
    const getPresets =
      presetsMod.getPresets ??
      presetsMod.default?.getPresets ??
      (() => (presetsMod.default as Record<string, unknown>) ?? {});
    this.presets = Object.entries(getPresets());

    const dpr = Math.min(window.devicePixelRatio || 1, 2);
    this.viz = butterchurn.createVisualizer(audioCtx, canvas, {
      width: ctx.width,
      height: ctx.height,
      pixelRatio: dpr,
    });

    if (ctx.audio.source) this.viz.connectAudio(ctx.audio.source);
    this.unsub = ctx.audio.onSource((_c, src) => this.viz?.connectAudio(src));

    this.loadPreset(0, 0);
  }

  private loadPreset(i: number, blend = 2.7): void {
    if (!this.presets.length || !this.viz) return;
    this.index = ((i % this.presets.length) + this.presets.length) % this.presets.length;
    this.viz.loadPreset(this.presets[this.index][1], blend);
  }

  resize(width: number, height: number): void {
    this.viz?.setRendererSize(width, height);
  }

  update(_features: AudioFeatures, dt: number): void {
    this.since += dt;
    if (this.since >= this.switchEvery) {
      this.since = 0;
      this.loadPreset(this.index + 1);
    }
  }

  render(): void {
    this.viz?.render();
  }

  dispose(): void {
    this.unsub?.();
    this.unsub = null;
    this.viz = null;
    this.canvas?.remove();
    this.canvas = null;
    this.presets = [];
  }
}
