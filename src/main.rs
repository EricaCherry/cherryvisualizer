//! Cherry Visualizer — a native, modular music visualizer with a desktop UI.
//!
//! The window has a normal application chrome (egui): a menu bar, a tabbed
//! sidebar (Modes / Settings / Library / Export), and a transport bar with a
//! seek slider and volume. The visualizer renders in the central viewport; the
//! UI is painted on top.
//!
//! CLI: `cherry [--file <audio>]`
//!      `cherry --shot [breakout|surfer] [--file <audio>]`  -> renders 180
//!      frames headlessly (no UI), saves shot-<mode>.png, exits.
//!      `cherry --gen-wav <path>`  -> writes a small test WAV and exits.

// DSP/draw loops here index several arrays in lockstep; the iterator+zip
// rewrites this lint suggests read worse than the indices.
#![allow(clippy::needless_range_loop)]

mod analysis;
mod audio;
mod config;
mod export;
mod material3d;
mod modes;
mod postfx;
mod style;
mod track;
mod view;

use std::path::PathBuf;
use std::sync::mpsc::{channel, Receiver, TryRecvError};

use egui_macroquad::egui;
use macroquad::prelude::*;

use analysis::{features_at, Analyser, FFT_LEN, WAVE_LEN};
use audio::AudioEngine;
use export::{ExportSettings, Exporter};
use modes::breakout::Breakout;
use modes::nebula::Nebula;
use modes::radial::Radial;
use modes::railshooter::RailShooter;
use modes::scope::Scope;
use modes::spectrogram::Spectrogram;
use modes::spectrum::Spectrum;
use modes::starfield::Starfield;
use modes::galaxy::Galaxy;
use modes::lava::Lava;
use modes::mirror::Mirror;
use modes::pills::Pills;
use modes::ring::RingFire;
use modes::surfer::Surfer;
use modes::terrain::Terrain;
use modes::tunnel::Tunnel;
use modes::vinyl::Vinyl;
use modes::{Category, FrameCtx, Mode, Param, ParamKind};
use postfx::PostFx;
use track::Track;

const SHOT_FRAMES: u32 = 180;

/// The mode registry — the single source of truth for the picker, the factory,
/// and `MODE_COUNT`. Add a mode here (plus its `mod` line) and it appears
/// everywhere; nothing else to keep in sync.
const MODES: [fn() -> Box<dyn Mode>; 17] = [
    || Box::new(Breakout::new()),
    || Box::new(Spectrum::new()),
    || Box::new(Scope::new()),
    || Box::new(Spectrogram::new()),
    || Box::new(Starfield::new()),
    || Box::new(Tunnel::new()),
    || Box::new(Radial::new()),
    || Box::new(Nebula::new()),
    || Box::new(Vinyl::new()),
    || Box::new(Mirror::new()),
    || Box::new(RingFire::new()),
    || Box::new(Galaxy::new()),
    || Box::new(Lava::new()),
    || Box::new(Pills::new()),
    || Box::new(Terrain::new()),
    || Box::new(Surfer::new()),
    || Box::new(RailShooter::new()),
];
const MODE_COUNT: usize = MODES.len();

/// Picker groups, in display order: the game-likes lead, then the traditional
/// visualizers. Shared by the Modes tab AND Tab-key cycling so the keyboard
/// walks the exact sequence the sidebar shows.
const GROUPS: [(Category, &str); 2] =
    [(Category::Game, "Game-likes"), (Category::Visualizer, "Visualizers")];

/// The mode after `sel` in the picker's DISPLAYED order (groups first, registry
/// order within each group).
fn next_mode(modes: &[Box<dyn Mode>], sel: usize) -> usize {
    let order: Vec<usize> = GROUPS
        .iter()
        .flat_map(|(c, _)| (0..modes.len()).filter(move |&i| modes[i].category() == *c))
        .collect();
    let pos = order.iter().position(|&i| i == sel).unwrap_or(0);
    order[(pos + 1) % order.len().max(1)]
}

/// Build a fresh instance of mode `i` (clamped). Used both to populate the live
/// picker and to give the exporter its own untouched copy of the selected mode.
fn make_mode(i: usize) -> Box<dyn Mode> {
    MODES[i.min(MODE_COUNT - 1)]()
}

/// Map a resolution shorthand (720/1080/1440/2160) + fps to export settings.
fn settings_from(res: u32, fps: u32) -> ExportSettings {
    let (width, height) = match res {
        720 => (1280, 720),
        1440 => (2560, 1440),
        2160 => (3840, 2160),
        _ => (1920, 1080),
    };
    ExportSettings { width, height, fps }
}

/// The settings as they stand RIGHT NOW. Every save path — the debounce,
/// File→Quit, the window-close request — snapshots through here, so no change
/// is ever newer than the file (saving the last-written `cfg` would silently
/// drop anything younger than the debounce tick).
fn live_cfg(cfg: &config::Config, audio: &AudioEngine, ui: &UiState) -> config::Config {
    config::Config {
        theme: style::current_theme(),
        volume: audio.volume(),
        sidebar: ui.sidebar,
        export_res: ui.export_res,
        export_fps: ui.export_fps,
        custom: ui.custom_anchors,
        recent: cfg.recent.clone(),
    }
}

/// Install the four custom-theme anchor colors as the Custom palette.
fn apply_custom(a: [[u8; 3]; 4]) {
    let col = |c: [u8; 3]| Color::new(c[0] as f32 / 255.0, c[1] as f32 / 255.0, c[2] as f32 / 255.0, 1.0);
    style::set_custom(style::palette_from_anchors(col(a[0]), col(a[1]), col(a[2]), col(a[3])));
}

fn window_conf() -> macroquad::conf::Conf {
    macroquad::conf::Conf {
        miniquad_conf: Conf {
            window_title: "Cherry Visualizer".to_owned(),
            window_width: 1320,
            window_height: 760,
            window_resizable: true,
            sample_count: 4,
            icon: Some(cherry_icon()),
            ..Default::default()
        },
        // The heightfield ground (Terrain / Rail Shooter) is one ~21k-index
        // mesh; macroquad's defaults (10k verts / 5k indices) silently CLAMP
        // anything bigger — dropped triangles and a per-frame console warning.
        draw_call_vertex_capacity: 32_768,
        draw_call_index_capacity: 65_536,
        ..Default::default()
    }
}

/// The taskbar/alt-tab icon, painted procedurally (no asset): two deep-red
/// cherries with cream speculars hanging from a small green stem.
fn cherry_icon() -> miniquad::conf::Icon {
    fn draw(s: usize) -> Vec<u8> {
        let mut px = vec![0u8; s * s * 4];
        let cherries = [(0.34f32, 0.66f32), (0.68, 0.72)];
        let (top_x, top_y) = (0.55f32, 0.10f32);
        let aa = 1.2 / s as f32; // one-ish pixel of edge smoothing
        let cov = |d: f32, r: f32| ((r - d) / aa).clamp(0.0, 1.0);
        // distance from p to the segment a-b (the stems)
        let seg = |px_: f32, py: f32, ax: f32, ay: f32, bx: f32, by: f32| {
            let (dx, dy) = (bx - ax, by - ay);
            let t = (((px_ - ax) * dx + (py - ay) * dy) / (dx * dx + dy * dy)).clamp(0.0, 1.0);
            ((px_ - ax - dx * t).powi(2) + (py - ay - dy * t).powi(2)).sqrt()
        };
        for y in 0..s {
            for x in 0..s {
                let (u, v) = ((x as f32 + 0.5) / s as f32, (y as f32 + 0.5) / s as f32);
                let mut c = [0.0f32; 4]; // premult-ish working color
                let mut put = |col: [f32; 3], a: f32| {
                    if a > 0.0 {
                        let na = a + c[3] * (1.0 - a);
                        for i in 0..3 {
                            c[i] = (col[i] * a + c[i] * c[3] * (1.0 - a)) / na.max(1e-6);
                        }
                        c[3] = na;
                    }
                };
                // stems first (cherries overlap them at the join)
                for (cx, cy) in cherries {
                    let d = seg(u, v, top_x, top_y, cx, cy - 0.16);
                    put([0.36, 0.52, 0.30], cov(d, 0.05));
                }
                // leaf at the stem top
                let dl = ((u - top_x - 0.09).powi(2) * 3.2 + (v - top_y - 0.03).powi(2) * 8.0).sqrt();
                put([0.42, 0.60, 0.34], cov(dl, 0.16));
                for (cx, cy) in cherries {
                    let d = ((u - cx).powi(2) + (v - cy).powi(2)).sqrt();
                    // body shaded darker toward the lower right
                    let shade = 1.0 - ((u - cx) + (v - cy) + 0.3).clamp(0.0, 0.6);
                    put([0.78 * shade, 0.16 * shade, 0.22 * shade], cov(d, 0.21));
                    // cream specular, offset to the key light
                    let ds = ((u - cx + 0.07).powi(2) + (v - cy + 0.08).powi(2)).sqrt();
                    put([0.93, 0.89, 0.81], cov(ds, 0.055));
                }
                let i = (y * s + x) * 4;
                for k in 0..3 {
                    px[i + k] = (c[k] * 255.0) as u8;
                }
                px[i + 3] = (c[3] * 255.0) as u8;
            }
        }
        px
    }
    let mut icon = miniquad::conf::Icon {
        small: [0; 16 * 16 * 4],
        medium: [0; 32 * 32 * 4],
        big: [0; 64 * 64 * 4],
    };
    icon.small.copy_from_slice(&draw(16));
    icon.medium.copy_from_slice(&draw(32));
    icon.big.copy_from_slice(&draw(64));
    icon
}

#[derive(Default)]
struct Args {
    shot: Option<String>,
    file: Option<PathBuf>,
    gen_wav: Option<PathBuf>,
    /// `--export <out.mp4>`: render the selected mode to a video and exit.
    export: Option<PathBuf>,
    /// `--export-frame <mode>`: dump a single PNG of the mode and exit (dev).
    export_frame: Option<String>,
    /// `--bench [mode]`: time update+draw per mode headless, print a table, exit.
    bench: Option<String>,
    /// `--mode <tag>`: start on (or export) the mode whose name contains <tag>.
    mode: Option<String>,
    theme: Option<usize>,
    res: Option<u32>,
    fps: Option<u32>,
}

fn parse_args() -> Args {
    let mut out = Args::default();
    let mut it = std::env::args().skip(1).peekable();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--shot" => {
                let mode = it.peek().filter(|v| !v.starts_with("--")).cloned();
                if mode.is_some() {
                    it.next();
                }
                out.shot = Some(mode.unwrap_or_else(|| "breakout".into()));
            }
            "--file" => out.file = it.next().map(PathBuf::from),
            "--gen-wav" => out.gen_wav = it.next().map(PathBuf::from),
            "--export" => out.export = it.next().map(PathBuf::from),
            "--export-frame" => {
                let mode = it.peek().filter(|v| !v.starts_with("--")).cloned();
                if mode.is_some() {
                    it.next();
                }
                out.export_frame = Some(mode.unwrap_or_else(|| "breakout".into()));
            }
            "--bench" => {
                let mode = it.peek().filter(|v| !v.starts_with("--")).cloned();
                if mode.is_some() {
                    it.next();
                }
                out.bench = Some(mode.unwrap_or_default());
            }
            "--mode" => {
                let m = it.peek().filter(|v| !v.starts_with("--")).cloned();
                if m.is_some() {
                    it.next();
                }
                out.mode = m;
            }
            "--theme" => out.theme = it.next().and_then(|v| v.parse().ok()),
            "--res" => out.res = it.next().and_then(|v| v.parse().ok()),
            "--fps" => out.fps = it.next().and_then(|v| v.parse().ok()),
            "--help" | "-h" => {
                print_help();
                std::process::exit(0);
            }
            other if other.starts_with('-') => {
                eprintln!("cherry: unknown flag '{other}' (try --help)");
            }
            _ => {}
        }
    }
    out
}

fn print_help() {
    println!("Cherry Visualizer — a native music visualizer where the audio plays the game.\n");
    println!("USAGE:\n  cherry [--file <audio>]            launch the desktop app\n");
    println!("OPTIONS:");
    println!("  --file <path>            open an audio file (mp3/wav/flac/ogg/m4a)");
    println!("  --mode <tag>             start on the mode whose name contains <tag>");
    println!("  --export <out.mp4>       render the selected mode to a video, then exit");
    println!("  --export-frame <mode>    dump one 1080p PNG of <mode>, then exit");
    println!("  --bench [mode]           time update+draw per mode, print a table, exit");
    println!("  --shot <mode|ui>         render a headless PNG (dev), then exit");
    println!("  --gen-wav <path>         write a small test WAV, then exit");
    println!("  --theme <0-5>            color theme (Dusk Encom/Sunset/Nyx/Oil/Forest/Ember)");
    println!("  --res <720|1080|1440>    export/preview resolution (default 1080)");
    println!("  --fps <30|60>            export/preview frame rate (default 60)");
    println!("  --help, -h               show this help");
    println!("\nModes: breakout, spectrum, oscilloscope, spectrogram, starfield, surfer");
}

enum LoadResult {
    Loaded(Track, PathBuf),
    Cancelled,
    Failed(String),
}

struct LoadJob {
    rx: Receiver<LoadResult>,
}

/// Open the native file dialog AND decode on one background thread, so the
/// window keeps rendering the whole time (no freeze while the dialog is up).
fn spawn_open() -> LoadJob {
    let (tx, rx) = channel();
    std::thread::spawn(move || {
        let result = match rfd::FileDialog::new()
            .add_filter("Audio", &AUDIO_EXTS)
            .set_title("Open audio file")
            .pick_file()
        {
            None => LoadResult::Cancelled,
            Some(p) => decode(p),
        };
        let _ = tx.send(result);
    });
    LoadJob { rx }
}

/// Decode a known path (recent-file click, drag-and-drop) on a background
/// thread — same contract as [`spawn_open`], no dialog.
fn spawn_load(path: PathBuf) -> LoadJob {
    let (tx, rx) = channel();
    std::thread::spawn(move || {
        let _ = tx.send(decode(path));
    });
    LoadJob { rx }
}

fn decode(p: PathBuf) -> LoadResult {
    match Track::from_file(&p) {
        Ok(t) => LoadResult::Loaded(t, p),
        Err(e) => LoadResult::Failed(e),
    }
}

const AUDIO_EXTS: [&str; 5] = ["mp3", "wav", "flac", "ogg", "m4a"];

fn is_audio(p: &std::path::Path) -> bool {
    p.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| AUDIO_EXTS.contains(&e.to_lowercase().as_str()))
}

/// Open the native "save as" dialog on a background thread; the channel yields
/// the chosen path (or None if cancelled).
fn spawn_save() -> Receiver<Option<PathBuf>> {
    let (tx, rx) = channel();
    std::thread::spawn(move || {
        let p = rfd::FileDialog::new()
            .add_filter("MP4 video", &["mp4"])
            .set_file_name("cherry.mp4")
            .set_title("Export video")
            .save_file();
        let _ = tx.send(p);
    });
    rx
}

#[derive(Clone, Copy, PartialEq)]
enum Tab {
    Modes,
    Settings,
    Library,
    Export,
}

/// What the UI asks the app to do (applied after the egui pass).
enum Action {
    OpenFile,
    /// Load a specific file (recent-list click / drag-and-drop).
    OpenPath(PathBuf),
    Quit,
    ToggleFullscreen,
    ShowAbout,
    SelectMode(usize),
    SetParam(&'static str, f32),
    TogglePause,
    Seek(f32),
    SetVolume(f32),
    RestartMode,
    SetTheme(usize),
    /// Custom-theme anchors: background, body, hero, highlight (sRGB).
    SetCustom([[u8; 3]; 4]),
    StartExport(ExportSettings),
    CancelExport,
}

/// Read-only snapshot handed to the UI builder.
struct UiData {
    track_name: String,
    pos: f32,
    dur: f32,
    paused: bool,
    volume: f32,
    fps: i32,
    modes: Vec<(&'static str, &'static str, Category)>,
    sel: usize,
    active: bool,
    params: Vec<Param>,
    themes: Vec<&'static str>,
    theme: usize,
    loading: bool,
    exporting: bool,
    export_progress: f32,
    export_status: String,
    /// Recently opened files, newest first.
    recent: Vec<PathBuf>,
}

/// UI-owned state that persists across frames.
struct UiState {
    tab: Tab,
    seeking: bool,
    seek_value: f32,
    about_open: bool,
    sidebar: bool,
    export_res: u32,
    export_fps: u32,
    /// Custom-theme anchor colors (background, body, hero, highlight), sRGB.
    custom_anchors: [[u8; 3]; 4],
    /// A transient top-of-screen toast (message, seconds left) for errors that
    /// would otherwise only hit stderr (file-open / export failures).
    banner: Option<(String, f32)>,
}

fn fmt_time(s: f32) -> String {
    let s = s.max(0.0);
    format!("{}:{:02}", (s / 60.0) as u32, (s % 60.0) as u32)
}

/// Force full alpha on a captured frame before saving a PNG. The feedback
/// composite leaves the frame's alpha < 1 (harmless on screen and in the MP4,
/// which ignore alpha) but it reads as washed-out when a PNG is viewed over
/// white. Saved screenshots should be opaque.
fn opaque(mut img: Image) -> Image {
    for px in img.bytes.chunks_mut(4) {
        px[3] = 255;
    }
    img
}

#[macroquad::main(window_conf)]
async fn main() {
    let args = parse_args();

    if let Some(path) = &args.gen_wav {
        match track::write_test_wav(path) {
            Ok(()) => println!("wrote {}", path.display()),
            Err(e) => eprintln!("failed to write {}: {e}", path.display()),
        }
        std::process::exit(0);
    }

    let headless = args.shot.is_some();
    let ui_shot = args.shot.as_deref() == Some("ui"); // capture the UI for dev verification
    let cli_export = args.export.is_some() || args.export_frame.is_some() || args.bench.is_some();
    let mut audio = AudioEngine::new(!(headless || cli_export));
    let mut analyser = Analyser::new(FFT_LEN);
    let mut modes: Vec<Box<dyn Mode>> = (0..MODE_COUNT).map(make_mode).collect();
    // For --shot/--export-frame, pick the mode whose name contains the tag.
    let tag = args.shot.clone().or_else(|| args.export_frame.clone());
    let mut sel = match tag.as_deref() {
        Some(t) if t != "ui" => {
            modes.iter().position(|m| m.name().to_lowercase().contains(t)).unwrap_or(0)
        }
        _ => 0,
    };
    // `--mode <tag>` picks the starting mode — and, with `--export`, WHICH mode
    // the CLI renders (previously the CLI could only export the first one).
    if tag.is_none()
        && let Some(m) = &args.mode {
            let ml = m.to_lowercase();
            match modes.iter().position(|x| x.name().to_lowercase().contains(&ml)) {
                Some(i) => sel = i,
                None => eprintln!("cherry: no mode matching '{m}' (see --help)"),
            }
        }

    // Persisted settings (theme/volume/chrome/recents). Interactive runs and
    // real --export renders read them (the video should match what the user
    // sees); the dev capture paths (--shot/--export-frame/--bench) ignore them
    // entirely so captures are reproducible across machines. Only interactive
    // runs ever write.
    let dev_capture = headless || args.export_frame.is_some() || args.bench.is_some();
    let mut cfg = if dev_capture { config::Config::default() } else { config::load() };
    let persist = !(headless || cli_export);
    audio.set_volume(cfg.volume);
    apply_custom(cfg.custom);

    if let Some(f) = &args.file {
        if let Err(e) = audio.load_file(f) {
            eprintln!("could not load {}: {e}", f.display());
        } else if persist {
            cfg.touch_recent(f);
            config::save(&cfg);
        }
    }
    for m in modes.iter_mut() {
        m.reset(audio.track());
    }

    // Interactive launch starts BLANK — no mode runs until the user picks one,
    // opens a file, or hits play. CLI render paths and a --file launch are active.
    let mut active = headless || cli_export || args.file.is_some();
    if !active {
        audio.set_paused(true);
    }

    // Theme: the CLI flag wins; otherwise the persisted choice (which is the
    // default for dev captures, per above).
    match (args.theme, dev_capture) {
        (Some(t), _) => style::set_theme(t),
        (None, false) => style::set_theme(cfg.theme),
        (None, true) => {}
    }

    let export_settings = settings_from(args.res.unwrap_or(1080), args.fps.unwrap_or(60));

    let mut window = vec![0.0f32; FFT_LEN];
    let mut last_t = 0.0f32;
    let mut frame = 0u32;
    let mut fullscreen = false;
    let mut loading: Option<LoadJob> = None;
    // Interactive export state.
    let mut exporter: Option<Exporter> = None;
    let mut save_dialog: Option<Receiver<Option<PathBuf>>> = None;
    let mut pending_settings: Option<ExportSettings> = None;
    let mut export_status = String::new();
    // Headless CLI export (created lazily on the first frame so GL is ready).
    let mut cli_exporter: Option<Exporter> = None;
    // The "alive" feedback pipeline (lazily sized to the window).
    let mut postfx: Option<PostFx> = None;
    // Settings are persisted on a ~1s debounce (volume drags coalesce).
    let mut cfg_timer = 0.0f32;
    let mut ui = UiState {
        tab: Tab::Modes,
        seeking: false,
        seek_value: 0.0,
        about_open: false,
        sidebar: cfg.sidebar,
        export_res: cfg.export_res,
        export_fps: cfg.export_fps,
        custom_anchors: cfg.custom,
        banner: None,
    };
    if persist {
        // Intercept the window's X / Alt+F4 so settings can be saved first
        // (handled at the top of the frame loop).
        prevent_quit();
    }

    // ---- headless CLI: per-mode CPU cost of update()+draw() ----------------
    if let Some(tag) = args.bench.clone() {
        let which: Vec<usize> = if tag.is_empty() {
            (0..modes.len()).collect()
        } else {
            modes
                .iter()
                .position(|m| m.name().to_lowercase().contains(&tag))
                .map(|i| vec![i])
                .unwrap_or_else(|| (0..modes.len()).collect())
        };
        let frames = 400u32;
        let warmup = 30u32;
        let dt = 1.0 / 60.0;
        println!("{:<14}{:>10}{:>10}{:>10}{:>9}", "mode", "upd ms", "draw ms", "p95 ms", "est fps");
        for &mi in &which {
            modes[mi].reset(audio.track());
            let mut upd_sum = 0.0f64;
            let mut draws: Vec<f64> = Vec::with_capacity(frames as usize);
            for f in 0..frames + warmup {
                let t = f as f32 * dt;
                let feat = features_at(&mut analyser, audio.track(), &mut window, t, (t - dt).max(0.0), dt);
                let ctx = FrameCtx { wave: &window[..WAVE_LEN], feat: &feat, track: audio.track(), time: t, dt };
                let t0 = std::time::Instant::now();
                modes[mi].update(&ctx);
                let t1 = std::time::Instant::now();
                modes[mi].draw(&ctx);
                let t2 = std::time::Instant::now();
                if f >= warmup {
                    upd_sum += (t1 - t0).as_secs_f64() * 1000.0;
                    draws.push((t2 - t1).as_secs_f64() * 1000.0);
                }
                next_frame().await;
            }
            let n = draws.len().max(1) as f64;
            let upd_m = upd_sum / n;
            let draw_m = draws.iter().sum::<f64>() / n;
            draws.sort_by(|a, b| a.total_cmp(b));
            let p95 = draws
                .get(((draws.len() as f64 * 0.95) as usize).min(draws.len().saturating_sub(1)))
                .copied()
                .unwrap_or(0.0);
            let total = (upd_m + draw_m).max(1e-3);
            println!(
                "{:<14}{:>10.3}{:>10.3}{:>10.3}{:>9.0}",
                modes[mi].name(),
                upd_m,
                draw_m,
                p95,
                1000.0 / total
            );
        }
        std::process::exit(0);
    }

    loop {
        let dt = if headless { 1.0 / 60.0 } else { get_frame_time().min(0.05) };

        // With prevent_quit() armed, closing the window surfaces here instead
        // of killing the process — save the live settings, then really quit.
        if persist && is_quit_requested() {
            config::save(&live_cfg(&cfg, &audio, &ui));
            std::process::exit(0);
        }

        // ---- headless CLI: one-shot PNG of a single frame (dev orientation) --
        if args.export_frame.is_some() {
            let frame = std::env::var("CHERRY_FRAME").ok().and_then(|s| s.parse().ok()).unwrap_or(300);
            let img = export::render_preview(export_settings, make_mode(sel), audio.track(), frame);
            img.export_png("export-frame.png");
            println!("wrote export-frame.png ({}x{})", img.width, img.height);
            std::process::exit(0);
        }

        // ---- headless CLI: render the whole track to a video, then exit ------
        if let Some(out) = &args.export {
            if cli_exporter.is_none() {
                match Exporter::start(export_settings, make_mode(sel), audio.track(), out.clone()) {
                    Ok(e) => {
                        println!("exporting {} frames to {}…", e.total(), out.display());
                        cli_exporter = Some(e);
                    }
                    Err(e) => {
                        eprintln!("export failed: {e}");
                        std::process::exit(1);
                    }
                }
            }
            clear_background(style::ink());
            if let Some(exp) = cli_exporter.as_mut() {
                match exp.step(audio.track(), 50) {
                    Some(Ok(p)) => {
                        println!("done: {}", p.display());
                        std::process::exit(0);
                    }
                    Some(Err(e)) => {
                        eprintln!("export failed: {e}");
                        std::process::exit(1);
                    }
                    None => {
                        if exp.frames_done() % export_settings.fps == 0 {
                            println!("  {:.0}%  ({}/{})", exp.progress() * 100.0, exp.frames_done(), exp.total());
                        }
                    }
                }
            }
            next_frame().await;
            continue;
        }

        // ---- poll the background decode job -------------------------------
        if let Some(job) = &loading {
            match job.rx.try_recv() {
                // A decode that lands MID-EXPORT must be discarded: the
                // exporter renders from the live track every frame, so swapping
                // it would desync the remaining video from the muxed audio.
                Ok(LoadResult::Loaded(..)) if exporter.is_some() => {
                    ui.banner = Some(("Track not loaded — an export is running.".into(), 6.0));
                    loading = None;
                }
                Ok(LoadResult::Loaded(t, path)) => {
                    audio.set_track(t);
                    audio.restart();
                    for m in modes.iter_mut() {
                        m.reset(audio.track());
                    }
                    last_t = 0.0;
                    active = true; // opening a file starts the visualizer
                    if let Some(p) = postfx.as_mut() {
                        p.reset();
                    }
                    if persist {
                        cfg.touch_recent(&path);
                        config::save(&cfg);
                    }
                    loading = None;
                }
                Ok(LoadResult::Failed(e)) => {
                    eprintln!("could not open file: {e}");
                    ui.banner = Some((format!("Couldn't open file: {e}"), 6.0));
                    loading = None;
                }
                Ok(LoadResult::Cancelled) => loading = None,
                Err(TryRecvError::Disconnected) => loading = None,
                Err(TryRecvError::Empty) => {}
            }
        }

        // ---- poll the export save-dialog; kick off the exporter -----------
        if let Some(rx) = &save_dialog {
            match rx.try_recv() {
                Ok(Some(path)) => {
                    // Export a fresh instance carrying the user's live params,
                    // so the running view is never disturbed.
                    let mut m = make_mode(sel);
                    for p in modes[sel].params() {
                        m.set_param(p.name, p.value);
                    }
                    let settings = pending_settings.take().unwrap_or(export_settings);
                    match Exporter::start(settings, m, audio.track(), path) {
                        Ok(e) => {
                            exporter = Some(e);
                            audio.set_paused(true);
                            export_status = "Rendering…".into();
                        }
                        Err(e) => {
                            ui.banner = Some((format!("Export failed: {e}"), 7.0));
                            export_status = format!("Export failed: {e}");
                        }
                    }
                    save_dialog = None;
                }
                Ok(None) => {
                    save_dialog = None;
                    pending_settings = None;
                    export_status = "Export cancelled.".into();
                }
                Err(TryRecvError::Disconnected) => save_dialog = None,
                Err(TryRecvError::Empty) => {}
            }
        }

        // Age out the transient error toast.
        if let Some((_, ttl)) = &mut ui.banner {
            *ttl -= dt;
            if *ttl <= 0.0 {
                ui.banner = None;
            }
        }

        // ---- UI pass (collect actions); skipped when headless -------------
        let mut wants_kb = false;
        if !headless || ui_shot {
            let data = UiData {
                track_name: audio.track().name.clone(),
                pos: audio.position(),
                dur: audio.duration(),
                paused: audio.is_paused(),
                volume: audio.volume(),
                fps: (1.0 / dt.max(1e-4)) as i32,
                modes: modes.iter().map(|m| (m.name(), m.about(), m.category())).collect(),
                sel,
                active,
                params: modes[sel].params(),
                themes: style::theme_names(),
                theme: style::current_theme(),
                loading: loading.is_some(),
                exporting: exporter.is_some(),
                export_progress: exporter.as_ref().map_or(0.0, |e| e.progress()),
                export_status: export_status.clone(),
                recent: cfg.recent.clone(),
            };
            let mut actions: Vec<Action> = Vec::new();
            egui_macroquad::ui(|ctx| {
                build_ui(ctx, &data, &mut ui, &mut actions);
                wants_kb = ctx.wants_keyboard_input();
            });

            // keyboard shortcuts (only when egui isn't capturing the keyboard)
            if !wants_kb {
                if is_key_pressed(KeyCode::Space) {
                    actions.push(Action::TogglePause);
                }
                if is_key_pressed(KeyCode::F) {
                    actions.push(Action::ToggleFullscreen);
                }
                if is_key_pressed(KeyCode::R) {
                    actions.push(Action::RestartMode);
                }
                if is_key_pressed(KeyCode::Tab) {
                    actions.push(Action::SelectMode(if active { next_mode(&modes, sel) } else { sel }));
                }
                if is_key_pressed(KeyCode::Right) {
                    actions.push(Action::Seek((audio.position() + 5.0).min(audio.duration())));
                }
                if is_key_pressed(KeyCode::Left) {
                    actions.push(Action::Seek((audio.position() - 5.0).max(0.0)));
                }
                if is_key_pressed(KeyCode::Up) {
                    actions.push(Action::SetVolume(audio.volume() + 0.05));
                }
                if is_key_pressed(KeyCode::Down) {
                    actions.push(Action::SetVolume(audio.volume() - 0.05));
                }
            }

            // Drag-and-drop: the first audio file dropped on the window loads.
            if loading.is_none() && exporter.is_none() && save_dialog.is_none() {
                let dropped = get_dropped_files();
                if let Some(p) = dropped.iter().filter_map(|f| f.path.clone()).find(|p| is_audio(p)) {
                    loading = Some(spawn_load(p));
                } else if !dropped.is_empty() {
                    ui.banner = Some(("Drop an audio file: mp3 · wav · flac · ogg · m4a".into(), 5.0));
                }
            }

            for a in actions {
                match a {
                    Action::OpenFile => {
                        // Don't swap the track out from under an active export
                        // (or one being configured in the save dialog).
                        if loading.is_none() && exporter.is_none() && save_dialog.is_none() {
                            loading = Some(spawn_open());
                        }
                    }
                    Action::OpenPath(p) => {
                        if loading.is_none() && exporter.is_none() && save_dialog.is_none() {
                            loading = Some(spawn_load(p));
                        }
                    }
                    Action::Quit => {
                        if persist {
                            config::save(&live_cfg(&cfg, &audio, &ui));
                        }
                        std::process::exit(0)
                    }
                    Action::ToggleFullscreen => {
                        fullscreen = !fullscreen;
                        set_fullscreen(fullscreen);
                    }
                    Action::ShowAbout => ui.about_open = true,
                    Action::SelectMode(i) => {
                        sel = i;
                        active = true;
                        modes[sel].reset(audio.track());
                        if let Some(p) = postfx.as_mut() {
                            p.reset();
                        }
                    }
                    Action::SetParam(name, v) => modes[sel].set_param(name, v),
                    Action::TogglePause => {
                        active = true; // play also starts the visualizer on a blank launch
                        audio.toggle_pause();
                    }
                    Action::Seek(t) => {
                        audio.seek(t);
                        last_t = t;
                        if let Some(p) = postfx.as_mut() {
                            p.reset();
                        }
                    }
                    Action::SetVolume(v) => audio.set_volume(v),
                    Action::RestartMode => {
                        audio.restart();
                        modes[sel].reset(audio.track());
                        last_t = 0.0;
                        if let Some(p) = postfx.as_mut() {
                            p.reset();
                        }
                    }
                    Action::SetTheme(i) => {
                        style::set_theme(i);
                        if let Some(p) = postfx.as_mut() {
                            p.reset(); // trails would otherwise be the old palette
                        }
                    }
                    Action::SetCustom(a) => {
                        apply_custom(a);
                        style::set_theme(style::theme_count() - 1); // re-activate Custom + rebake
                        if let Some(p) = postfx.as_mut() {
                            p.reset();
                        }
                    }
                    Action::StartExport(settings) => {
                        // Also refuse while a decode is in flight — it would
                        // land mid-export and have to be thrown away.
                        if exporter.is_none() && save_dialog.is_none() && loading.is_none() {
                            pending_settings = Some(settings);
                            save_dialog = Some(spawn_save());
                            export_status = "Choose where to save…".into();
                        }
                    }
                    Action::CancelExport => {
                        if let Some(e) = exporter.take() {
                            e.cancel(); // tears down ffmpeg AND sweeps the partial MP4
                        }
                        audio.set_paused(false);
                        export_status = "Export cancelled.".into();
                    }
                }
            }

            // Persist settings at most once a second, and only when changed.
            cfg_timer += dt;
            if persist && cfg_timer >= 1.0 {
                cfg_timer = 0.0;
                let now = live_cfg(&cfg, &audio, &ui);
                if now != cfg {
                    cfg = now;
                    config::save(&cfg);
                }
            }
        }

        // ---- advance audio + step/draw the visualizer scene ---------------
        if let Some(exp) = exporter.as_mut() {
            // Export owns the frame: it renders offscreen at the export size.
            set_default_camera();
            clear_background(style::ink());
            match exp.step(audio.track(), 12) {
                Some(Ok(p)) => {
                    export_status = format!("Saved {}", p.display());
                    exporter = None;
                    audio.set_paused(false);
                }
                Some(Err(e)) => {
                    ui.banner = Some((format!("Export failed: {e}"), 7.0));
                    export_status = format!("Export failed: {e}");
                    exporter = None;
                    audio.set_paused(false);
                }
                None => {
                    let e = exporter.as_ref().unwrap();
                    export_status = format!(
                        "Rendering {:.0}%  ({}/{} frames)",
                        e.progress() * 100.0,
                        e.frames_done(),
                        e.total()
                    );
                }
            }
        } else if active {
            audio.tick(dt);
            let t = audio.position();
            // Rewind guard stays at the call site (it resets the mode on loop).
            if t < last_t {
                last_t = 0.0;
                modes[sel].reset(audio.track());
                if let Some(p) = postfx.as_mut() {
                    p.reset();
                }
            }
            let feat = features_at(&mut analyser, audio.track(), &mut window, t, last_t, dt);
            last_t = t;

            let ctx = FrameCtx { wave: &window[..WAVE_LEN], feat: &feat, track: audio.track(), time: t, dt };
            if !audio.is_paused() {
                modes[sel].update(&ctx);
            }
            if modes[sel].own_background() {
                modes[sel].draw(&ctx); // Surfer: paints its own sky, drawn direct
            } else {
                // Everything else goes through the feedback pipeline.
                let (sw, sh) = (screen_width() as u32, screen_height() as u32);
                if postfx.as_ref().map(PostFx::size) != Some((sw, sh)) {
                    postfx = Some(PostFx::new(sw, sh));
                }
                postfx.as_mut().unwrap().render(&*modes[sel], &ctx, None);
            }
        } else {
            // Blank startup — just the chrome over a dark canvas until a mode runs.
            set_default_camera();
            clear_background(style::ink());
        }

        // ---- paint UI on top, or capture a headless shot ------------------
        if !headless || ui_shot {
            egui_macroquad::draw();
        }
        if headless {
            frame += 1;
            if frame >= SHOT_FRAMES {
                let name = if ui_shot {
                    "shot-ui.png".to_string()
                } else {
                    format!("shot-{}.png", modes[sel].name().to_lowercase().replace(' ', "-"))
                };
                opaque(get_screen_data()).export_png(&name);
                println!("wrote {name}");
                std::process::exit(0);
            }
        }

        next_frame().await
    }
}

/// egui theme keyed to the "Dusk Encom" palette so the app shell matches the
/// canvas (no default IDE-grey panels or blue selection).
fn cherry_visuals() -> egui::Visuals {
    use egui::Color32;
    let ink = Color32::from_rgb(11, 16, 20);
    let slate = Color32::from_rgb(17, 24, 29);
    let teal = Color32::from_rgb(63, 154, 160);
    let amber = Color32::from_rgb(224, 138, 60);
    let cream = Color32::from_rgb(236, 227, 207);
    let mut v = egui::Visuals::dark();
    v.panel_fill = slate;
    v.window_fill = ink;
    v.faint_bg_color = ink;
    v.extreme_bg_color = ink;
    v.override_text_color = Some(cream);
    v.hyperlink_color = teal;
    v.selection.bg_fill = teal.linear_multiply(0.45);
    v.selection.stroke = egui::Stroke::new(1.0, teal);
    v.widgets.hovered.weak_bg_fill = Color32::from_rgb(34, 48, 52);
    v.widgets.active.weak_bg_fill = amber;
    v.widgets.active.bg_fill = amber;
    v
}

fn build_ui(ctx: &egui::Context, data: &UiData, ui: &mut UiState, actions: &mut Vec<Action>) {
    ctx.set_visuals(cherry_visuals());
    // ---- menu bar ---------------------------------------------------------
    egui::TopBottomPanel::top("menu_bar").show(ctx, |bar| {
        egui::menu::bar(bar, |bar| {
            bar.menu_button("File", |m| {
                if m.button("Open audio file…").clicked() {
                    actions.push(Action::OpenFile);
                    m.close_menu();
                }
                m.separator();
                if m.button("Quit").clicked() {
                    actions.push(Action::Quit);
                    m.close_menu();
                }
            });
            bar.menu_button("View", |m| {
                m.checkbox(&mut ui.sidebar, "Sidebar");
                if m.button("Toggle fullscreen").clicked() {
                    actions.push(Action::ToggleFullscreen);
                    m.close_menu();
                }
            });
            bar.menu_button("Help", |m| {
                if m.button("About Cherry Visualizer").clicked() {
                    actions.push(Action::ShowAbout);
                    m.close_menu();
                }
            });
            bar.with_layout(egui::Layout::right_to_left(egui::Align::Center), |r| {
                // Surface export progress everywhere, not just on the Export tab.
                if data.exporting {
                    r.add(egui::ProgressBar::new(data.export_progress).desired_width(120.0).show_percentage());
                    r.label(egui::RichText::new("Exporting").strong());
                } else {
                    r.label(egui::RichText::new(format!("{} fps", data.fps)).weak());
                }
            });
        });
    });

    // ---- transport bar (bottom) ------------------------------------------
    egui::TopBottomPanel::bottom("transport").show(ctx, |t| {
        t.add_space(2.0);
        t.horizontal(|h| {
            if h.button(if data.paused { "  Play  " } else { "  Pause " }).clicked() {
                actions.push(Action::TogglePause);
            }
            h.label(fmt_time(if ui.seeking { ui.seek_value } else { data.pos }));

            // seek slider takes the remaining width minus the time + volume
            let mut sv = if ui.seeking { ui.seek_value } else { data.pos };
            let resp = h.add_sized(
                [h.available_width() - 210.0, 18.0],
                egui::Slider::new(&mut sv, 0.0..=data.dur.max(0.1)).show_value(false),
            );
            if resp.dragged() {
                ui.seeking = true;
                ui.seek_value = sv;
            }
            if resp.drag_stopped() {
                actions.push(Action::Seek(sv));
                ui.seeking = false;
            }
            h.label(fmt_time(data.dur));

            h.separator();
            h.label("Vol");
            let mut vol = data.volume;
            if h
                .add_sized([110.0, 18.0], egui::Slider::new(&mut vol, 0.0..=1.0).show_value(false))
                .changed()
            {
                actions.push(Action::SetVolume(vol));
            }
        });
        t.add_space(2.0);
    });

    // ---- sidebar (tabs + content) ----------------------------------------
    if ui.sidebar {
        egui::SidePanel::left("sidebar")
            .resizable(true)
            .default_width(290.0)
            .min_width(220.0)
            // Slightly translucent so the full-frame visual reads through the
            // chrome instead of looking hard-cropped on the left.
            .frame(
                egui::Frame::side_top_panel(&ctx.style())
                    .fill(egui::Color32::from_rgba_unmultiplied(15, 21, 26, 206)),
            )
            .show(ctx, |s| {
                s.add_space(4.0);
                s.horizontal(|tabs| {
                    for (label, tab) in [
                        ("Modes", Tab::Modes),
                        ("Settings", Tab::Settings),
                        ("Library", Tab::Library),
                        ("Export", Tab::Export),
                    ] {
                        if tabs.selectable_label(ui.tab == tab, label).clicked() {
                            ui.tab = tab;
                        }
                    }
                    tabs.with_layout(egui::Layout::right_to_left(egui::Align::Center), |r| {
                        if r.button("«").on_hover_text("Hide sidebar").clicked() {
                            ui.sidebar = false;
                        }
                    });
                });
                s.separator();
                egui::ScrollArea::vertical().show(s, |s| match ui.tab {
                    Tab::Modes => tab_modes(s, data, actions),
                    Tab::Settings => tab_settings(s, ui, data, actions),
                    Tab::Library => tab_library(s, data, actions),
                    Tab::Export => tab_export(s, ui, data, actions),
                });
            });
    } else {
        // Collapsed — a small floating button to bring the sidebar back.
        egui::Area::new(egui::Id::new("sidebar_expand"))
            .anchor(egui::Align2::LEFT_TOP, egui::vec2(6.0, 34.0))
            .show(ctx, |a| {
                if a.button("»").on_hover_text("Show sidebar").clicked() {
                    ui.sidebar = true;
                }
            });
    }

    // ---- About window -----------------------------------------------------
    let mut about_open = ui.about_open;
    egui::Window::new("About Cherry Visualizer")
        .open(&mut about_open)
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .show(ctx, |w| {
            w.heading("Cherry Visualizer");
            w.label(egui::RichText::new(concat!("v", env!("CARGO_PKG_VERSION"))).weak().small());
            w.label("A native, open-source music visualizer the song plays.");
            w.add_space(6.0);
            w.label("Open a track, pick a mode, and the audio plays the game.");
            w.add_space(8.0);
            w.label(egui::RichText::new("Shortcuts").strong());
            w.label(egui::RichText::new("Space  play/pause   ·   Tab  next mode").weak());
            w.label(egui::RichText::new("R  restart   ·   F  fullscreen").weak());
            w.label(egui::RichText::new("←/→  seek 5s   ·   ↑/↓  volume").weak());
            w.add_space(8.0);
            w.label(egui::RichText::new("MIT licensed · built with macroquad + egui").weak());
        });
    ui.about_open = about_open;

    // ---- loading overlay --------------------------------------------------
    if data.loading {
        egui::Window::new("loading")
            .title_bar(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |w| {
                w.horizontal(|h| {
                    h.spinner();
                    h.label("Loading…");
                });
            });
    }

    // ---- transient error toast --------------------------------------------
    if let Some((msg, _)) = &ui.banner {
        egui::Window::new("cherry_banner")
            .title_bar(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_TOP, [0.0, 44.0])
            .show(ctx, |w| {
                w.label(egui::RichText::new(msg).strong().color(egui::Color32::from_rgb(240, 180, 120)));
            });
    }
}

fn tab_modes(ui: &mut egui::Ui, data: &UiData, actions: &mut Vec<Action>) {
    ui.add_space(4.0);
    // The groups fill themselves from the registry: a mode lands under its
    // `Mode::category()`, empty groups vanish, and each group lays out as a
    // tile grid that adapts to the sidebar width (1 column when narrow, more
    // as the panel is dragged wider) instead of one long button list.
    for (cat, title) in GROUPS {
        let group: Vec<usize> = (0..data.modes.len()).filter(|&i| data.modes[i].2 == cat).collect();
        if group.is_empty() {
            continue;
        }
        ui.horizontal(|h| {
            h.label(egui::RichText::new(title).strong());
            h.label(egui::RichText::new(group.len().to_string()).weak().small());
        });
        ui.add_space(4.0);

        const MIN_TILE_W: f32 = 104.0;
        let spacing = ui.spacing().item_spacing.x;
        let avail = ui.available_width();
        let cols = (((avail + spacing) / (MIN_TILE_W + spacing)).floor() as usize).max(1);
        let tile_w = (avail - spacing * (cols as f32 - 1.0)) / cols as f32;
        for row in group.chunks(cols) {
            ui.horizontal(|h| {
                for &i in row {
                    let (name, about, _) = data.modes[i];
                    let resp = h
                        .add_sized(
                            [tile_w, 40.0],
                            egui::Button::new(egui::RichText::new(name).strong().size(13.0))
                                .wrap()
                                .selected(i == data.sel && data.active),
                        )
                        .on_hover_text(about);
                    if resp.clicked() {
                        actions.push(Action::SelectMode(i));
                    }
                }
            });
        }
        ui.add_space(8.0);
        ui.separator();
        ui.add_space(4.0);
    }
    // One-line description of the current mode (per-tile blurbs live on hover).
    if data.active
        && let Some((_, about, _)) = data.modes.get(data.sel) {
            ui.label(egui::RichText::new(*about).weak().small());
        }
}

fn tab_settings(ui: &mut egui::Ui, st: &mut UiState, data: &UiData, actions: &mut Vec<Action>) {
    // ---- theme (global, applies to every mode + exports) ------------------
    ui.add_space(4.0);
    ui.label(egui::RichText::new("Theme").strong());
    egui::ComboBox::from_id_salt("theme_combo")
        .selected_text(data.themes.get(data.theme).copied().unwrap_or("?"))
        .show_ui(ui, |ui| {
            for (i, name) in data.themes.iter().enumerate() {
                if ui.selectable_label(i == data.theme, *name).clicked() {
                    actions.push(Action::SetTheme(i));
                }
            }
        });
    // Custom palette pickers — only when the last theme (Custom) is selected.
    if data.theme + 1 == data.themes.len() {
        ui.add_space(6.0);
        ui.label(egui::RichText::new("Custom palette").weak().small());
        let mut changed = false;
        for (i, label) in ["Background", "Body", "Hero", "Highlight"].iter().enumerate() {
            ui.horizontal(|h| {
                if h.color_edit_button_srgb(&mut st.custom_anchors[i]).changed() {
                    changed = true;
                }
                h.label(*label);
            });
        }
        if changed {
            actions.push(Action::SetCustom(st.custom_anchors));
        }
    }
    ui.add_space(8.0);
    ui.separator();
    ui.add_space(4.0);

    let mode_name = data.modes.get(data.sel).map(|m| m.0).unwrap_or("");
    ui.label(egui::RichText::new(format!("{mode_name} settings")).strong());
    ui.add_space(6.0);
    if data.params.is_empty() {
        ui.label(egui::RichText::new("This mode has no adjustable settings yet.").weak());
    }
    for p in &data.params {
        let mut v = p.value;
        let slider = match p.kind {
            ParamKind::Int => egui::Slider::new(&mut v, p.min..=p.max).step_by(1.0).text(p.name),
            _ => egui::Slider::new(&mut v, p.min..=p.max).text(p.name),
        };
        if ui.add(slider).changed() {
            actions.push(Action::SetParam(p.name, v));
        }
    }
    ui.add_space(10.0);
    if ui.button("Restart mode").clicked() {
        actions.push(Action::RestartMode);
    }
}

fn tab_library(ui: &mut egui::Ui, data: &UiData, actions: &mut Vec<Action>) {
    ui.add_space(4.0);
    ui.label(egui::RichText::new("Now playing").strong());
    ui.label(&data.track_name);
    ui.add_space(10.0);
    if ui.button("Open audio file…").clicked() {
        actions.push(Action::OpenFile);
    }
    ui.add_space(4.0);
    ui.label(egui::RichText::new("mp3 · wav · flac · ogg · m4a — or drop a file on the window").weak().small());

    if !data.recent.is_empty() {
        ui.add_space(10.0);
        ui.separator();
        ui.add_space(4.0);
        ui.label(egui::RichText::new("Recent").strong());
        ui.add_space(4.0);
        for p in &data.recent {
            let name = p.file_name().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
            let resp = ui.add(
                egui::Button::new(egui::RichText::new(name).size(13.0))
                    .wrap()
                    .min_size(egui::vec2(ui.available_width(), 0.0)),
            );
            if resp.on_hover_text(p.display().to_string()).clicked() {
                actions.push(Action::OpenPath(p.clone()));
            }
        }
    }
}

fn tab_export(ui: &mut egui::Ui, st: &mut UiState, data: &UiData, actions: &mut Vec<Action>) {
    ui.add_space(4.0);
    ui.label(egui::RichText::new("Video export").strong());
    ui.add_space(4.0);
    ui.label(
        egui::RichText::new("Render the current mode to a 16:9 MP4 with the track muxed in.")
            .weak()
            .small(),
    );
    ui.add_space(8.0);
    ui.label(format!("Mode:  {}", data.modes.get(data.sel).map(|m| m.0).unwrap_or("?")));
    ui.add_space(10.0);

    ui.add_enabled_ui(!data.exporting, |ui| {
        ui.label("Resolution");
        ui.horizontal(|h| {
            for (label, val) in [("720p", 720u32), ("1080p", 1080), ("1440p", 1440)] {
                if h.selectable_label(st.export_res == val, label).clicked() {
                    st.export_res = val;
                }
            }
        });
        ui.add_space(6.0);
        ui.label("Frame rate");
        ui.horizontal(|h| {
            for (label, val) in [("30 fps", 30u32), ("60 fps", 60)] {
                if h.selectable_label(st.export_fps == val, label).clicked() {
                    st.export_fps = val;
                }
            }
        });
        ui.add_space(12.0);
        if ui
            .add(egui::Button::new(egui::RichText::new("Export MP4…").strong()))
            .clicked()
        {
            actions.push(Action::StartExport(settings_from(st.export_res, st.export_fps)));
        }
    });

    if data.exporting {
        ui.add_space(12.0);
        ui.add(egui::ProgressBar::new(data.export_progress).show_percentage());
        ui.add_space(4.0);
        if ui.button("Cancel").clicked() {
            actions.push(Action::CancelExport);
        }
    }

    if !data.export_status.is_empty() {
        ui.add_space(8.0);
        ui.label(egui::RichText::new(&data.export_status).weak().small());
    }

    ui.add_space(10.0);
    ui.separator();
    ui.label(
        egui::RichText::new(
            "Exporting renders as fast as it can and pauses playback. \
             Requires ffmpeg on your PATH.",
        )
        .weak()
        .small(),
    );
}
