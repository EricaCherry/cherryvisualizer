# Cherry — Architecture

## TL;DR — the stack

Cherry — a TypeScript-core, WebGPU-first (Three.js r184+ WebGPURenderer + TSL) modular music visualizer with a single language-agnostic semver'd Mode ABI, a single AudioFeatures bus fed by two interchangeable drivers (realtime AnalyserNode+Meyda / deterministic OfflineAudioContext), per-backend renderers (Three/TSL primary; PixiJS, OGL, regl, Phaser4+Box2D, raw-WGSL, Butterchurn all lazy-loaded), shipping three artifacts from one repo (zero-install static web app -> Tauri 2 desktop -> headless-Chromium deterministic video export). Python is an OPTIONAL, out-of-band offline pre-analysis CLI (librosa + beat_this) that emits a JSON sidecar; it is never in the runtime and never an engine dependency.

## Why this, and the Python question

This synthesizes Proposals 1 and 3 (both TS-web-first and near-identical at the core) and rejects Proposal 2 (Rust+wgpu native) as the base. Proposal 3's clean spine is the architecture: one AudioFeatures bus with two drivers, a semver'd language-agnostic ModeAPI, declared audioPorts driving lazy feature extraction, three targets from one codebase. Proposal 1's first-class game/demoscene backends (Phaser 4 + phaser-box2d, rapier2d-deterministic for export, OGL/regl raw-shader) are folded in because the 225-mode set explicitly includes arcade and runner/satisfying-physics categories that a pure shader/3D stack cannot host.

WHY WEB-FIRST TS, NOT RUST-NATIVE: success for a music-channel visualizer is reach + contribution velocity, and the research is unambiguous that both favor the web. (1) Distribution: a URL is the whole product (static web app is the fastest path to audience, no install); the landscape survey shows native-first incumbents (projectM frontends, Astrofox, Plane9, MilkDrop3) lost on discoverability; Proposal 2 itself concedes distribution loses decisively to the web as the single biggest adoption tax. (2) The pool of adaptable code for 225 modes is overwhelmingly JS/GLSL: Butterchurn (MIT) yields 15k+ Milkdrop presets via npm in ~10 minutes; the Shadertoy corpus drops in through the 512x2 dB-scaled audio texture; three.js/PixiJS/OGL/regl/Phaser4 each unlock large MIT ecosystems. Proposal 2 admits no ready-made permissive preset ecosystem and that reference code needs porting through naga. (3) Contributor on-ramp: a mode is a single .ts/.js file with no build step on the happy path vs gating contributors on a Rust toolchain + per-platform CI + code signing/notarization. wgpu's real wins (GPU compute, sub-16ms latency, single desktop+web codebase) are largely recovered: Three.js WebGPURenderer gives GPU compute via TSL/raw-WGSL, the latency budget is enforceable, and Tauri 2 gives desktop (incl. iOS/Android) from the same source. We give up native peak throughput for extreme scenes (1M+ sprites, heavy compute) — acceptable for a visualizer, and the raw-WGSL escape hatch covers the heaviest modes.

WHY THREE.JS WEBGPU + TSL: the renderer survey names it the clear primary — the only option natively unifying 2D+3D+shader authoring, auto-compiling one shader source to WGSL (WebGPU) and GLSL (WebGL2 fallback) since r171, shipping a node-based RenderPipeline built for modular pass composition (the legacy EffectComposer is WebGL-only and silently breaks under WebGPURenderer — never used), MIT with a huge ecosystem. WebGPU is ~82% desktop / ~71% mobile in 2026 with Firefox-desktop flag-gated, so WebGL2 is the guaranteed floor and ships unconditionally; TSL means modes are authored once and run on both backends, avoiding the WebGL2 ceiling that forces incumbents into rewrites while still shipping everywhere today.

THE PYTHON QUESTION (resolved): Python is the WRONG base and the RIGHT optional helper. All three proposals and the research converge. Against Python-as-core: every load-bearing asset is JS/TS + GPU-shader (Butterchurn presets, Meyda the only permissive realtime MIR lib, Three.js/WebGPU rendering, OfflineAudioContext-based export, Tauri shell); a Python core (moderngl/pygame) strands you from all of it and reintroduces distribution-trust problems (PyInstaller AV false-positives, large opaque binaries), the loopback-audio gap, and a GIL-bound 60fps render+analysis loop. FOR Python, narrowly: an offline out-of-band CLI that pre-analyses a full track into a timestamped JSON sidecar — global BPM, key, beat/downbeat positions — using librosa (ISC) primary and beat_this (MIT, ISMIR 2024). These are exactly the features unreliable from a short realtime window, and precomputing them makes them deterministic for export. Hard license constraint: MIT/BSD/ISC only — EXCLUDE madmom (CC-BY-NC-SA weights), Essentia (AGPL), aubio (GPL-3). The sidecar merges into the bus as an optional channel sampled against audio.currentTime; the engine runs fully without it.

HOW 225 MODES ACROSS 9 CATEGORIES FIT ONE ABI: every mode implements the same lifecycle and declares (a) a backend tag and (b) the audio ports it consumes; the host owns the single GPU context, hands each mode a dedicated offscreen RenderTarget/TextureView, and composites all outputs through the node RenderPipeline. classic-spectral -> tsl/regl reading fftTex+bands; preset-engines -> milkdrop backend (Butterchurn web / libprojectM native, dynamically linked); shaders -> tsl/ogl reading the 512x2 Shadertoy fftTex; particles -> wgsl-compute/tsl or pixi2d; 3D -> tsl; arcade + runner/satisfying -> phaser (Phaser4 + phaser-box2d realtime, rapier2d-deterministic export) with bodies driven by bus features; generative + experimental -> any backend, deterministic:true makes them exportable. The declared-ports + shared-bus + host-owned-context design is precisely what lets one interface span all nine.

## The Mode ABI

Every one of the 225+ modes implements this single, semver'd interface. The happy path is **one `.ts`/`.js` file, no build step.**

```ts
// ===========================================================================
// Cherry Mode ABI — every one of the 225 modes implements VisualizerMode.
// Semver'd: host advertises CONTRACT_VERSION; manifest.apiVersion is checked,
// mismatches are REJECTED, not silently run. Happy path = drop one .ts/.js
// file exporting `default` VisualizerMode; no build step.
// ===========================================================================

export const CONTRACT_VERSION = "1.0.0";

// --- What audio a mode declares it needs (drives lazy feature extraction:
//     host only computes/uploads channels some ACTIVE mode requests) ----------
type AudioPort =
  | "bass" | "mid" | "treble"        // log-binned, attack/release-smoothed band energy
  | "rms" | "loudnessDb"
  | "centroid" | "rolloff" | "flux"  // timbre + onset proxy
  | "beat" | "beatCount" | "onset"   // beat = 1.0 for exactly one frame
  | "chroma" | "mfcc"                // Float32Array vectors
  | "waveform"                       // time-domain Float32Array (scopes/Lissajous)
  | "fftTex" | "waveformTex"         // GPU textures (raw-shader modes)
  | "sidecar";                       // optional Python BPM/key/downbeats

type Backend =
  | "tsl"          // DEFAULT. Three.js TSL -> WGSL(WebGPU)/GLSL(WebGL2). 2D & 3D.
  | "wgsl-compute" // raw GPUDevice + WGSL compute. Gated by navigator.gpu. CPU/WASM fallback.
  | "ogl"          // lazy. ultra-lean fullscreen raw GLSL -> TextureNode.
  | "regl"         // lazy. functional WebGL data-viz modes.
  | "pixi2d"       // lazy. 2D sprite/HUD via shared-context (resetState + stencil).
  | "phaser"       // lazy. Phaser4 + phaser-box2d games; rapier2d-deterministic on export.
  | "milkdrop"     // Butterchurn (web) / libprojectM (native). One built-in wrapper.
  | "sandbox";     // p5.js / isolated mode via Worker/iframe message-passing (same lifecycle).

interface ParamSpec {                 // JSON-schema-ish; auto-renders UI + is keyframeable
  type: "float" | "int" | "bool" | "color" | "enum" | "select-preset";
  default: unknown; min?: number; max?: number; step?: number; options?: string[];
  label?: string; automatable?: boolean; // automatable params feed timeline + beat-triggers
}

interface ModeManifest {
  id: string;                         // "spectrum.bars", "milkdrop", "game.breakout", "demo.plasma"
  name: string;
  apiVersion: string;                 // semver; checked against CONTRACT_VERSION
  category: "classic-spectral" | "preset-engine" | "shader" | "particles"
          | "3d" | "arcade" | "runner-satisfying" | "generative" | "experimental";
  backend: Backend;
  audioPorts: AudioPort[];            // declared inputs — the only audio the host wires up
  params: Record<string, ParamSpec>;
  deterministic: boolean;             // true => safe for frame-perfect video export
  license: string;                    // SPDX; non-permissive content is flagged/quarantined
}

// --- Host-owned, passed into init(). Modes NEVER create a canvas/context. -----
interface ModeContext {
  manifest: ModeManifest;
  resolution: { w: number; h: number; dpr: number };
  params: ParamStore;                 // live values for declared params; .on('change', ...)
  rng: () => number;                  // SEEDED RNG. No Math.random in deterministic modes.

  // Backend handles — exactly one is non-null, matching manifest.backend:
  three?: WebGPURenderer;             // 'tsl' — shared rendering-context owner
  pipeline?: RenderPipeline;          // 'tsl' — register composable TSL passes here
  device?: GPUDevice;                 // 'wgsl-compute' (null if WebGPU absent -> fallback)
  gl?: WebGL2RenderingContext;        // 'ogl' | 'regl' | 'pixi2d' shared-context plugins
  phaser?: PhaserGameHandle;          // 'phaser' — Box2D/Rapier world + scene
  milkdrop?: MilkdropHandle;          // 'milkdrop' — Butterchurn/libprojectM instance
  worker?: SandboxBridge;             // 'sandbox' — postMessage(init/render/...) transport

  // EVERY backend draws into this host-allocated offscreen target; the host
  // composites it as a TextureNode in the RenderPipeline. This is what makes
  // heterogeneous renderers (Three+Pixi+OGL+Phaser) coexist with one context.
  outputTarget: RenderTarget;
}

// --- The contract every mode implements (identical across all backends) ------
interface VisualizerMode {
  readonly manifest: ModeManifest;

  init(ctx: ModeContext): void | Promise<void>;      // acquire GPU resources, build scene/shaders/world
  resize(w: number, h: number): void;                // viewport change
  update(features: AudioFeatures, dt: number): void; // step sim/automation. PURE wrt (features, seeded state).
                                                     // dt and features.time are the ONLY clocks. No wall-clock.
  render(): void;                                    // draw ONE frame into ctx.outputTarget
  dispose(): void;                                   // release every GPU resource (dev leak-guard enforced)
}

// ----------------------------- EXAMPLES --------------------------------------
// (1) RAW-SHADER mode (Shadertoy port). backend:'tsl' (or 'ogl'); reads fftTex.
const Plasma: VisualizerMode = {
  manifest: { id:"demo.plasma", name:"Plasma", apiVersion:"1.0.0", category:"shader",
    backend:"tsl", audioPorts:["fftTex","bass","beat"], deterministic:true,
    license:"MIT", params:{ warp:{type:"float",default:1.2,min:0,max:4,automatable:true} } },
  init(ctx){ /* build fullscreen TSL quad; bind ctx.params.warp + feature uniforms */ },
  resize(){}, update(f){ /* push f.bass, f.beat, f.fftTex into uniforms */ },
  render(){ /* renderer.render(quad -> ctx.outputTarget) */ }, dispose(){}
};

// (2) 2D-CANVAS / classic spectral. backend:'tsl' or 'regl'; reads fftTex + bands.
const Bars: VisualizerMode = {
  manifest: { id:"spectrum.bars", name:"Spectrum Bars", apiVersion:"1.0.0",
    category:"classic-spectral", backend:"tsl", audioPorts:["fftTex","bass","mid","treble"],
    deterministic:true, license:"MIT", params:{ barCount:{type:"int",default:64,min:8,max:256} } },
  init(ctx){ /* instanced quads; log-freq x-axis, dBFS y, peak-hold+gravity */ },
  resize(){}, update(f){ /* sample f.fftTex bins -> bar heights */ }, render(){}, dispose(){}
};

// (3) 3D mode. backend:'tsl'; full Three scene drawn into outputTarget.
const Tunnel3D: VisualizerMode = {
  manifest: { id:"3d.tunnel", name:"Audio Tunnel", apiVersion:"1.0.0", category:"3d",
    backend:"tsl", audioPorts:["bass","beat","centroid"], deterministic:true, license:"MIT",
    params:{ speed:{type:"float",default:1,min:0,max:5,automatable:true} } },
  init(ctx){ /* camera + mesh; ctx.three is the shared renderer */ },
  resize(w,h){}, update(f,dt){ /* advance along tunnel by dt*speed*(1+f.bass) */ },
  render(){}, dispose(){}
};

// (4) ARCADE game. backend:'phaser'; deterministic via rapier2d on export.
const Breakout: VisualizerMode = {
  manifest: { id:"game.breakout", name:"Audio Breakout", apiVersion:"1.0.0", category:"arcade",
    backend:"phaser", audioPorts:["beat","bass","rms"], deterministic:true, license:"MIT", params:{} },
  init(ctx){ /* build Box2D world (rapier-deterministic in export driver) */ },
  resize(){}, update(f,dt){ /* on f.beat===1 spawn/impulse ball; scale paddle by f.rms; world.step(dt) */ },
  render(){ /* phaser draws into ctx.outputTarget FBO */ }, dispose(){}
};
// Key ABI rules: versioned & rejected on mismatch; modes never touch AudioContext
// or the rendering context; update() is pure wrt (features, seeded state) so any
// deterministic:true mode is frame-perfect-exportable; a "scene" is JSON:
// ordered [{modeId, params, transition}] with beat-triggered + timeline automation.
```

## The AudioFeatures bus

The one immutable per-frame snapshot every mode reads. Modes never touch Web Audio directly. Two interchangeable drivers (realtime / deterministic) sit behind it, so the same mode code is live in the browser **and** frame-perfect in video export.

```ts
// ===========================================================================
// AudioFeatures — the ONE immutable per-frame snapshot every mode reads via
// update(). Modes never touch Web Audio. The bus is source-agnostic with TWO
// interchangeable drivers behind it; modes cannot tell which is active.
//   - REALTIME driver: MediaElement/mic/loopback -> AnalyserNode(fftSize=4096)
//       -> Meyda per-frame features. (web + Tauri live playback)
//   - DETERMINISTIC driver: OfflineAudioContext.startRendering() precomputes
//       the WHOLE track into Float32Array[] BEFORE the loop; export indexes by
//       frame number -> zero clock drift, frame-perfect A/V; WAV muxed at end.
// DSP correctness baked in (per research): fftSize>=4096 (sub-100Hz, separates
// C2/D2); log-frequency axis; dBFS y; SEPARATE attack(lambda~=0.1)/release
// (lambda~=0.9) one-pole smoothing (NOT AnalyserNode.smoothingTimeConstant,
// which doesn't even apply to time-domain); peak-hold+gravity init to -Infinity;
// textures via texSubImage2D (never per-frame texImage2D realloc), CLAMP_TO_EDGE
// + LINEAR; fftTex is dB-scaled 0-255 over 0..~11kHz (44.1kHz half-spectrum) —
// documented so authors don't treat dB as linear.
// ===========================================================================
interface AudioFeatures {
  // --- Clock (single source of truth; visuals never drift) ---
  time: number;          // audio.currentTime (realtime) OR frameIndex/fps (export)
  frameIndex: number;
  dt: number;            // seconds since previous frame
  sampleRate: number;    // typically 44100

  // --- Scalar features (band energies log-binned, attack/release smoothed) ---
  bass: number;          // ~20-250 Hz   [0..1]
  mid: number;           // ~250-2000 Hz [0..1]
  treble: number;        // ~2k-11k Hz   [0..1]
  rms: number;           // overall loudness [0..1]
  loudnessDb: number;    // dBFS, ~[-90..0]
  centroid: number;      // spectral centroid (Hz) — brightness/timbre
  rolloff: number;       // spectral rolloff (Hz)
  flux: number;          // spectral flux — onset strength proxy

  // --- Rhythm (realtime: energy-vs-43-frame-history threshold 1.3-1.5x) ---
  beat: 0 | 1;           // 1.0 for EXACTLY one frame on onset, else 0
  beatCount: number;     // monotonic count since start
  onset: number;         // continuous onset envelope [0..1]
  bpm: number | null;    // realtime autocorrelation estimate; null until confident
  beatPhase: number;     // [0..1) position within current beat (needs bpm)

  // --- Vectors ---
  chroma: Float32Array;  // [12] pitch-class energy (Krumhansl-Schmuckler-ready)
  mfcc: Float32Array;    // [13] timbre coefficients (palette/genre mapping)
  waveform: Float32Array;// time-domain samples (oscilloscope/Lissajous; corr-triggered upstream)

  // --- GPU textures (raw-shader modes; Shadertoy 512x2 layout available) ---
  fftTex: GPUTexture | WebGLTexture;       // row0 = dB-scaled spectrum, row1 = waveform
  waveformTex: GPUTexture | WebGLTexture;  // dedicated time-domain texture

  // --- OPTIONAL Python sidecar channel (null if no sidecar loaded) ---
  // Pre-analyzed offline (librosa ISC / beat_this MIT), sampled vs `time`.
  sidecar: {
    globalBpm: number;
    key: string;                 // e.g. "A minor" (Krumhansl-Schmuckler offline)
    beatTimes: Float32Array;     // precise beat timestamps (seconds)
    downbeatTimes: Float32Array; // measure boundaries -> scene transitions snap here
    nearestDownbeat: number;     // convenience: last downbeat <= time
    barPhase: number;            // [0..1) within current musical bar
  } | null;
}
// Uniform/port naming mirrors Audio-Shader-Studio (MIT): u_bass, u_treble,
// u_beatDetected, u_beatCount, u_frequencyTexture, u_timeDomainTexture — so
// contributors and Shadertoy ports map mentally with no translation.
```

## Repository layout

```text
cherry/                         # monorepo (pnpm workspaces + Vite); license: MIT/Apache-2.0/BSD/Unlicense only
  package.json
  pnpm-workspace.yaml
  tsconfig.base.json
  CONTRACT_VERSION.ts           # single source of truth for the Mode ABI semver
  LICENSES.md                   # SPDX inventory; every dep + the quarantine policy

  packages/
    contract/                   # @cherry/contract — ZERO-dep. The ABI surface ONLY.
      src/                      #   VisualizerMode, ModeManifest, ModeContext, AudioFeatures,
                                #   AudioPort, Backend, ParamSpec, CONTRACT_VERSION
                                #   (third-party modes compile against THIS package alone)

    engine-core/               # @cherry/engine — host. Owns the single GPU context.
      src/
        renderer/              #   WebGPURenderer setup + WebGL2 fallback probe
        pipeline/              #   node-based RenderPipeline (NOT legacy EffectComposer)
        compositor/            #   per-mode RenderTarget alloc + scene blending/transitions
        registry/              #   mode discovery + apiVersion gate + lazy import()
        scene/                 #   JSON scene format: [{modeId,params,transition}] + timeline
        automation/            #   keyframe curves + beat-triggers driving ParamStore
        params/                #   ParamStore (live values, serialization, change events)
        capability/            #   navigator.gpu / WebGL2 / WebCodecs probes
        leakguard/             #   dev-only GPU-resource leak detector for dispose()

    audio-bus/                 # @cherry/audio — the AudioFeatures bus + two drivers
      src/
        bus.ts                 #   immutable per-frame snapshot assembly
        ports.ts               #   declared-port -> lazy extractor wiring
        dsp/                   #   log-freq remap, dBFS, attack/release, peak-hold,
                               #     beat detector, autocorrelation BPM, chroma->K-S key,
                               #     correlation trigger, fftTex/waveformTex upload (texSubImage2D)
        drivers/
          realtime.ts          #   AnalyserNode(fftSize=4096) + Meyda
          deterministic.ts     #   OfflineAudioContext precompute -> Float32Array[]
        sidecar.ts             #   load + time-sample the optional Python JSON sidecar

    backends/                  # one adapter per Backend tag (all lazy-loaded)
      tsl/                     #   @cherry/backend-tsl   (default; Three TSL 2D+3D)
      wgsl-compute/            #   @cherry/backend-wgsl  (raw GPUDevice; CPU/WASM fallback)
      ogl/                     #   @cherry/backend-ogl   (lean raw GLSL -> TextureNode)
      regl/                    #   @cherry/backend-regl  (functional data-viz)
      pixi2d/                  #   @cherry/backend-pixi  (shared-context; resetState+stencil)
      phaser/                  #   @cherry/backend-phaser(Phaser4 + phaser-box2d; rapier on export)
      milkdrop/                #   @cherry/backend-milkdrop (Butterchurn web / libprojectM native)
      sandbox/                 #   @cherry/backend-sandbox  (Worker/iframe message bridge; p5.js)

    mode-sdk/                  # @cherry/mode-sdk — authoring helpers + templates
      src/                     #   defineMode(), per-backend boilerplate, dev hot-reload host
      templates/               #   one starter per category (shader/3d/particles/game/...)

    modes/                     # the 225 modes, lazily imported; grouped by category
      classic-spectral/        #   bars, radial, waterfall, oscilloscope, vectorscope, chromagram, CQT ...
      preset-engines/          #   milkdrop (Butterchurn wrapper) + libprojectM (native-only)
      shaders/                 #   plasma, tunnel, fire, kaleido, raymarch ... (license-checked)
      particles/               #   GPU particle systems (wgsl-compute / tsl), pixi sprite fields
      threed/                  #   3D scenes, mesh deformers, terrains
      arcade/                  #   breakout, pong, runner, brick physics (phaser)
      runner-satisfying/       #   BPM-synced runners, marble/physics-satisfying (phaser+rapier)
      generative/              #   L-systems, reaction-diffusion, cellular, flow fields
      experimental/            #   ML/compute, feedback synths (Hydra-UX reimplemented natively)
      _shared/                 #   ashima webgl-noise (MIT), SDF prims (MIT), demofx refs (BSD-2)

    export/                    # @cherry/export — deterministic video pipeline
      src/
        webcodecs/             #   in-browser short-clip: VideoEncoder + mp4box.js (BSD-3)
        headless/              #   Puppeteer + HeadlessExperimental.beginFrame (Linux/Docker)
        mux/                   #   native ffmpeg invoke: ProRes 422 HQ master + H.264 CRF18
      Dockerfile               #   Linux export image (macOS beginFrame unreliable)

  apps/
    web/                       # primary: Vite static app -> Netlify/Vercel (COOP/COEP headers)
      src/ui/                  #   mode browser, scene editor, timeline, ?scene= permalink
    desktop/                   # Tauri 2.x shell (Win/macOS/Linux + iOS/Android)
      src-tauri/               #   Rust glue; bundles libprojectM (dynamic) + ffmpeg + export sidecar

  tools/
    sidecar-py/                # OPTIONAL offline pre-analysis CLI (NOT a runtime dep)
      pyproject.toml           #   librosa (ISC) + beat_this (MIT). NO madmom/essentia/aubio.
      cherry_analyze/          #   emits the sidecar JSON consumed by audio-bus/sidecar.ts
      schema.json              #   sidecar schema kept in lockstep with AudioFeatures.sidecar

  fixtures/
    audio/                     #   royalty-free clips for tests + golden-frame export checks
    golden-frames/             #   reference PNGs proving deterministic export reproducibility

  docs/
    AUTHORING.md               #   "write a mode in 20 lines" + per-backend guides
    CONTRACT.md                #   the ABI spec + semver policy
    LICENSE_POLICY.md          #   trap list (AGPL/GPL/LGPL/NC) + quarantine rules
```

## Tech choices

| Concern | Choice | Why |
|---------|--------|-----|
| **Core engine / renderer** | Three.js r184+ WebGPURenderer + TSL (MIT) | Only option natively unifying 2D+3D+shader authoring; TSL auto-compiles one source to WGSL (WebGPU) and GLSL (WebGL2 fallback) since r171; node-based RenderPipeline is built for modular pass composition. WebGPU-primary unlocks compute for particles/audio; WebGL2 floor ships everywhere. Avoid the legacy EffectComposer (WebGL-only, silently breaks under WebGPURenderer). |
| **Primary distribution** | Zero-install static web app via Vite -> Netlify/Vercel (COOP/COEP headers) | A URL is the whole product - fastest path to a music-channel audience and to contributors (git clone, open browser). JSON scenes give shareable ?scene= permalinks. GitHub Pages can't set COOP/COEP for SharedArrayBuffer, so Netlify/Vercel. |
| **Preset engine (instant large content)** | Butterchurn 2.6.7 pinned (MIT) + butterchurn-presets (MIT), wrapped as ONE 'milkdrop' backend | 15k+ Milkdrop presets via npm in ~10 min; gated behind butterchurn.isSupported() with WebGL2 fallback. Pin 2.6.7 (3.0.0-beta is preset-incompatible). Large packs (ansorre 15k/IA 52k) ship as optional user-loadable content flagged license-unverified, never bundled. |
| **Native Milkdrop (desktop only)** | libprojectM (LGPL-2.1) dynamically linked as a shared .dll/.so/.dylib in the Tauri build | Same 'milkdrop' contract on native. Dynamic linking discharges the LGPL relink obligation; static linking would not. Kept out of the web bundle entirely. |
| **Realtime audio features** | Web Audio AnalyserNode (fftSize=4096) + Meyda v5 (MIT) | Meyda is the only permissively-licensed JS lib covering the full MIR set (rms, centroid, flux, rolloff, chroma, mfcc). fftSize=4096 gives sub-100Hz resolution (separates C2/D2). Essentia.js (AGPL) and aubiojs (GPL WASM core) are excluded. |
| **Game / physics modes (arcade + runner)** | Phaser 4 (MIT) + phaser-box2d (MIT) realtime; @dimforge/rapier2d-deterministic (Apache-2.0) for export | 225 modes include arcade + satisfying-physics categories a shader stack can't host. Phaser4 shares the AudioContext and renders into an FBO TextureNode. matter.js cross-platform determinism is NOT guaranteed, so export uses Rapier-deterministic for bit-identical replay. |
| **Secondary renderers (escape hatches)** | PixiJS v8 (MIT, 2D/HUD), OGL (Unlicense, lean raw GLSL), regl (MIT, data-viz) - all lazy-loaded | Each fills a niche (2D sprites, <25KB raw-shader passes, functional data-viz) and composites back as a TextureNode. Lazy import keeps first-load <200KB gzip. Shared-context modes need strict resetState() + stencil:true discipline. |
| **Raw compute escape hatch** | Raw-WGSL 'wgsl-compute' backend on GPUDevice, gated by navigator.gpu, with CPU/WASM fallback | Heaviest modes (GPU FFT, large particle sim, ML) write WGSL directly for maximum throughput - the capability WebGL2 incumbents structurally lack - while degrading gracefully where WebGPU is absent (~29% mobile). |
| **Deterministic video export (full videos)** | Headless Chromium + HeadlessExperimental.beginFrame + native ffmpeg (prores_ks LGPL or NVENC/VideoToolbox) | Virtualizes the clock so time never advances until a frame is requested -> perfect A/V sync for any deterministic:true mode. Audio rendered separately via OfflineAudioContext -> WAV, muxed by sample. Linux/Docker (macOS ignores deterministic flags). Avoids the GPL libx264 trap. |
| **In-browser export (short clips)** | WebCodecs VideoEncoder/AudioEncoder + mp4box.js (BSD-3) in a Web Worker | Avoids ffmpeg.wasm's 25-128x penalty; good for <3min 1080p shareables. mp4box (BSD-3) is the npm muxer, distinct from GPAC (LGPL). ProRes is not available via WebCodecs - that stays native-only. |
| **Desktop wrapper** | Tauri 2.x (MIT/Apache-2.0) | <10MB installer, 30-50MB RAM vs Electron's 150-300MB; targets Win/macOS/Linux + iOS/Android from the same source. Export shells to a bundled Puppeteer/Chromium sidecar because the OS WebView doesn't expose CDP's HeadlessExperimental domain. |
| **Offline analysis (optional)** | Python CLI: librosa (ISC) + beat_this (MIT) -> JSON sidecar | Global BPM, key, beat/downbeat positions are unreliable from a short realtime window and are deterministic when precomputed. Out-of-band, never a runtime dependency. EXCLUDES madmom (CC-BY-NC-SA weights), Essentia (AGPL), aubio (GPL). |
| **Shader-effect primitive & references** | ISF (MIT spec/BSD tooling) preferred over Milkdrop DSL; ashima webgl-noise (MIT), glsl-sdf-primitives (MIT), demofx (BSD-2) | ISF lowers the contribution barrier and opens the license-checked Shadertoy corpus. Permissive primitives only - explicitly avoid LYGIA (Prosperity/NC), hg_sdf (CC-BY-NC), shader-web-background (GPL), and default-CC-BY-NC-SA Shadertoy shaders. |
| **Feedback / synth UX (Hydra-style)** | Re-implement natively with ping-pong WebGLRenderTarget / WebGPU storage textures | Gives the o0..o3 feedback-routing UX without importing hydra-synth (AGPL-3.0 - network copyleft hard block). |
| **License posture** | Entire shipped bundle MIT/BSD/Apache/Unlicense; copyleft & NC content quarantined | Permissive licensing is the single biggest adoption differentiator in a field littered with GPL/LGPL/AGPL/NC traps. Apache-2.0 (Rapier) noted for its patent clause; LGPL (libprojectM) only dynamically linked and native-only; preset/Shadertoy IP ambiguity handled as optional, flagged, never-bundled content. |

## Key architectural decisions

1. TypeScript + Three.js WebGPU/TSL is the core; Rust+wgpu native (Proposal 2) is rejected as base. The pool of adaptable code for 225 modes (Butterchurn, Shadertoy, three.js/Phaser/Pixi) and the zero-install/instant-share distribution model both live in the web stack; native loses decisively on discoverability and contributor onboarding for a music-channel product.
2. Python is an OPTIONAL offline pre-analysis sidecar (librosa + beat_this -> JSON), never the runtime, never an engine dependency. It supplies only what a short realtime browser window can't: global BPM, key, beat/downbeat positions. Engine runs fully without it. Excludes madmom/Essentia/aubio (NC/AGPL/GPL).
3. ONE Mode ABI (@cherry/contract, zero-dep, semver'd) spans all 9 categories. Every mode implements init/resize/update(features,dt)/render/dispose, declares a backend tag + audioPorts, and draws into a host-allocated offscreen RenderTarget. Host owns the single GPU context; modes never touch AudioContext or the renderer. apiVersion mismatches are rejected, not silently run.
4. Heterogeneous backends (tsl, wgsl-compute, ogl, regl, pixi2d, phaser, milkdrop, sandbox) coexist because each renders to an FBO/TextureView that the host composites as a TextureNode in the node-based RenderPipeline. This is the single mechanism that lets shader, 3D, particle, game, and preset modes share one interface.
5. Game/runner modes are first-class via a dedicated 'phaser' backend (Phaser4 + phaser-box2d realtime), not bolted on - required because the 225-mode set includes arcade and satisfying-physics categories. Export swaps in @dimforge/rapier2d-deterministic for bit-identical replay (matter.js determinism is not guaranteed cross-platform).
6. ONE AudioFeatures bus, TWO interchangeable drivers (realtime AnalyserNode+Meyda / deterministic OfflineAudioContext). Modes can't tell which is active, so the SAME mode code is frame-perfect in export and live in realtime. update() is pure wrt (features, seeded RNG state) - this single precondition makes any deterministic:true mode exportable.
7. Declared audioPorts drive LAZY feature extraction: the host only computes/uploads the channels some active mode requests, keeping the realtime path cheap. Combined with lazy backend imports, first-load stays <200KB gzip.
8. WebGPU-first with a guaranteed WebGL2 floor: TSL compiles one shader source to both backends, so modes are authored once; raw-WGSL compute modes get a CPU/WASM fallback. WebGL2 ships unconditionally (WebGPU ~71% mobile, Firefox-desktop flag-gated in 2026).
9. DSP correctness baked into the bus by default: fftSize>=4096, log-freq axis, dBFS, separate attack/release smoothing (not smoothingTimeConstant), peak-hold+gravity, texSubImage2D uploads, energy-vs-43-frame beat detection, Shadertoy 512x2 dB-scaled fftTex layout with documented caveats. Contributor modes look correct without re-deriving the perceptual math.
10. Three distribution targets from one repo, prioritized: (1) static web app (primary), (2) Tauri 2 desktop (only place dynamically-linked native libprojectM lives), (3) headless-Chromium deterministic export on Linux/Docker + WebCodecs for short clips. ProRes master + H.264 CRF18 for YouTube.
11. Uniformly permissive license posture (MIT/BSD/Apache/Unlicense) is a hard architectural constraint. Every flagged trap is routed around: hydra-synth/Essentia/audioMotion/Bemuse (AGPL) avoided; libx264 (GPL) avoided via prores_ks/NVENC/VideoToolbox; libprojectM (LGPL) dynamically linked + native-only; LYGIA/hg_sdf/Shadertoy-default (NC) excluded. Milkdrop preset packs and unverified Shadertoy shaders are optional, flagged, never-bundled content.
12. A 'scene' is JSON: an ordered list of {modeId, params, transition} with timeline automation curves and beat-triggered transitions operating on declared automatable params - the demoscene/game 'animator' idiom for free, and the basis for ?scene= permalinks.

## Competing proposals considered

### Proposal 1: Cherry — a web-first modular music visualizer (TypeScript + WebGPU/WebGL2 + Web Audio, with an optional Python pre-analysis sidecar)
- **Language:** TypeScript (strict), authored as ES modules, bundled with Vite. Plugin authors write TypeScript modules implementing a 3-method interface; shader-only contributors write GLSL/ISF or TSL and never touch the host. An optional Python 3.11+ helper exists purely as an offline CLI, not part of the runtime.
- **Core libs:** three.js r184+ (MIT) — PRIMARY engine. WebGPURenderer with automatic WebGL2 fallback owns the single rendering context. Internal shaders authored in TSL so they auto-compile to WGSL (WebGPU) and GLSL (WebGL2). The node-based RenderPipeline is the modular post-process/pass graph that every mode plugs into., Web Audio API (native, no dep) — AudioContext graph: source → analyser(s) → destination. AnalyserNode supplies raw FFT/waveform Uint8/Float arrays uploaded to the GPU as textures., Meyda v5 (MIT) — the ONLY permissively licensed JS lib covering the full MIR feature set. Used for rms, energy, spectralCentroid, spectralFlux, spectralRolloff, chroma, mfcc per frame. (Essentia.js is AGPL — excluded. aubiojs bundles GPL'd WASM — excluded from the default build.), butterchurn 2.6.7 (MIT) + butterchurn-presets (MIT) — wrapped as ONE built-in mode ('milkdrop') for instant access to hundreds of WebGL2 presets. Pinned to 2.6.7 (3.0.0-beta is preset-incompatible). Gated behind butterchurn.isSupported()., PixiJS v8 (MIT) — LAZY-LOADED secondary renderer for strictly-2D modes (spectrum bars, HUD, sprite/particle overlays) via the shared-context pattern. Not used for 3D., OGL (Unlicense) — LAZY-LOADED for ultra-lean raw-shader plugins that render a fullscreen texture composited back as a Three.js TextureNode (<25KB)., Phaser 4 (MIT) + phaser-box2d (MIT) — LAZY-LOADED for game-based modes (Breakout/runner). Built-in WebAudioSoundManager shares the AudioContext; Box2D bodies driven by audio features., @dimforge/rapier2d-deterministic (Apache-2.0) — LAZY-LOADED physics ONLY for deterministic video export of game modes (matter.js cross-platform determinism is not guaranteed)., ashima/webgl-noise (MIT) — GLSL noise primitive for plasma/fire/voxel modes. mrkite/demofx (BSD-2) and sandner-art/Audio-Shader-Studio (MIT code) used as ALGORITHM references for the audio-uniform contract and classic effects., Build/deploy: Vite (MIT) + Netlify or Vercel (free tier, can set COOP/COEP headers). mp4box.js (BSD-3) as the WebCodecs muxer for in-browser short-clip export., EXCLUDED ON PURPOSE (license traps): LYGIA (Prosperity, non-commercial), hg_sdf (CC-BY-NC), Hydra/hydra-synth (AGPL), audioMotion-analyzer/JUCE/Essentia (AGPL), shader-web-background (GPL), Bemuse (AGPL), projectM (LGPL — fine for native, avoided for the web core). Shadertoy shaders only adopted when an explicit MIT/CC0/BSD header is present; default CC-BY-NC-SA is a derivative-work trap.
- **Python verdict:** Your instinct to reach for Python is understandable but, for THIS product, Python is the wrong base and the right helper — keep it, just not on the critical path. Why not the base: the deliverable is a zero-install, instantly-shareable, real-time 60fps browser experience for a music channel. The largest pool of adaptable audio-reactive code (Shadertoy, Butterchurn, three.js, Phaser, the whole 512x2 FFT-texture convention) lives in the browser/GLSL world, not in Python. Real-time audio→GPU at <16ms latency, Web Audio's AnalyserNode, WebGPU/WebGL2 rendering, and a URL-is-the-app distribution model are all native to the web stack and would each be a fight to replicate from Python (you'd end up shipping a heavy desktop binary, losing zero-install and shareability — exactly the incumbent trap projectM/Astrofox fell into). A web mode-plugin can be a TS file a contributor drops in with no build step; a Python equivalent can't render in the browser at all. Where Python genuinely earns its place (optional, offline, never required to run the app): a small MIT/ISC-licensed CLI that pre-analyses a full track and emits a timestamped JSON sidecar — global BPM, musical key, beat positions, downbeats — using librosa (ISC) as primary and beat_this (MIT, ISMIR 2024) for high-accuracy beats/downbeats. These are precisely the features that are unreliable to compute from a short realtime browser window, and pre-computing them offline also makes them fully deterministic for export. Hard constraints: stay MIT/BSD/ISC — avoid madmom (CC-BY-NC-SA model weights), essentia (AGPL + commercial-license trap), and aubio (GPL-3). Python can ALSO optionally drive the headless ffmpeg render orchestration if a creator prefers it to Node. Net: web-first TypeScript core is non-negotiable for the stated goals; Python is a valuable, clearly-bounded offline pre-analysis/render sidecar — not the engine.
- **Pros:** Largest pool of adaptable code, exactly as requested: the 512x2 dB-scaled Shadertoy audio-texture layout makes the entire Shadertoy + Butterchurn shader corpus drop-in; Butterchurn (MIT) gives hundreds-to-thousands of Milkdrop presets via npm in minutes; three.js/Phaser/PixiJS/OGL each unlock their own large MIT ecosystems for 3D, game, 2D, and lean-shader modes — no other base language has this breadth of ready audio-reactive code.; Zero-install distribution and instant shareability: a URL is the whole product; JSON scenes make looks permalink-shareable and deep-linkable — ideal for a music channel.; Single codebase spans realtime AND frame-perfect export: the pure-render() + audio-clock contract plus OfflineAudioContext/headless-beginFrame gives guaranteed A/V sync without a separate engine.; Future-proof rendering: WebGPU-primary via three.js TSL with automatic WebGL2 fallback and compute-shader escape hatch — avoids the WebGL2 ceiling that forces incumbents (Butterchurn/modV) into eventual rewrites.; Clean, fully permissive license surface (MIT/BSD/Apache/Unlicense) — the single biggest adoption differentiator in a landscape littered with GPL/LGPL/AGPL traps; the architecture explicitly routes around every trap flagged in the research.; Real, versioned plugin ABI with a no-build-step happy path — the documented 3-method contract that the surveyed 'modular' projects (modV, VSXu, projectM) never shipped, which is what enables a community to add modes without touching core.; Game + demoscene idioms are first-class (Phaser/Box2D modes, ping-pong feedback, timeline automation, beat-triggered scene transitions) rather than bolted on.
- **Cons:** WebGPU is not universal (~82% desktop / ~71% mobile, Firefox-desktop flag-only as of mid-2026): every TSL shader must be validated on the WebGL2 fallback, and raw-WebGPU compute modes need a CPU/WASM fallback — real ongoing test burden.; Heterogeneous renderers sharing one GL context (Three + Pixi + Phaser) demand strict resetState()/stencil discipline; a missed reset causes silent artifacts, and resources can't be shared across renderers (extra texture copies).; Browser audio realtime path is inherently limited: AnalyserNode gives only smoothed dB-magnitude (no phase), no global BPM/key — pushing the most musically interesting features into the offline/Python path and a sidecar-loading dependency.; Preset/shader IP ambiguity is unavoidable: the Milkdrop corpus and most Shadertoy shaders lack clean permissive licenses, so the large-content story ships as optional user-loadable packs with disclaimers rather than bundled, and each adapted Shadertoy shader needs a manual license check.; TSL is verbose (method-chaining, no infix operators) — onboarding friction for plugin authors used to raw GLSL; mitigated but not eliminated by the RawShaderMaterial/OGL escape hatch (which is WebGL2-only).; macOS headless export is unreliable (deterministic-mode flags ignored), forcing Linux/Windows Docker for the high-quality render pipeline — a CI/infra cost.; Performance ceiling vs. native for very heavy scenes (1M+ sprites, large compute) is higher in the browser than a Vulkan/Metal native engine; acceptable for a music-channel visualizer but a real cap for extreme workloads.; Two-language maintenance: keeping the optional Python analyzer's sidecar schema in lockstep with the TS AudioFrame.sidecar type is an ongoing coordination cost.

### Proposal 2: Cherry Native: Rust + wgpu core with an embedded Python authoring runtime
- **Language:** Rust for the engine core (render loop, audio I/O, mode ABI, export), with Python 3.12 embedded via PyO3 as the optional scripting/authoring layer for mode plugins. Plugin authors can write a mode in pure WGSL (no host language), in Rust (compiled cdylib), or in Python (hot-reloaded).
- **Core libs:** wgpu (Apache-2.0/MIT) — primary rendering backend, Vulkan/Metal/DX12 native + WebGPU/WASM from one codebase; gives compute shaders for GPU-side audio analysis that WebGL2 incumbents cannot do, winit (Apache-2.0) — windowing/input, cross-platform native + web, cpal (Apache-2.0/MIT) — cross-platform audio I/O; on Windows it already exposes WASAPI loopback for system-audio capture, which is the painful gap on the Python side, rustfft (MIT/Apache-2.0) — SIMD FFT for the realtime DSP path, libprojectM 4.0.0 (LGPL-2.1) — linked as a SHARED library (.dll/.so/.dylib) via FFI for the optional Milkdrop-preset compatibility mode; v4 ships a stable pure-C API explicitly built for bindings, and shared linking discharges the LGPL relink obligation, projectm-eval (MIT) — standalone equation parser if preset math is needed without the full engine, PyO3 + maturin (Apache-2.0) — embed CPython for the Python mode-authoring path and hot reload, naga (part of wgpu, Apache-2.0/MIT) — WGSL/GLSL/SPIR-V cross-compile so ISF/Shadertoy-style GLSL fragment shaders can be ingested as modes, ffmpeg as an external dynamically-invoked binary (not linked) — ProRes via prores_ks (LGPL) and H.264 for the offline export muxer; keep it a separate process to avoid the libx264 GPL trap, Tauri or egui (MIT/Apache-2.0) for the control-panel UI shell — egui keeps everything in Rust/wgpu with zero web dependency
- **Python verdict:** Honest answer: your instinct toward Python is reasonable for prototyping but wrong as the engine foundation — Python should be an embedded authoring layer, not the core. The case FOR Python-as-core is real and not trivial: moderngl is a mature, stable OpenGL 4.3+ binding with working compute shaders; numpy + rustfft-equivalents (scipy/numpy FFT) make the DSP trivial; libprojectM 4.0's new pure-C API makes a clean ctypes/cffi binding genuinely easy now; and a Python contributor on-ramp is the lowest-friction way to get community modes. If the goal were a personal tool or a research-grade analyzer, Python+moderngl+sounddevice would be a defensible, fast-to-build choice. But three hard problems sink Python as the SHIPPING core for this specific product: (1) Distribution — PyInstaller's interpreter-extraction pattern triggers antivirus false-positives and produces large opaque binaries, and Briefcase cannot cross-compile (you build per-platform anyway) and targets GUI-toolkit apps, not a custom GPU render loop; packaging a Python+OpenGL app into something a stranger will trust and run is materially worse than shipping a single signed Rust binary. (2) Realtime audio — system-audio loopback, the feature a 'what you hear' visualizer needs, requires a PortAudio fork (PyAudioWPatch) on Windows because stock sounddevice lacks loopback; the audio path lands on patched/fragmented dependencies, whereas cpal has WASAPI loopback in-tree. (3) The render/DSP hot loop under the GIL — sustaining a 60fps render while a separate realtime audio-analysis thread runs is exactly where Python's GIL and per-frame allocation overhead bite; you end up writing the hot parts in C/Cython anyway, which is an argument for a native core to begin with. And moderngl is OpenGL-only — choosing it forecloses the WebGPU/compute and single-codebase-web advantages that are this project's strongest differentiators per the research. Recommendation: build the core in Rust+wgpu, and KEEP Python — embed it via PyO3 as the hot-reloadable mode-authoring tier so contributors get numpy and a 3-method interface with no build step, while the engine, audio I/O, ABI, and export stay native. You satisfy the Python instinct where it actually pays (authoring ergonomics, community on-ramp) without paying its costs where they'd hurt (distribution trust, loopback audio, GIL-bound 60fps).
- **Pros:** WebGPU/wgpu native from day one means real GPU COMPUTE shaders — GPU-side FFT reduction, particle simulation, and on-GPU beat features — which every WebGL2 incumbent (butterchurn, modV) structurally cannot do and would require a rewrite to gain; the landscape survey explicitly flags WebGL2 as a 2-3 year ceiling; Single wgpu codebase covers native desktop (Vulkan/Metal/DX12) AND a future WASM+WebGPU web build, occupying the desktop+web single-codebase niche the survey identifies as unoccupied; Deterministic export is trivial: the render loop is already native and frame-indexed, so no headless-Chromium beginFrame, no macOS deterministic-mode caveat, no captureStream/requestFrame fragility — pull AudioFrame[n], render, pipe to ffmpeg, mux by sample; A genuine versioned plugin ABI (Rust cdylib + C header) is the thing no incumbent ships — third parties compile modes independently, the real differentiator over modV/projectM/butterchurn 'modular' branding; Audio-to-visual latency budget (<16ms) is actually enforceable natively with a lock-free ring buffer; the survey notes projectM's SDL frontend has documented sync lag and CAVA disclaims accuracy; cpal gives cross-platform system-audio loopback (incl. Windows WASAPI) in the core toolchain — capturing 'what you hear' is first-class, whereas the browser cannot capture arbitrary system audio at all; Permissive (MIT/Apache) core with shared-linked LGPL libprojectM and out-of-process ffmpeg cleanly avoids the GPL/AGPL/LGPL-static traps that poison the field; Native binary = no interpreter-extraction antivirus false positives and no Electron 150-300MB RAM / compositor-latency ceiling
- **Cons:** Distribution loses decisively to the web on instant access: a native app is a download and an OS trust prompt, versus opening a URL — for a music-channel/VJ audience this is the single biggest adoption tax, and the web path (butterchurn) reaches thousands of presets in ~10 minutes; Code availability and contributor onboarding are harder: Rust toolchain + per-platform CI builds vs. 'git clone, npm install, open browser'; the WGSL and Python mode tiers help but the engine core still gates contributors on Rust; wgpu/WebGPU is still spec-evolving — occasional backwards-incompatible changes (confirmed for wgpu-py's tracking of the spec), so the core may need maintenance churn the mature OpenGL/WebGL2 stacks don't; No ready-made permissive preset ecosystem to bootstrap from: the web side gets 15k+ butterchurn-JSON presets free; the native side either links LGPL libprojectM (Milkdrop DSL lock-in, 5-15% preset breakage) or builds modes from scratch; Per-platform native builds, code signing, and macOS notarization are real recurring CI/release overhead the web deploy (Netlify/Vercel push) doesn't have; Smaller talent pool and fewer copy-paste examples for Rust+wgpu audio-visual work than for Three.js/WebGL; most demoscene/Shadertoy reference code is GLSL/JS and needs porting through naga; Embedding CPython via PyO3 adds binary size and a GIL boundary; the Python mode tier is for authoring convenience, not the hot path, and must be sandboxed/threaded carefully to not stall the render loop

### Proposal 3: Cherry — a TypeScript-core, WebGPU-first portable visualizer with a language-agnostic mode contract and a shared AudioFeatures bus (one codebase → web, Tauri desktop, headless video export)
- **Language:** TypeScript is the primary runtime language for the engine, mode SDK, and all three targets (web/desktop/headless). WGSL (via Three.js TSL, which auto-emits GLSL for the WebGL2 fallback) is the shader language. Rust appears only as thin glue in the Tauri shell and the ffmpeg sidecar invocation. Python is a strictly optional, out-of-band offline analysis tool (librosa + beat_this) that emits a JSON sidecar — it is never in the realtime path and never a dependency of the engine.
- **Core libs:** Three.js r184+ with WebGPURenderer + TSL (MIT) — single rendering-context owner; TSL auto-compiles to WGSL on WebGPU and GLSL on WebGL2 fallback, so modes are written once. Use the new node-based RenderPipeline for post passes (NOT the legacy EffectComposer, which is WebGL-only and silently breaks under WebGPURenderer)., PixiJS v8 (MIT) — lazy-loaded ONLY for 2D-sprite/HUD modes, sharing the Three.js GL context with strict resetState() discipline + stencil:true., OGL (Unlicense) and regl (MIT) — lazy-loaded escape-hatch backends for ultra-lean raw-shader and data-viz mode types; their output composites into the RenderPipeline as a TextureNode., Meyda (MIT) — realtime per-frame features (rms, energy, spectralCentroid, spectralFlux, spectralRolloff, chroma, mfcc) feeding the AudioFeatures bus., Web Audio AnalyserNode (built-in) — raw FFT/waveform for the bus; OfflineAudioContext for deterministic pre-render in export mode., Butterchurn 2.6.7 pinned (MIT) + butterchurn-presets (MIT) — wrapped as ONE built-in mode type (a MilkdropMode), not the core engine. isSupported() gate + WebGL2 fallback. Larger packs (ansorre 15k, IA 52k) are optional user-loadable content flagged 'license-unverified', never bundled., libprojectM (LGPL-2.1) — OPTIONAL native-only Milkdrop backend for the Tauri build, dynamically linked as a shared .dll/.so/.dylib (never static) so LGPL relink obligation is satisfied; behind the same MilkdropMode contract., Tauri 2.x (MIT/Apache-2.0) — desktop shell (<10MB installer, 30-50MB RAM). The OS WebView cannot do headless export, so export uses a bundled Puppeteer/Chromium sidecar., Puppeteer + HeadlessExperimental.beginFrame + native ffmpeg (prores_ks LGPL encoder, or platform h264_nvenc/h264_videotoolbox to dodge the libx264 GPL trap) — headless deterministic frame pull for video export., mp4box.js (BSD-3) + WebCodecs VideoEncoder/AudioEncoder — in-browser export path for short clips., Vite (MIT) — build/dev; Netlify or Vercel for static deploy with COOP/COEP headers., OPTIONAL OFFLINE (Python, out-of-band): librosa (ISC) + beat_this (MIT) for global BPM, key, beat/downbeat positions → JSON sidecar. madmom and Essentia are explicitly excluded (CC-BY-NC-SA models / AGPL).
- **Python verdict:** Python is the wrong base for this project, and the instinct to reach for it should be resisted — but it earns a small, well-defined supporting role. Every load-bearing asset in this ecosystem is JavaScript/TypeScript + GPU-shader centric: the entire reusable preset corpus runs through Butterchurn (JS/WebGL2), the only permissive realtime feature library is Meyda (JS), the rendering frontier (Three.js WebGPU/TSL, PixiJS, OGL, regl) is JS, the cross-target shell is Tauri (web runtime), and the frame-perfect export pipeline is built on headless Chromium + Web Audio's OfflineAudioContext + native ffmpeg. A Python core (e.g. moderngl/pygame/PyQt) would strand you from all of it: no Butterchurn presets, no AnalyserNode/Meyda bus, no Tauri/WebGPU portability, no zero-install web target, and a far weaker GPU/shader story. You'd be reimplementing the ecosystem instead of standing on it. Where Python genuinely wins is OFFLINE, OUT-OF-BAND analysis: librosa (ISC) and beat_this (MIT) compute global BPM, musical key, and downbeat positions far more accurately than anything you can do in a short realtime browser window, and they emit a JSON sidecar the TS engine loads at playback. That is Python's correct and only place here — a pre-analysis script, never the runtime, never a dependency of the engine, and explicitly avoiding the madmom (CC-BY-NC-SA models) and Essentia (AGPL) traps. So: TypeScript core, Python as an optional analysis sidecar.
- **Pros:** One mode written once runs on WebGPU and WebGL2, web/desktop/export — the TSL+shared-context design is the core of cross-target mode reuse, the explicit positioning gap no current project fills.; A real, semver'd plugin ABI (4-method lifecycle + declared audioPorts + JSON-schema params) lets third parties ship modes as standalone npm packages without touching core — happy path needs no build step.; Single AudioFeatures bus with two drivers means the SAME mode code is frame-perfect-deterministic in export and live in realtime; modes never know which driver is active.; Uniformly permissive licensing (MIT/Apache/BSD/Unlicense) removes the adoption barrier that throttles projectM (LGPL), audioMotion (AGPL), Wav2Bar (GPL) — every copyleft/NC trap from the research is quarantined or avoided.; Butterchurn-as-a-mode gives instant access to hundreds-to-thousands of Milkdrop presets without chaining the core architecture to ns-eel2/.milk lock-in.; DSP-correct defaults baked into the bus (log-freq axis, dBFS, attack/release smoothing, peak-hold, texSubImage2D, proper beat detection) so contributor modes look good without re-deriving the perceptual math.; WebGPU-first with a guaranteed WebGL2 floor avoids the forced-rewrite ceiling that WebGL2-only incumbents (Butterchurn, modV) face, while still shipping everywhere today.; ISF over Milkdrop DSL as the portable shader primitive lowers the contribution barrier and opens the (license-checked) Shadertoy corpus.
- **Cons:** Significant surface area: maintaining a stable ABI across 5+ backends (tsl/pixi/ogl/regl/wgsl/milkdrop) plus two audio drivers and three distribution targets is real, ongoing maintainer cost — the lone-maintainer burnout risk the research flags.; TSL is verbose (method-chaining, not infix) and unfamiliar; raw-GLSL/Shadertoy authors need onboarding, even with the RawShaderMaterial WebGL-only escape hatch.; Shared-context compositing between Three.js and PixiJS demands strict resetState() discipline and stencil:true; missed resets cause silent artifacts — a sharp edge for plugin authors.; Two Milkdrop backends (Butterchurn web / libprojectM native) behind one contract means GLSL-vs-native preset fidelity can diverge per target; some presets that render in one won't match the other.; WebGPU is still only ~71% mobile and Firefox-desktop flag-gated in 2026, so the WebGL2 fallback path must be tested and maintained in parallel indefinitely — not a write-once backend.; Headless deterministic export is Linux/Docker-only (macOS beginFrame unreliable) and Tauri's own WebView can't export, forcing a bundled Chromium sidecar that inflates the desktop story.; Preset/shader license ambiguity (no Milkdrop pack has a clean permissive license; ansorre pack has none stated) means large packs stay optional/unbundled and high-stakes commercial use still needs legal review.; Offline Python sidecar adds a second toolchain for the best beat/key features — optional, but a setup step for contributors who want those channels.

## Hardening backlog (from the completeness critic)

These must be designed into the ABI **before** scaling mode count — several are correctness bugs, not nice-to-haves:

- Define a first-class TransitionMode/compositor in the ABI: host owns N offscreen targets, transitions are themselves deterministic modes (crossfade, additive, displacement, luma-wipe) with their own params — this is what makes 'scene = [{modeId, transition}]' real and is a differentiator
- Ship a flagship 'Reactive Ring on Album Art' mode early (Phase 1) and bake square + 9:16 vertical export presets with safe-area guides — directly target the viral-shorts goal you state
- Move feature extraction into an AudioWorklet and add the missing AudioFeatures fields: beatPhase, tempo/bpm, bar/downbeatPhase, beatConfidence, outputLatencyMs (A/V sync), plus optional stems/MIDI/mic/camera ports — fixes both jank and groove-locking
- Standardize on ONE physics engine (Rapier 2D, fixed-timestep, deterministic) for BOTH preview and export so deterministic:true is actually true; drop the box2d-runtime/rapier-export split
- Add a mode-validator CLI + golden-frame regression harness: render deterministic modes to N reference frames, diff in CI with a perceptual tolerance, and gate PRs on it — this is the single biggest 'best-on-GitHub' credibility win for a community mode ecosystem
- Add a performance-budget API to ModeContext (frameBudgetMs, requestResolutionScale, qualityTier) and an automatic adaptive-quality governor so a heavy mode degrades gracefully instead of dropping frames
- Add a crash/panic boundary: wrap non-sandbox update()/render() in a watchdog that auto-disables a misbehaving mode, surfaces the error, and protects the shared context; treat dispose-leak detection as a CI gate
- Ship a built-in safety layer: flash-rate limiter (WCAG 2.3.1 / Harding-style), a prominent photosensitivity warning, and prefers-reduced-motion handling honored by the host compositor — protects users and the project legally
- Make licensing enforceable, not advisory: a build-time SPDX gate that refuses to bundle non-permissive preset/asset content, a clearly-curated permissively-licensed Milkdrop pack, and 'bring-your-own-preset/lyrics/art' UX so risky content is user-supplied only; get legal clarity on libprojectM (L)GPL before committing it to the Tauri binary
- Define the asset pipeline explicitly: an asset manifest field on modes, a resolver that works across web/Tauri/export, lazy fetching + caching, and a documented place for glTF/fonts/.lrc/presets — without this, half the appeal-5 modes can't ship
- Add a modulation matrix to ParamSpec (LFO/envelope/audio-source -> param routing with depth) on top of keyframes — turns static params into living visuals and massively increases perceived quality per mode
- Add the missing iconic modes as a curated 'Greatest Hits' starter pack (vinyl turntable, karaoke bouncing ball, Synthesia piano-roll, ferrofluid bass spikes, assistant orb, DVD-logo beat meme, infinite Mandelbrot autopilot, one rhythm-game note-highway) — these are what people screenshot and star
- Establish a third-party mode trust model: signed/locked manifests, a cherry.lock, a capability sandbox by default for community modes, and a registry/gallery site — so the 'drop a .ts file' promise is safe at scale
- Cut the catalog's critical path: pick ~30 'lighthouse' modes to polish to perfection for v1 and clearly mark the rest as community/long-tail, so breadth never dilutes the first impression
- Provide an explicit web audio-input UX (file drop + decode, mic, tab/system capture where supported) and document the streaming-DRM limitation up front, so the zero-install app is usable on landing

### Open architectural gaps to close

- No transition/compositor contract: 'scene = ordered modes with transition' is promised but the ABI never defines who owns the crossfade, how two modes' outputTargets blend, or a TransitionMode interface — this is core to the product and undefined
- AudioFeatures lacks continuous rhythm fields: only beat/beatCount/onset impulses exist, but smooth modes need beatPhase (0..1), tempo/BPM, bar/downbeat, and a beat-confidence — without phase, anything non-impulsive can't lock to the groove
- No A/V latency-compensation field: realtime AnalyserNode output lags the heard audio; there's no declared output-latency / sync-offset in AudioFeatures, so realtime beat sync will visibly drift
- No error/panic isolation for non-sandbox modes: if a 'tsl'/'phaser' mode throws in update()/render() it can take down the shared renderer; the ABI has no crash boundary, watchdog, or auto-disable contract
- No performance-budget / adaptive-quality API: modes can't query frame budget or request resolution-scale/FFT-size downgrades; with 225 modes and a shared context this guarantees frame drops with no graceful degradation path
- Determinism is under-specified across two physics engines: 'phaser-box2d at runtime, rapier2d on export' means preview and export use DIFFERENT solvers and WILL diverge — deterministic:true is a lie for arcade/runner modes
- Headless-Chromium export of WebGPU compute is not de-risked: SwiftShader/ANGLE lack robust WebGPU compute; without a real-GPU export node, every wgsl-compute and GPU-particle mode silently fails or falls back, breaking frame-perfect parity
- Feature extraction threading unspecified: Meyda/AnalyserNode on the main thread will jank the render loop — needs an AudioWorklet; the plan never commits to this
- Audio-input story missing for the zero-install web app: no contract for mic capture, tab/system audio, file-drop decode, or the DRM wall on streaming services — users may have no way to feed audio
- Asset pipeline undefined: glTF models, fonts, .lrc lyrics, album art, and Milkdrop/AVS preset packs have no declared location, fetch strategy, caching, or per-artifact (web/Tauri/export) resolution; no asset audioPort/manifest field
- No accessibility/safety contract: a flashing music visualizer needs a photosensitive-epilepsy (flash-rate) guard and prefers-reduced-motion handling — absent, and this is a real legal/ethical liability
- No vertical (9:16) or square (1:1) output story: 'viral shorts' is the stated goal but resolution is a single {w,h,dpr} with no aspect/safe-area presets for TikTok/Reels/Shorts export
- Contribution/quality infra missing: no mode-validator CLI, no golden-frame regression harness for deterministic modes, no perf CI gate, no third-party-mode trust/signing or registry — yet 'best modular visualizer on GitHub' depends on safe community modes
- Param ABI lacks modulation sources: params are keyframeable but there's no LFO/envelope/audio-modulation-matrix routing, no units, no log-scaling — limits the 'feels alive' factor and the automation story
- No mic/MIDI/stems/camera audioPorts despite catalog modes (ASCII Webcam, lyric modes, rhythm games) needing them — the declared port list contradicts the mode list
- Sandbox (p5/iframe/worker) compositing into a shared GPU outputTarget across the worker boundary is hand-waved — per-frame frame transfer is a correctness and perf hazard with no defined transport budget
