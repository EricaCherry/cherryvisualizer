import * as THREE from 'three';
import {
  AudioBus,
  isApiCompatible,
  mulberry32,
  type AudioFeatures,
  type ModeClass,
  type ModeContext,
  type VisualizerMode,
} from '@cherry/core';

export interface FrameInfo {
  dt: number;
  features: AudioFeatures;
}

/**
 * The host. Owns the single WebGLRenderer, the AudioBus, and the active mode's
 * lifecycle. Modes never create a renderer or touch audio — they get both from
 * the ModeContext the host hands them.
 */
export class Engine {
  readonly renderer: THREE.WebGLRenderer;
  readonly bus = new AudioBus();

  private current: VisualizerMode | null = null;
  private currentClass: ModeClass | null = null;
  private raf = 0;
  private last = 0;
  private width = 0;
  private height = 0;
  private dpr = 1;

  onFrame?: (info: FrameInfo) => void;

  constructor(private readonly canvas: HTMLCanvasElement) {
    this.renderer = new THREE.WebGLRenderer({
      canvas,
      antialias: true,
      powerPreference: 'high-performance',
      // keep the last frame in the buffer so headless/screenshot capture is reliable
      preserveDrawingBuffer: true,
    });
    this.renderer.setClearColor(new THREE.Color(0x04050a), 1);
    this.resize();
    window.addEventListener('resize', this.resize);
    // the canvas fills the viewport via CSS; observe it directly so we get the
    // real size even when window.innerWidth isn't available yet (headless/embed).
    new ResizeObserver(() => this.resize()).observe(canvas);
  }

  private resize = (): void => {
    const w = this.canvas.clientWidth || window.innerWidth || 1280;
    const h = this.canvas.clientHeight || window.innerHeight || 720;
    if (w === this.width && h === this.height && w > 0) return;
    this.width = w;
    this.height = h;
    this.dpr = Math.min(window.devicePixelRatio || 1, 2);
    this.renderer.setPixelRatio(this.dpr);
    this.renderer.setSize(this.width, this.height, false);
    this.current?.resize(this.width, this.height);
  };

  get modeManifest() {
    return this.currentClass?.manifest ?? null;
  }

  async loadMode(cls: ModeClass): Promise<void> {
    if (!isApiCompatible(cls.manifest.apiVersion)) {
      throw new Error(
        `Mode "${cls.manifest.id}" needs API ${cls.manifest.apiVersion}, host is incompatible.`,
      );
    }
    if (this.current) {
      this.current.dispose();
      this.current = null;
    }
    const mode = new cls();
    const bus = this.bus;
    const ctx: ModeContext = {
      manifest: cls.manifest,
      width: this.width,
      height: this.height,
      dpr: this.dpr,
      three: { THREE, renderer: this.renderer },
      audio: {
        ensureContext: () => bus.ensureContext(),
        get source() {
          return bus.audioSource;
        },
        onSource: (cb) => bus.onSource(cb),
      },
      container: this.canvas.parentElement ?? document.body,
      rng: mulberry32(0xc0ffee),
      getParam: (name) => cls.manifest.params?.[name]?.default,
    };
    await mode.init(ctx);
    mode.resize(this.width, this.height);
    this.current = mode;
    this.currentClass = cls;
  }

  /** Advance and render exactly one frame. Also the unit used by export/tests. */
  tick(dt: number): void {
    const features = this.bus.update(dt);
    if (this.current) {
      this.current.update(features, dt);
      this.current.render();
    }
    this.onFrame?.({ dt, features });
  }

  start(): void {
    this.last = performance.now();
    const loop = (): void => {
      this.raf = requestAnimationFrame(loop);
      const now = performance.now();
      const dt = Math.min(0.05, (now - this.last) / 1000);
      this.last = now;
      this.tick(dt);
    };
    this.raf = requestAnimationFrame(loop);
  }

  stop(): void {
    cancelAnimationFrame(this.raf);
  }
}
