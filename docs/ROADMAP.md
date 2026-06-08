# Cherry — Roadmap

Wow-demo first, then scale breadth. Every phase is additive — Phase 0 proves the full vertical slice so nothing later is structural.

## Phase 0 — Engine Skeleton, Audio Pipeline & First Killer Mode

**Goal:** Stand up the complete repo scaffold, the AudioFeatures bus with both drivers (realtime AnalyserNode+Meyda and deterministic OfflineAudioContext stub), the semver'd Mode ABI, the Three.js r184+ WebGPURenderer+TSL primary renderer, and ship one end-to-end wow demo: Waveform Paddle Breakout. This phase proves the full vertical slice — audio in, ABI contract, physics, render out — so every later phase is additive, never structural.

**Deliverables:**
- Monorepo scaffold: packages/core, packages/renderer-three, packages/renderer-pixi (stub), packages/modes, apps/web, apps/desktop (stub), apps/export (stub)
- AudioFeatures bus: typed schema (rms, peak, spectralCentroid, bands[32], onsets, bpm, beatPhase) versioned at v1.0.0
- Realtime driver: AnalyserNode + Meyda, emitting AudioFeatures at animation-frame rate
- Deterministic driver: OfflineAudioContext stub returning static fixture data for CI
- Mode ABI v1.0.0: TypeScript interface ModePlugin { id, version, renderer, init(canvas, features$), tick(dt, features), dispose() }
- Three.js r184+ WebGPURenderer + TSL renderer backend, registered and lazy-capable
- Waveform Paddle Breakout mode: waveform-driven paddle position, physics via Matter.js (bundled inline for this mode), bricks reset on beat onset, TSL shaders for neon glow
- Static web app (Vite, zero-install): drag-and-drop audio file or mic input, canvas fullscreen, mode switcher stub
- CI: GitHub Actions running TS typecheck, unit tests for AudioFeatures bus, deterministic driver snapshot test
- Deployed preview URL (Netlify/Vercel) of the single Breakout mode

**Modes in this phase (1):** Waveform Paddle Breakout

**✅ Milestone:** M0 — Live URL shows Waveform Paddle Breakout running against a dropped MP3, AudioFeatures bus logs clean data in devtools, CI green

---

## Phase 1 — Easy Visual Wins: Synthwave, Classic Spectral & Radial Centerpieces

**Goal:** Add the highest-appeal, easy-to-medium modes that need no GPU compute and no external engine, proving the ABI with varied render styles and rapidly filling the mode switcher with crowd-pleasing visuals. Prioritize modes that look impressive in a short clip — ideal for social sharing. Also add the Scope Tube and Circular Radial Bars as the two other appeal-5 flagship modes alongside Breakout.

**Deliverables:**
- Mode plugin loader: dynamic import() per ModePlugin.id, hot-swap without page reload
- Synthwave Grid Drive: Three.js TSL grid mesh with perspective scroll speed driven by RMS, neon color palette keyed to spectral centroid
- Circular Radial Bars: FFT bands mapped to radial bar lengths, mirrored, bloom post-process via TSL
- Scope Tube (XY Phase Oscilloscope): raw time-domain L/R samples as line geometry, tube extrusion, color = phase angle
- Lissajous Vectorscope: 2D XY scatter of L vs R sample pairs, persistence trail via additive blending
- Logarithmic FFT Bars: classic bar chart, log-scale frequency, smoothed peaks, appeal-4 baseline
- Starfield Warp: instanced points, speed = RMS, onset = burst scatter
- Audio-Reactive Julia Set: fragment shader, c parameter driven by spectral centroid + bass
- Ball Pit Physics: Matter.js balls, radius = band amplitude, onset = spawn burst, appeal-5 easy
- Beat-Bounce Grow Ball: single ball radius oscillates with RMS, color = hue from BPM
- Perlin Flow Field: noise-displaced particles on 2D canvas fallback + Three plane, flow speed = RMS
- Kaleidoscope Mirror Fold: post-process pass folding the framebuffer N-symmetry, N driven by beat phase
- CRT / VHS Scanline Overlay: TSL post-process composited over any mode (first reusable overlay primitive)
- Chromatic Aberration Pulse: TSL offset on beat onset, reusable overlay
- De Jong Attractor Bloom: attractor constants driven by spectral features, plotted as additive point cloud
- Phyllotaxis Spiral: polar grid of instanced circles, radius and color keyed to bands
- Harmonograph Pendulum: parametric Lissajous trail, frequency ratio driven by detected BPM
- Superformula Bloom: superformula mesh morph on beat, TSL vertex displacement
- Fireworks on Onsets: burst particle emitter triggered on onset events, GPU instanced
- Glitter Snow Globe: instanced sphere particles, gravity + RMS turbulence, onset = shake
- Audio Pong Duel: two paddles tracking L/R RMS, ball speed = spectral centroid, appeal-4
- Mode switcher UI: thumbnail grid overlay, keyboard shortcut cycling, URL hash persistence

**Modes in this phase (20):** Synthwave Grid Drive, Circular Radial Bars, Scope Tube (XY Phase Oscilloscope), Lissajous Vectorscope, Logarithmic FFT Bars, Starfield Warp, Audio-Reactive Julia Set, Ball Pit Physics, Beat-Bounce Grow Ball, Perlin Flow Field, Kaleidoscope Mirror Fold, CRT / VHS Scanline Overlay, Chromatic Aberration Pulse, De Jong Attractor Bloom, Phyllotaxis Spiral (Sunflower), Harmonograph Pendulum, Superformula Bloom, Fireworks on Onsets, Glitter Snow Globe, Audio Pong Duel

**✅ Milestone:** M1 — Mode switcher shows 21 modes, all running in browser with no install. Synthwave Grid Drive and Circular Radial Bars posted as social clips. CI includes per-mode deterministic screenshot regression.

---

## Phase 2 — Preset Engines: Butterchurn/Milkdrop Batch + Medium Spectral Classics

**Goal:** Integrate Butterchurn as a lazy-loaded renderer backend, unlock the full Milkdrop preset catalog, and ship the hybrid overlay modes. Simultaneously backfill the medium-difficulty appeal-4 spectral/waveform classics. This phase delivers the most recognizable 'winamp-era' nostalgia modes and the highest passive-watch appeal.

**Deliverables:**
- Butterchurn renderer backend: lazy-loads butterchurn npm package, wraps in ModePlugin adapter, exposes preset list as AudioFeatures-reactive parameter
- Butterchurn Milkdrop Player: full preset catalog, beat-reactive preset auto-advance, appeal-5
- Butterchurn Beat-Reactive Preset Switcher: onset-triggered preset change with crossfade, appeal-4 easy
- Butterchurn Curated Playlist Mode: hand-curated preset sequence with per-song mapping, appeal-4
- Milkdrop + Lyrics Overlay Hybrid: Butterchurn background + WebVTT lyric track rendered as TSL text quads over the canvas, appeal-5
- Milkdrop + Album Art Warp Hybrid: album art texture fed into Butterchurn warp shader via custom preset injection, appeal-5 hard
- Milkdrop Custom Waveform Overlay: Butterchurn preset + custom waveform drawn via its internal waveform API, appeal-4
- Milkdrop Composite: Spectrum Bars + Preset Background: bars composited in screen-space over Butterchurn output, appeal-4 easy
- GeISS-Style Feedback Warp: feedback texture ping-pong with warp displacement driven by spectral centroid, appeal-4
- Shadertoy Preset Carousel: sandboxed WGSL port of 6 curated Shadertoy shaders, carousel auto-advances on beat, appeal-5
- Spectrogram Waterfall (Horizontal): rolling FFT texture updated each frame, appeal-4
- Waterfall Cascade (Vertical Drop): vertical scroll variant of waterfall, appeal-4
- Polar Spectrum Ring: radial version of logarithmic bars with fill, appeal-4
- Mirrored Waveform Tunnel: waveform extruded along Z as tunnel walls, appeal-4
- 3D Spectrum Ribbon: waveform history as ribbon mesh in 3D, appeal-4
- Radial Waveform Clock: clock-face waveform ring with hour/minute hands = BPM / beat phase, appeal-4
- Stereo Split Butterfly Spectrum: L spectrum mirrored to R, appeal-4
- Audio Tunnel Flythrough: procedural tunnel, radius driven by RMS, color by spectral centroid, appeal-5 medium
- Neon Cityscape Skyline: instanced building silhouettes, height = band amplitude, neon bloom, appeal-5
- Video Feedback Zoomer: framebuffer zoom+rotate each frame, color shift on onset, appeal-5
- Feedback Delay Mirror Room: multi-mirror feedback planes, reflection depth = beat count, appeal-5
- Slit-Scan Time Smear: horizontal slit of current frame appended to history texture, appeal-4
- Lyric input system: WebVTT parser, lyrics$ observable injected into AudioFeatures bus as optional sidecar

**Modes in this phase (21):** Butterchurn Milkdrop Player, Butterchurn Beat-Reactive Preset Switcher, Butterchurn Curated Playlist Mode, Milkdrop + Lyrics Overlay Hybrid, Milkdrop + Album Art Warp Hybrid, Milkdrop Custom Waveform Overlay, Milkdrop Composite: Spectrum Bars + Preset Background, GeISS-Style Feedback Warp, Shadertoy Preset Carousel, Spectrogram Waterfall (Horizontal), Waterfall Cascade (Vertical Drop), Polar Spectrum Ring, Mirrored Waveform Tunnel, 3D Spectrum Ribbon, Radial Waveform Clock, Stereo Split Butterfly Spectrum, Audio Tunnel Flythrough, Neon Cityscape Skyline, Video Feedback Zoomer, Feedback Delay Mirror Room, Slit-Scan Time Smear

**✅ Milestone:** M2 — Butterchurn player live with full preset catalog, Milkdrop+Lyrics hybrid demoed with a real song+SRT file, Shadertoy Carousel running 6 shaders. Mode count reaches ~40.

---

## Phase 3 — GPU Compute: Particles, Physics Simulations & 3D Scene Modes

**Goal:** Unlock WebGPU compute shaders (via TSL ComputeNode) for the heavy particle and physics modes. Add the OGL and regl lazy-loaded renderer backends for modes that benefit from leaner GL abstractions. Ship the high-appeal particle river, galaxy, and smoke modes, plus the remaining 3D scene modes.

**Deliverables:**
- TSL ComputeNode pipeline: generic GPGPU double-buffer helper, particle state struct, ping-pong dispatch abstraction
- Curl-Noise Particle River: 1M+ particles, curl noise velocity field, audio drives noise frequency and amplitude, appeal-5 hard
- Gravity-Well Galaxy: N-body approximation via Barnes-Hut on CPU + GPU particle integration, audio drives galaxy arm twist, appeal-5 hard
- GPU Galaxy N-Body: full O(N²) brute-force for smaller N on compute shader, appeal-5 hard
- Audio-Displaced Point Cloud: PLY/glTF point cloud loaded, vertices displaced by FFT band per-vertex, appeal-5 medium
- Smoke Plume Simulation: 3D texture advection on compute shader, RMS drives emitter rate, appeal-5 hard
- Lissajous Particle Spray: particles emitted along Lissajous curve, audio drives a/b ratio, appeal-5 medium
- Reaction-Diffusion Particle Seeding: Gray-Scott on compute, particle seeds triggered by onsets, appeal-5 hard
- Boids Flocking Flock: compute-shader boids, separation/alignment/cohesion weights driven by spectral features, appeal-4
- Spring-Mesh Cloth Bass Ripple: Verlet spring mesh on compute, bass band = ripple force, appeal-4 hard
- Magnetic Field Lines: field line tracer, pole positions driven by band amplitudes, appeal-4
- Granular Sand Pile: cellular automaton on compute, onset = pour event, appeal-4
- Liquid Crystal Defect Annihilation: Q-tensor PDE on compute, audio drives temperature parameter, appeal-4 hard
- Charged Particle Plasma: electrostatic N-body on compute, audio drives charge magnitude, appeal-4 hard
- SPH Water Splash: SPH fluid on compute, onset = splash event, appeal-4 hard
- Metaball Blob Swarm: metaball field on compute, rendered via marching squares, appeal-4
- Audio Slitscan Particle Sheet: particles sample slit-scan texture for color, appeal-4
- Terrain Flythrough: noise-generated terrain mesh, camera speed = RMS, appeal-4
- Reactive Wireframe Globe: icosphere wireframe, vertex displacement by band, appeal-4
- Exploding Geometry: glTF mesh shattered on onset via instanced fragment pieces, appeal-4
- Marching-Cubes Blob: scalar field on compute, isosurface extracted each frame, appeal-4 hard
- Waveform Ribbon: history of waveform samples as ribbon geometry, appeal-4
- Voronoi Crystal Fracture: Voronoi diagram on compute, onset = new crack, appeal-4 hard
- Low-Poly Landscape Morph: low-poly terrain morph target driven by band amplitudes, appeal-4
- Infinite Corridor: procedural corridor, scale = beat phase, texture = spectrogram, appeal-4
- Aurora Borealis Volume: volumetric raymarched auroras, color by spectral hue, appeal-5 hard
- Black Hole Accretion: Schwarzschild raymarched disk, accretion rate = RMS, appeal-5 hard
- Smoke/Fluid Simulation: 2D Navier-Stokes on compute (lighter variant), appeal-5 medium
- Mirror-Room Kaleidoscope: planar reflections in 3D, N planes driven by beat count, appeal-5 hard
- Particle Vortex Tornado: vortex attractor on compute, onset = vortex split, appeal-5 hard
- OGL renderer backend registered and lazy-loaded
- regl renderer backend registered and lazy-loaded

**Modes in this phase (30):** Curl-Noise Particle River, Gravity-Well Galaxy, GPU Galaxy N-Body, Audio-Displaced Point Cloud, Smoke Plume Simulation, Lissajous Particle Spray, Reaction-Diffusion Particle Seeding, Sand Mandala Erosion, Boids Flocking Flock, Spring-Mesh Cloth Bass Ripple, Magnetic Field Lines, Granular Sand Pile, Liquid Crystal Defect Annihilation, Charged Particle Plasma, SPH Water Splash, Metaball Blob Swarm, Audio Slitscan Particle Sheet, Terrain Flythrough, Reactive Wireframe Globe, Exploding Geometry, Marching-Cubes Blob, Waveform Ribbon, Voronoi Crystal Fracture, Low-Poly Landscape Morph, Infinite Corridor, Aurora Borealis Volume, Black Hole Accretion, Smoke/Fluid Simulation, Mirror-Room Kaleidoscope, Particle Vortex Tornado

**✅ Milestone:** M3 — Curl-Noise Particle River running at 60fps with 500K+ particles in Chrome Canary WebGPU. GPU compute pipeline documented. Mode count reaches ~68.

---

## Phase 4 — Demoscene Shaders, Generative Math, Glitch & Remaining Feedback Modes

**Goal:** Ship the raw-WGSL renderer backend for modes that need full shader authoring freedom outside TSL, then implement all the raymarched fractals, IFS attractors, reaction-diffusion, and the generative/mathematical art and glitch/feedback modes.

**Deliverables:**
- Raw-WGSL renderer backend: sandboxed WGSL module loader, uniform binding for AudioFeatures, hot-reload in dev
- Constant-Q Rainbow Spectrogram: CQT computed on AudioWorklet, visualized as rainbow heatmap, appeal-5 hard
- Mandelbulb Raymarch: DE raymarcher in WGSL, orbit trap coloring, audio drives power parameter, appeal-5 hard
- Kaleido IFS Mirror: IFS iteration in WGSL fragment shader, mirror symmetry, audio drives contraction, appeal-5 hard
- Gyroid SDF Scene: gyroid implicit surface raymarched, audio drives lattice scale, appeal-5 hard
- Mandelbox Fly-Through: Mandelbox DE raymarcher, camera flies inward, speed = RMS, appeal-5 hard
- Fractal Flame (IFS Attractor): GPU IFS in compute shader, histogram density estimation, appeal-5 hard
- Preset Morph Blender (Dual-Pipeline): two Butterchurn pipelines rendered to offscreen textures, blended with crossfade weight driven by audio energy, appeal-5 hard
- Milkdrop Song-Section Preset Playlist: beat-tracker detects sections (intro/verse/chorus), switches preset per section, appeal-5 hard
- Preset Roulette: Genre-Tagged Auto-DJ: genre classifier (OfflineAudioContext + pre-analyzed features) selects preset pool, auto-DJ randomizes within pool, appeal-5 hard
- ProjectM Preset Runner (WASM Bridge): projectM compiled to WASM, bridge feeding AudioFeatures, appeal-5 hard
- Gray-Scott Reaction-Diffusion: ping-pong compute, feed/kill parameters driven by spectral centroid/RMS, appeal-5 hard
- Lenia Continuous Cellular Automaton: continuous convolution CA on compute, kernel driven by audio, appeal-5 hard
- Menger Sponge / 3D Fractal Ray-March: iterative SDF, LOD controlled by RMS, appeal-5 hard
- Iterated Function System (IFS) Flame: second IFS variant with log-density color, appeal-5 hard
- AVS Classic Stack Emulator: emulates classic AVS superscope + dot plane pipeline, appeal-4 hard
- Menger Sponge IFS (2D): 2D cross-section render of Menger, audio drives iteration depth, appeal-4 hard
- Reaction-Diffusion (Gray-Scott) demoscene variant: visual-focus tuning distinct from the generative-math variant, appeal-4 hard
- Hyperbolic Tiling: Poincare disk tiling, tile colors driven by bands, appeal-4 hard
- Tunnel Warp: texture-warped tunnel using feedback + warp map, appeal-4 easy
- Voxel Landscape: voxel raycast via WGSL, terrain height = FFT, appeal-4 medium
- Domain-Warp Noise Field: domain-warped fbm rendered as heightmap, audio drives warp strength, appeal-4
- SDF Morphing Primitives: blend between SDF primitives, morph weight = beat phase, appeal-4
- Neon Grid / Tron Plane: grid plane with neon edge glow, camera banks on beat, appeal-4 easy
- Psychedelic Feedback Loop: color-rotated feedback zoom, appeal-4 easy
- Clifford Attractor Field: Clifford strange attractor, parameter driven by spectral features, appeal-5 medium
- Lorenz Strange Attractor: Lorenz ODE integrated on GPU, trail rendered as additive line, appeal-4
- Voronoi Shimmer: animated Voronoi with cell color driven by band, appeal-4
- Delaunay Triangulation Mesh: Delaunay over beat-placed points, edges glow on onset, appeal-4
- Chladni Plate: modal frequency patterns, resonance frequency driven by detected pitch, appeal-4
- Mandelbrot / Julia Set Explorer: smooth coloring, c driven by spectral centroid, appeal-4
- Turing Pattern Morphogenesis: two-chemical reaction-diffusion, audio drives ratio, appeal-4 hard
- Perlin Terrain (Topographic Scan): contour-line style top-down, noise speed = RMS, appeal-4
- Falling Sand / Powder Toy: cellular automaton, onset = drop event, appeal-4
- Conway's Game of Life (Beat-Seeded): beat places live cells, appeal-3 easy
- L-System Fractal Tree: L-system grammar, audio-driven branching angle and depth, appeal-3
- Barnsley Fern & IFS Variations: classic IFS, parameter randomized on onset, appeal-3 easy
- Datamosh Beat Smasher: simulated datamosh (motion vector scramble on beat), appeal-5 hard
- Kinetic Lyric Typography: lyrics words fly in/out with velocity from RMS, appeal-5 medium
- Pixel Sort on Beats: pixel sorting post-process triggered on onset, appeal-5 medium
- Tunnel of Text: 3D text quads rushing toward camera, speed = RMS, appeal-5
- Lyric Word Explosion: words explode outward on onset, appeal-5 hard
- Spectral Flame Kaleidoscope: IFS flame rendered into kaleidoscope mirror, appeal-5 hard
- Edge-Glow Wireframe: Sobel edge detect post-process, glow intensity = RMS, appeal-4
- Scanner Darkly Posterize: posterize + rotoscope post-process, appeal-4
- Glitch Block Corruptor: block-level pixel displacement on onset, appeal-4
- Infinite Zoom Fractal: self-similar texture zoom loop, appeal-4
- Reaction-Diffusion Skin: Gray-Scott tuned to skin-pattern parameters, audio drives feed rate, appeal-4 hard
- Neon Sign Flicker: neon tube geometries with flicker probability driven by audio noise floor, appeal-4
- Voronoi Shatter Glitch: Voronoi shatter post-process on onset, appeal-4 hard
- Halftone / Dithered Stipple: halftone post-process, dot size = RMS, appeal-3 easy
- Moire Optical Illusion Grid: two rotating grids, rotation speed = spectral features, appeal-3 easy
- Tape Warp / Wow & Flutter: pitch-warped audio visualization overlay, appeal-3 easy
- Retro Demoscene Scroller: classic sine-scroll text, appeal-3 easy
- Rotozoomer: classic rotozoom on beat-synced angle, appeal-2 easy
- Chromagram Wheel: chromagram sectors around a circle, appeal-4 hard
- Comb Filter Harmonic Ruler: harmonic series ruler visualization, appeal-4 hard

**Modes in this phase (56):** Constant-Q Rainbow Spectrogram, Mandelbulb Raymarch, Kaleido IFS Mirror, Gyroid SDF Scene, Mandelbox Fly-Through, Fractal Flame (IFS Attractor), Preset Morph Blender (Dual-Pipeline), Milkdrop Song-Section Preset Playlist, Preset Roulette: Genre-Tagged Auto-DJ, ProjectM Preset Runner (WASM Bridge), Gray-Scott Reaction-Diffusion, Lenia Continuous Cellular Automaton, Menger Sponge / 3D Fractal Ray-March, Iterated Function System (IFS) Flame, AVS Classic Stack Emulator, Menger Sponge IFS, Reaction-Diffusion (Gray-Scott), Hyperbolic Tiling, Tunnel Warp, Voxel Landscape, Domain-Warp Noise Field, SDF Morphing Primitives, Neon Grid / Tron Plane, Psychedelic Feedback Loop, Clifford Attractor Field, Lorenz Strange Attractor, Voronoi Shimmer, Delaunay Triangulation Mesh, Chladni Plate (Modal Frequency Patterns), Mandelbrot / Julia Set Explorer, Turing Pattern Morphogenesis, Perlin Terrain (Topographic Scan), Falling Sand / Powder Toy, Conway's Game of Life (Beat-Seeded), L-System Fractal Tree, Barnsley Fern & IFS Variations, Datamosh Beat Smasher, Kinetic Lyric Typography, Pixel Sort on Beats, Tunnel of Text, Lyric Word Explosion, Spectral Flame Kaleidoscope, Edge-Glow Wireframe, Scanner Darkly Posterize, Glitch Block Corruptor, Infinite Zoom Fractal, Reaction-Diffusion Skin, Neon Sign Flicker, Voronoi Shatter Glitch, Halftone / Dithered Stipple, Moire Optical Illusion Grid, Tape Warp / Wow & Flutter, Retro Demoscene Scroller, Rotozoomer, Chromagram Wheel, Comb Filter Harmonic Ruler

**✅ Milestone:** M4 — Mandelbulb Raymarch live at 60fps. Butterchurn dual-pipeline morph blender demoed. Mode count reaches ~130. Raw-WGSL backend documented with authoring guide.

---

## Phase 5 — Game-Based, Runner & Satisfying Modes (Phaser4 + Box2D Backend)

**Goal:** Register the Phaser4+Box2D lazy-loaded renderer backend and implement all the arcade, paddle/ball, runner, and satisfying-loop modes. These are the highest engagement-bait and most shareable modes for short-form video content.

**Deliverables:**
- Phaser4 + Box2D renderer backend: lazy-loads Phaser 4 and planck.js (Box2D port), wraps physics world in ModePlugin lifecycle, AudioFeatures injected as Phaser events each frame
- Waveform Breakout Paddle Mirror: waveform-mirrored paddle pair, bricks arranged in FFT pattern, appeal-5 medium
- Spectral Columns Arkanoid: columns of bricks, column height = band amplitude, appeal-5 medium
- Deformable Wall Breakout: soft-body brick wall deforms on ball impact and bass hits, appeal-5 hard
- Chromagram Paddle: paddle width = chromagram chord complexity, bricks = chromagram sectors, appeal-5 hard
- Frequency Flipper Pinball: full pinball table, flipper force = bass RMS, bumper layout = FFT peaks, appeal-5 hard
- Harmonic Peggle: pegs arranged in harmonic series ring, ball path influenced by pitch tracking, appeal-5 hard
- Beat Pinball: physics pinball, bumper lights triggered by onsets, appeal-5 hard
- Spectrum Brick City: city skyline of bricks, height = band, ball demolishes on impact, appeal-5 hard
- Bass Asteroid Field: asteroids with mass = band amplitude, player ship steers via keyboard, appeal-4
- Spectrum Invaders: invaders descend in FFT bar formation, player shoots on beat, appeal-4
- Rhythm Tetris: tetromino fall speed = BPM, piece type = onset count mod 7, appeal-4
- Bass Bounce Bricks: bricks inflate/deflate with bass band, break when amplitude threshold exceeded, appeal-4
- Waveform Wall Pong: waveform-shaped wall that ball bounces off, appeal-4
- Spectral Breakout Multiplier: score multiplier driven by spectral richness, appeal-4
- Stereo Galaga Formation: enemies in L/R stereo spectrum formation, appeal-4
- Tempo Tron Light Cycles: Tron-style light cycles, turn events on beat, appeal-4
- Beat Defender Castle: castle walls take damage proportional to bass hits, appeal-4 hard
- Centipede Frequency Chain: centipede body length = spectral centroid, appeal-4
- Missile Command Frequency Defense: incoming missiles triggered by frequency onsets, appeal-4
- Qix Spectral Painter: Qix area-fill game, fill speed = RMS, fuse = spectral centroid, appeal-4 hard
- Audio Pac-Man Maze: maze generated from FFT, Pac-Man speed = RMS, ghost speed = BPM, appeal-4 hard
- Onset Snake: snake grows on onset, direction from spectral centroid drift, appeal-3 easy
- Frogger Rhythm Lanes: lane speed = band amplitude, car density = RMS, appeal-3
- Beat-Gate Marble Run: marble track gates open on beat, track layout from FFT envelope, appeal-5
- Plinko / Galton Board: pegs arranged in log-frequency grid, ball drop on onset, appeal-5
- Paint Pour / Fluid Marbling: Navier-Stokes 2D ink pour, onset = pour drop, RMS = viscosity, appeal-5
- Liquid Fill Progress Ring: ring fills with liquid as song progresses, ripple on onset, appeal-5
- Rube Goldberg Chain Trigger: chain of physics events, each link triggered by a beat, appeal-5 hard
- Slime / Oobleck Press: non-Newtonian fluid sim, RMS = pressure, appeal-5 hard
- Kinetic Sand Cut: particle sand displaced by waveform blade, appeal-5 hard
- Waveform Terrain Runner: endless runner, terrain height = waveform, appeal-4
- ASMR Sorting Bars: bubble/merge sort bars with audio-driven comparison swap rate, appeal-4 hard
- Domino Chain Reaction: domino rigid bodies, onset = first push, appeal-4 hard
- Hydraulic Press Crush Loop: physics object crushed on beat, respawn cycle, appeal-4 hard
- Soap Bubble Lattice: soap film simulation between wire frame, onset = bubble pop, appeal-4 hard
- Magnet Ball Swarm: magnetic dipole attraction, poles flip on beat, appeal-4 hard
- Coin Pusher Arcade: coin pusher physics, coins dropped on onset, appeal-4
- Endless Tunnel Runner: tunnel walls = waveform, obstacle spawn rate = RMS, appeal-4
- Sand Clock / Hourglass Reset: hourglass particle sand, reset on section change, appeal-4
- Drum Machine Falling Blocks: blocks fall in drum-grid lanes on each hit, appeal-4
- Lava Lamp Blob Merge: metaball blobs rise and merge, RMS = heat, appeal-4
- Slot Machine Reel: reel spins on onset, payout on spectral match, appeal-4
- Newton's Cradle Beat Sync: rigid-body cradle, onset = first ball pull, appeal-4
- Spectrum Waterfall / Infinite Scroll: horizontal infinite scroll of spectrogram, appeal-3 easy
- Infinite Conveyor Sort: objects sorted on conveyor, swap triggered by band crossing threshold, appeal-3 hard

**Modes in this phase (45):** Waveform Breakout Paddle Mirror, Spectral Columns Arkanoid, Deformable Wall Breakout, Chromagram Paddle, Frequency Flipper Pinball, Harmonic Peggle, Beat Pinball, Spectrum Brick City, Bass Asteroid Field, Spectrum Invaders, Rhythm Tetris, Bass Bounce Bricks, Waveform Wall Pong, Spectral Breakout Multiplier, Stereo Galaga Formation, Tempo Tron Light Cycles, Beat Defender Castle, Centipede Frequency Chain, Missile Command Frequency Defense, Qix Spectral Painter, Audio Pac-Man Maze, Onset Snake, Frogger Rhythm Lanes, Beat-Gate Marble Run, Plinko / Galton Board, Paint Pour / Fluid Marbling, Liquid Fill Progress Ring, Rube Goldberg Chain Trigger, Slime / Oobleck Press, Kinetic Sand Cut, Waveform Terrain Runner, ASMR Sorting Bars, Domino Chain Reaction, Hydraulic Press Crush Loop, Soap Bubble Lattice, Magnet Ball Swarm, Coin Pusher Arcade, Endless Tunnel Runner, Sand Clock / Hourglass Reset, Drum Machine Falling Blocks, Lava Lamp Blob Merge, Slot Machine Reel, Newton's Cradle Beat Sync, Spectrum Waterfall / Infinite Scroll, Infinite Conveyor Sort

**✅ Milestone:** M5 — Phaser4+Box2D backend registered. Plinko/Galton Board and Rube Goldberg demoed as viral-format clips. All 200+ modes listed in switcher. Phaser4 backend documented.

---

## Phase 6 — Deterministic Video Export (Headless Chromium + OfflineAudioContext)

**Goal:** Ship the apps/export artifact: a Node.js CLI that drives headless Chromium via Puppeteer, feeds the deterministic OfflineAudioContext driver (pre-decoded audio, frame-locked tick), captures frames via chrome://screenshot or WebGPU readback, and muxes them into an MP4/WebM with ffmpeg. Output is bit-identical across runs on the same input.

**Deliverables:**
- apps/export package: Node CLI entry point, accepts --input audio.mp3 --mode mode-id --duration 30 --fps 60 --output out.mp4 --width 1920 --height 1080
- Deterministic OfflineAudioContext driver: fully implemented (Phase 0 had stub), decodes audio offline, computes AudioFeatures frame-by-frame at 1/fps intervals, emits synchronously
- Frame capture loop: Puppeteer page.evaluate() triggers mode tick, then GPUBuffer.mapAsync readback of framebuffer, writes raw RGBA PNG to temp dir
- ffmpeg mux: child_process spawn of ffmpeg, pipe PNGs via stdin or temp dir, output H.264/VP9, configurable CRF
- Python sidecar CLI (optional, out-of-band): packages/python-analysis/ with librosa + beat_this, outputs JSON sidecar at packages/python-analysis/output/{hash}.json, format documented in ABI
- JSON sidecar ingest: AudioFeatures bus optionally reads pre-analyzed sidecar instead of running Meyda, enables richer beat_this BPM accuracy for export mode
- Export CI job: GitHub Actions renders a 10-second fixture clip of Circular Radial Bars, compares frame hash to golden, fails on drift
- Export documentation: CLI reference, Docker image for hermetic ffmpeg environment
- All existing modes verified deterministic: any Math.random() calls replaced with seeded PRNG, timestamp-based logic replaced with dt accumulator

**✅ Milestone:** M6 — CLI renders a 60-second MP4 of Curl-Noise Particle River at 4K/60fps in under 10 minutes on a developer workstation. Export CI job runs on every PR. Python sidecar produces beat_this BPM for a test track.

---

## Phase 7 — Tauri 2 Desktop, Remaining Long-Tail Modes & Production Polish

**Goal:** Package the web app as a Tauri 2 desktop application (Windows/macOS/Linux), wire native audio system capture (WASAPI/CoreAudio via Tauri plugin), add the final long-tail modes not yet implemented, and reach production quality: settings persistence, mode favorites, keyboard shortcuts, accessibility, and a public release.

**Deliverables:**
- apps/desktop: Tauri 2 project wrapping apps/web as WebView, native menu bar with Open File, Audio Source selector, Export shortcut
- Native audio capture: tauri-plugin-audio (WASAPI on Windows, CoreAudio on macOS, PulseAudio on Linux) feeding samples into the existing AnalyserNode pipeline via a SharedArrayBuffer bridge
- System tray: minimize to tray, global hotkey for mode cycle
- Tauri updater: auto-update channel via GitHub Releases
- Remaining appeal-4/3 modes not yet shipped: Plasma Sine Field, Metaball Soup, Procedural Fire, Voronoi Crackle, Bokeh Light Field, Lissajous Surface Warp, Spherical Harmonic Blob, Cellular Automaton (Cyclic CA), Beat-Burst Emitter, Fountain Emitter Wall, Constellation Connector, DNA Helix Particle Wrap, Verlet Rope Forest, Reactive 3D Typography, Hypercube Projection, Parametric Surface Zoo, Solar System Orrery, Crystalline Lattice, DNA Double Helix, Fractal Dimension Zoom, Comet Trail Galaxy Zoom, Flow Field Ink Drop, VU Meter Wall, Bark-Scale Band Display, Stereo Correlation Meter, Frequency Domain Heatmap Grid, LED Peak Meter Columns, Spectral Centroid Tracer, Slit-Scan Waveform, Waveform Envelope Fill, Linear FFT Bars, Mirrored Spectrum, Classic Oscilloscope, ASCII Webcam Render, Truchet Tile Mosaic, Dragon Curve & Space-Filling Curves, Langton's Ant (Multi-Ant Colony), Epidemic / SIR Automaton, Phosphene Sine Field, XMMS/Infinity Plugin Replica, BeatDrop AVS-Style Color Map Cycler, MilkDrop Minimal, OpenMPT Scope View Presets, Plane9 Scene Import Mode, NSS/WhiteCap Style Organic Blob Presets, Electric Sheep Flam3 Fractal Preset Player, MilkDrop Reaction-Diffusion Preset Hybrid, libvisual Plugin Host (WASM), Frei0r Plugin Preset Stack, AVS Superscope Playground
- Dancing glTF Model: glTF with skeleton, BVH animation driven by beat phase and RMS per-bone, appeal-5 hard
- Mode favorites, search filter, and tag system in the switcher UI
- Settings panel: theme, default mode, audio source, export quality presets
- Keyboard shortcut customization
- WCAG 2.1 AA accessibility audit and fixes for all UI chrome
- GitHub Releases: web app static build, Windows MSI, macOS DMG, Linux AppImage
- Public documentation site (VitePress): architecture guide, Mode ABI authoring tutorial, Python sidecar guide, export CLI reference

**Modes in this phase (50):** Dancing glTF Model, Plasma Sine Field, Metaball Soup, Procedural Fire, Voronoi Crackle, Bokeh Light Field, Lissajous Surface Warp, Spherical Harmonic Blob, Cellular Automaton (Cyclic CA), Beat-Burst Emitter, Fountain Emitter Wall, Constellation Connector, DNA Helix Particle Wrap, Verlet Rope Forest, Reactive 3D Typography, Hypercube Projection, Parametric Surface Zoo, Solar System Orrery, Crystalline Lattice, DNA Double Helix, Fractal Dimension Zoom, Comet Trail Galaxy Zoom, Flow Field Ink Drop, VU Meter Wall, Bark-Scale Band Display, Stereo Correlation Meter, Frequency Domain Heatmap Grid, LED Peak Meter Columns, Spectral Centroid Tracer, Slit-Scan Waveform, Waveform Envelope Fill, Linear FFT Bars, Mirrored Spectrum, Classic Oscilloscope, ASCII Webcam Render, Truchet Tile Mosaic, Dragon Curve & Space-Filling Curves, Langton's Ant (Multi-Ant Colony), Epidemic / SIR Automaton, AVS Superscope Playground, XMMS/Infinity Plugin Replica, BeatDrop AVS-Style Color Map Cycler, MilkDrop Minimal (Waveform-Only Presets), OpenMPT / ModPlug Tracker Scope View Presets, NSS / WhiteCap Style Organic Blob Presets, Electric Sheep Flam3 Fractal Preset Player, MilkDrop Reaction-Diffusion Preset Hybrid, libvisual Plugin Host (WASM), Frei0r Plugin Preset Stack, Plane9 Scene Import Mode

**✅ Milestone:** M7 — Tauri 2 installers published on GitHub Releases for all three platforms. All 240+ modes registered in the switcher. Public docs site live. v1.0.0 tag cut.

---

