//! Cherry — a native, modular music visualizer with a desktop UI.
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

mod analysis;
mod audio;
mod export;
mod modes;
mod postfx;
mod style;
mod track;
mod view;

use std::path::PathBuf;
use std::sync::mpsc::{channel, Receiver, TryRecvError};

use egui_macroquad::egui;
use macroquad::prelude::*;

use analysis::{features_at, Analyser, FFT_LEN};
use audio::AudioEngine;
use export::{ExportSettings, Exporter};
use modes::breakout::Breakout;
use modes::railshooter::RailShooter;
use modes::scope::Scope;
use modes::spectrogram::Spectrogram;
use modes::spectrum::Spectrum;
use modes::starfield::Starfield;
use modes::surfer::Surfer;
use modes::{FrameCtx, Mode, Param, ParamKind};
use postfx::PostFx;
use track::Track;

const SHOT_FRAMES: u32 = 180;

/// The mode registry — the single source of truth for the picker, the factory,
/// and `MODE_COUNT`. Add a mode here (plus its `mod` line) and it appears
/// everywhere; nothing else to keep in sync.
const MODES: [fn() -> Box<dyn Mode>; 7] = [
    || Box::new(Breakout::new()),
    || Box::new(Spectrum::new()),
    || Box::new(Scope::new()),
    || Box::new(Spectrogram::new()),
    || Box::new(Starfield::new()),
    || Box::new(Surfer::new()),
    || Box::new(RailShooter::new()),
];
const MODE_COUNT: usize = MODES.len();

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

fn window_conf() -> Conf {
    Conf {
        window_title: "Cherry".to_owned(),
        window_width: 1320,
        window_height: 760,
        window_resizable: true,
        sample_count: 4,
        ..Default::default()
    }
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
    println!("Cherry — a native music visualizer where the audio plays the game.\n");
    println!("USAGE:\n  cherry [--file <audio>]            launch the desktop app\n");
    println!("OPTIONS:");
    println!("  --file <path>            open an audio file (mp3/wav/flac/ogg/m4a)");
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
    Loaded(Track),
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
            .add_filter("Audio", &["mp3", "wav", "flac", "ogg", "m4a"])
            .set_title("Open audio file")
            .pick_file()
        {
            None => LoadResult::Cancelled,
            Some(p) => match Track::from_file(&p) {
                Ok(t) => LoadResult::Loaded(t),
                Err(e) => LoadResult::Failed(e),
            },
        };
        let _ = tx.send(result);
    });
    LoadJob { rx }
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
    modes: Vec<(&'static str, &'static str)>,
    sel: usize,
    params: Vec<Param>,
    themes: Vec<&'static str>,
    theme: usize,
    loading: bool,
    exporting: bool,
    export_progress: f32,
    export_status: String,
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
    if let Some(f) = &args.file {
        if let Err(e) = audio.load_file(f) {
            eprintln!("could not load {}: {e}", f.display());
        }
    }
    for m in modes.iter_mut() {
        m.reset(audio.track());
    }

    if let Some(th) = args.theme {
        style::set_theme(th);
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
    let mut ui = UiState {
        tab: Tab::Modes,
        seeking: false,
        seek_value: 0.0,
        about_open: false,
        sidebar: true,
        export_res: 1080,
        export_fps: 60,
        banner: None,
    };

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
                let ctx = FrameCtx { wave: &window, feat: &feat, track: audio.track(), time: t, dt };
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

        // ---- headless CLI: one-shot PNG of a single frame (dev orientation) --
        if args.export_frame.is_some() {
            let img = export::render_preview(export_settings, make_mode(sel), audio.track(), 300);
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
                Ok(LoadResult::Loaded(t)) => {
                    audio.set_track(t);
                    audio.restart();
                    for m in modes.iter_mut() {
                        m.reset(audio.track());
                    }
                    last_t = 0.0;
                    if let Some(p) = postfx.as_mut() {
                        p.reset();
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
                modes: modes.iter().map(|m| (m.name(), m.about())).collect(),
                sel,
                params: modes[sel].params(),
                themes: style::themes().iter().map(|t| t.name).collect(),
                theme: style::current_theme(),
                loading: loading.is_some(),
                exporting: exporter.is_some(),
                export_progress: exporter.as_ref().map_or(0.0, |e| e.progress()),
                export_status: export_status.clone(),
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
                    actions.push(Action::SelectMode((sel + 1) % modes.len()));
                }
            }

            for a in actions {
                match a {
                    Action::OpenFile => {
                        // Don't swap the track out from under an active export.
                        if loading.is_none() && exporter.is_none() {
                            loading = Some(spawn_open());
                        }
                    }
                    Action::Quit => std::process::exit(0),
                    Action::ToggleFullscreen => {
                        fullscreen = !fullscreen;
                        set_fullscreen(fullscreen);
                    }
                    Action::ShowAbout => ui.about_open = true,
                    Action::SelectMode(i) => {
                        sel = i;
                        modes[sel].reset(audio.track());
                        if let Some(p) = postfx.as_mut() {
                            p.reset();
                        }
                    }
                    Action::SetParam(name, v) => modes[sel].set_param(name, v),
                    Action::TogglePause => audio.toggle_pause(),
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
                    Action::StartExport(settings) => {
                        if exporter.is_none() && save_dialog.is_none() {
                            pending_settings = Some(settings);
                            save_dialog = Some(spawn_save());
                            export_status = "Choose where to save…".into();
                        }
                    }
                    Action::CancelExport => {
                        exporter = None; // Drop tears down ffmpeg + temp file
                        audio.set_paused(false);
                        export_status = "Export cancelled.".into();
                    }
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
        } else {
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

            let ctx = FrameCtx { wave: &window, feat: &feat, track: audio.track(), time: t, dt };
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
                if m.button("About Cherry").clicked() {
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
                s.horizontal_wrapped(|tabs| {
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
                });
                s.separator();
                egui::ScrollArea::vertical().show(s, |s| match ui.tab {
                    Tab::Modes => tab_modes(s, data, actions),
                    Tab::Settings => tab_settings(s, data, actions),
                    Tab::Library => tab_library(s, data, actions),
                    Tab::Export => tab_export(s, ui, data, actions),
                });
            });
    }

    // ---- About window -----------------------------------------------------
    let mut about_open = ui.about_open;
    egui::Window::new("About Cherry")
        .open(&mut about_open)
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .show(ctx, |w| {
            w.heading("Cherry");
            w.label("A native, open-source music visualizer the song plays.");
            w.add_space(6.0);
            w.label("Open a track, pick a mode, and the audio plays the game.");
            w.add_space(8.0);
            w.label(egui::RichText::new("Shortcuts").strong());
            w.label(egui::RichText::new("Space  play/pause   ·   Tab  next mode").weak());
            w.label(egui::RichText::new("R  restart   ·   F  fullscreen").weak());
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
    for (i, (name, about)) in data.modes.iter().enumerate() {
        let selected = i == data.sel;
        let resp = ui.add(
            egui::Button::new(egui::RichText::new(*name).strong())
                .min_size(egui::vec2(ui.available_width(), 0.0))
                .selected(selected),
        );
        if resp.clicked() {
            actions.push(Action::SelectMode(i));
        }
        ui.label(egui::RichText::new(*about).weak().small());
        ui.add_space(8.0);
    }
}

fn tab_settings(ui: &mut egui::Ui, data: &UiData, actions: &mut Vec<Action>) {
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
    ui.add_space(8.0);
    ui.label(egui::RichText::new("mp3 · wav · flac · ogg · m4a").weak().small());
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
