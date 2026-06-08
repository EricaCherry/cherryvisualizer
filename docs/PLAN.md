# Cherry — Master Plan

A modular, open-source music visualizer where every visual is a swappable plugin, inspired
by games, demos, and the whole history of audio-reactive code. Drop in an audio file, pick a
mode, watch it react. Built to be the engine behind a music channel's drops **and** the
easiest visualizer on GitHub to contribute a new mode to.

This document is the decision record. The detail lives in:
- **[ARCHITECTURE.md](ARCHITECTURE.md)** — the Mode ABI, AudioFeatures bus, backends, repo layout, tech-choices table.
- **[MODES.md](MODES.md)** — the full catalog (225 modes + a curated "greatest hits" starter set), each with its audio mapping, a source to adapt, and its license.
- **[ROADMAP.md](ROADMAP.md)** — 8 phases, wow-demo first, then breadth.
- **[STRATEGY.md](STRATEGY.md)** — licensing, repo, contribution flow, and how the visualizer grows the channel.

---

## The decision you asked for: language & base

**Web-first TypeScript, not Python.** Your instinct was that Python would have the most code to
grab — that's true for *audio analysis*, but not for *visuals*, and visuals are 95% of this
project. Here's the honest breakdown the research converged on:

| | Web (TS + WebGL/WebGPU) | Python (moderngl/pygame) |
|---|---|---|
| Where the adaptable visual code lives | **Here.** Butterchurn (MIT) = 15k+ Milkdrop presets via `npm install` in ~10 min. Shadertoy, three.js, Phaser, PixiJS are all JS/GLSL. | Almost none. You'd reimplement everything. |
| Distribution | **A URL is the whole product.** Zero install — critical for advertising alongside music. | PyInstaller binaries, AV false-positives, large opaque downloads. |
| GPU rendering | Free and modern (WebGPU compute + WebGL2 floor). | Possible but clunky; GIL fights a 60fps render+analysis loop. |
| Contributor on-ramp | One `.ts` file, no build step on the happy path. | Python toolchain + native deps per platform. |
| Video export for the channel | Deterministic headless-Chromium → frame-perfect MP4. | Workable but you lose the shared-code story. |

**Where Python still wins, narrowly:** an *optional, out-of-band* CLI that pre-analyzes a full
track into a JSON sidecar — global BPM, musical key, beat/downbeat positions — using
`librosa` (ISC) + `beat_this` (MIT). These are exactly the features a short realtime browser
window can't compute reliably. The sidecar is **never** a runtime dependency; the engine runs
fully without it. (Hard rule: no `madmom`/`essentia`/`aubio` — they're CC-BY-NC / AGPL / GPL.)

**The stack, in one line:** TypeScript core · Three.js r184+ **WebGPURenderer + TSL** (auto-compiles
one shader to WGSL for WebGPU and GLSL for the WebGL2 fallback) · one **AudioFeatures bus** with
realtime + deterministic drivers · lazy per-backend renderers (TSL, raw-WGSL, OGL, regl, PixiJS,
Phaser4+physics, Butterchurn) · three artifacts from one repo: **zero-install web app → Tauri 2
desktop → headless deterministic video export.**

---

## Why this is more than "another visualizer"

The OSS landscape is full of either single-effect toys or monolithic apps (projectM frontends,
Astrofox, Plane9, MilkDrop3) that lost on discoverability and can't be extended without forking.
None offer a **semver'd plugin ABI** + a **shared audio feature bus** + **deterministic video
export** in one permissively-licensed package. That's the gap Cherry fills:

1. **One contract, 225+ modes.** Shader art, 3D scenes, GPU particle sims, *and* full physics
   games all implement the same `init / update(features, dt) / render / dispose` interface and
   composite through one render pipeline.
2. **The music plays the game.** Your original Breakout idea — waveforms forming the paddles that
   push the ball, bricks built from the spectrum — is the Phase 0 centerpiece, and a whole
   category of arcade/runner/"satisfying" engagement-bait modes follows it.
3. **Frame-perfect export.** Because `update()` is pure with respect to `(features, seeded RNG)`,
   any `deterministic` mode renders a bit-identical MP4 — so a channel drop is reproducible and
   the toolchain *is* the marketing.
4. **Permissive by construction.** Every shipped line is MIT/BSD/Apache/Unlicense; every
   copyleft/NC trap (libprojectM LGPL, Shadertoy NC, libx264 GPL…) is mapped and routed around.

---

## What the completeness critic changed

The research workflow's final critic pass caught real problems. These are now first-class plan
items, not afterthoughts (full lists in [ARCHITECTURE.md](ARCHITECTURE.md#hardening-backlog-from-the-completeness-critic)
and [STRATEGY.md](STRATEGY.md#risks-to-manage)):

- **Determinism bug:** the draft used Box2D for preview but Rapier for export — different solvers
  diverge, so `deterministic` would be a lie for ~20 game modes. **Fix: one engine (Rapier 2D,
  fixed timestep) for both.**
- **Missing groove lock:** the bus had only beat *impulses*, no continuous `beatPhase`/`bpm`/
  `downbeat`. **Fix: added; move feature extraction into an AudioWorklet so it can't jank the
  render loop.**
- **No transition/compositor contract:** "a scene is an ordered list of modes with transitions"
  was promised but undefined. **Fix: transitions are themselves deterministic modes the host owns.**
- **Safety + legal:** no photosensitive-epilepsy flash guard, no `prefers-reduced-motion`, no
  build-time license gate. **Fix: all three are baked into the host, not left to mode authors.**
- **~19 iconic modes were missing** — vinyl turntable, karaoke bouncing-ball lyrics, Synthesia
  piano-roll, ferrofluid bass spikes, the assistant-orb, a rhythm-game note highway, infinite
  Mandelbrot autopilot. **Fix: shipped as a curated "Greatest Hits" starter pack** (Appendix A in MODES.md).

---

## Scope discipline (the one real risk)

225 modes is a *catalog*, not a backlog to grind through. The dominant failure mode is breadth
diluting quality — dozens of half-finished modes. So:

- **Pick ~30 "lighthouse" modes** and polish them to perfection for v1. Everything else is
  clearly labeled community/long-tail and shipped as people build it.
- **Phase 0 ships exactly one mode end-to-end** (Waveform Breakout) to prove the whole vertical
  slice. Every later phase is additive.
- **A new mode must pass** a golden-frame deterministic regression test + a license check in CI
  before merge. Quality is gated, not hoped for.

---

## Success criteria

- **M0:** live URL plays Waveform Breakout against a dropped MP3; AudioFeatures bus logs clean data; CI green.
- **v1:** ~30 polished lighthouse modes, the web app, the deterministic export CLI producing a
  YouTube-ready MP4, `@cherry/mode-sdk` on npm, and a "write your first mode in 15 minutes" guide.
- **Flywheel:** a music drop ships with a Cherry render; the video description links the exact
  CLI command and the mode SDK; viewers become contributors; the best community mode ships in the
  next drop.
