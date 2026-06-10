# 🍒 Cherry

**A native, open-source music visualizer where the audio plays the game.**

Open a song. Pick a mode. Watch the music play it — no player, no controls,
no install, no server. One double-clickable executable, written in Rust.

| Waveform Breakout | Beat Runner |
|---|---|
| ![Breakout](docs/screenshots/breakout-rust.png) | ![Runner](docs/screenshots/runner-rust.png) |

## The modes

**Waveform Breakout** — breakout with no player and no paddle sprite: **the live
waveform IS the paddle.** It forms a deforming surface along the bottom of the
arena that bats the ball up with power taken from the music's loudness. The
ball breaks the bricks (each column lit by its own frequency band); strong
beats kick the ball; broken bricks grow back so the rally never ends.

**Beat Runner** — an endless runner whose jump physics are ported from
[Chromium's T-Rex runner](https://source.chromium.org/chromium/chromium/src/+/main:components/neterror/resources/dino_game/)
(BSD-licensed; gravity `0.6`, jump velocity `-10`, speed `6→13`, rescaled from
its 600×150 canvas). What makes it a visualizer: Cherry pre-analyzes the whole
track at load, so **every beat becomes an obstacle placed to arrive exactly on
the beat**, world speed follows the track's loudness curve, and the runner
jumps itself — apex timed to the beat. Nobody is holding the spacebar.

## Run it

```
cargo run --release            # build + launch
cargo run --release -- --file path\to\song.mp3
```

The binary lands at `target/release/cherry.exe` — copy it anywhere and
double-click it. Supports mp3, wav, flac, ogg, m4a.

**Controls:** `O` open a song · `1`/`2`/`Tab` switch mode · `Space` pause ·
`R` restart. With no song loaded, a built-in demo groove plays.

## How it works

```
src/
  main.rs          app shell: window, input, HUD, mode switching
  audio.rs         playback + the master clock (rodio, with a silent fallback)
  track.rs         decode to PCM + offline pre-analysis (beat grid, loudness)
  analysis.rs      per-frame FFT features (32 log bands, bass/mid/treble, rms)
  view.rs          fixed 16x9 world space, letterboxed; shared palette
  modes/
    mod.rs         the Mode trait — a mode is one file implementing it
    breakout.rs    waveform-paddle breakout (rapier2d physics)
    runner.rs      beat-synced T-Rex runner (ported kinematics, no physics dep)
```

The design that makes "the music plays the game" exact rather than reactive:
tracks are **pre-analyzed offline at load** (a beat grid with strengths, plus a
loudness curve at ~12 ms resolution), so modes can place things at *future*
beats instead of guessing in realtime. Every mode reads one `FrameCtx` — the
PCM window at the playhead, its spectral features, and that profile — and draws
in a fixed 16:9 world space. Adding a mode is one file plus one line in
`main.rs`.

Stack: [macroquad](https://github.com/not-fl3/macroquad) (window + 2D),
[rapier2d](https://rapier.rs) (physics), [rodio](https://github.com/RustAudio/rodio)
(decode + playback), [realfft](https://github.com/HEnquist/realfft) (spectrum),
[rfd](https://github.com/PolyMeilex/rfd) (native file dialog). All permissively
licensed.

## Roadmap

The bigger vision — a large catalog of game/demo-inspired modes — lives in
[docs/MODES.md](docs/MODES.md) (225 catalogued concepts with audio mappings and
sources to adapt). The other docs in `docs/` are research from an earlier web
prototype; the mode catalog and strategy remain the guiding documents, ported
mode by mode into this native app.

Headless capture for development/CI: `cherry --shot [breakout|runner]
[--file song]` renders 180 frames on a silent fixed clock and writes a PNG.

## License

MIT. Runner kinematics ported from Chromium's T-Rex runner (BSD-style license,
The Chromium Authors).
