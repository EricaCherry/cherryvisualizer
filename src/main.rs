//! Cherry — a native, modular music visualizer. The audio plays the game.
//!
//! Flow: a main menu to choose a track and a mode, then play. Decoding happens
//! on a background thread behind a loading bar, so opening a song never freezes
//! the window.
//!
//! CLI: `cherry [--file <audio>]`
//!      `cherry --shot [breakout|surfer] [--file <audio>]`  -> renders 180
//!      frames headlessly (skips the menu), saves shot-<mode>.png, exits.
//!      `cherry --gen-wav <path>`  -> writes a small test WAV and exits.

mod analysis;
mod audio;
mod modes;
mod track;
mod view;

use std::path::PathBuf;
use std::sync::mpsc::{channel, Receiver, TryRecvError};

use macroquad::prelude::*;

use analysis::Analyser;
use audio::AudioEngine;
use modes::{breakout::Breakout, surfer::Surfer, FrameCtx, Mode};
use track::Track;
use view::{BG, INK, INK_DIM};

const FFT_LEN: usize = 2048;
const SHOT_FRAMES: u32 = 180;
const HILITE: Color = Color::new(0.92, 0.50, 0.52, 1.0); // cherry red

fn window_conf() -> Conf {
    Conf {
        window_title: "Cherry".to_owned(),
        window_width: 1280,
        window_height: 720,
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
            _ => {}
        }
    }
    out
}

/// The decode job running on a background thread.
struct LoadJob {
    rx: Receiver<Result<Track, String>>,
    name: String,
    t0: f32,
}

enum Screen {
    Menu,
    Loading(LoadJob),
    Playing,
}

/// Kick off `Track::from_file` on a worker thread; returns a job to poll.
fn spawn_load(path: PathBuf, now: f32) -> LoadJob {
    let name = path
        .file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "track".into());
    let (tx, rx) = channel();
    std::thread::spawn(move || {
        let _ = tx.send(Track::from_file(&path));
    });
    LoadJob { rx, name, t0: now }
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
    let mut audio = AudioEngine::new(!headless);
    let mut analyser = Analyser::new(FFT_LEN);
    let mut modes: Vec<Box<dyn Mode>> = vec![Box::new(Breakout::new()), Box::new(Surfer::new())];
    let mut sel = 0usize;
    let mut error: Option<String> = None;
    let mut window = vec![0.0f32; FFT_LEN];
    let mut last_t = 0.0f32;
    let mut ui_time = 0.0f32;
    let mut frame = 0u32;

    let mut screen = if headless {
        if let Some(f) = &args.file {
            if let Err(e) = audio.load_file(f) {
                eprintln!("could not load {}: {e}", f.display());
            }
        }
        if args.shot.as_deref() == Some("menu") {
            audio.set_paused(true);
            Screen::Menu
        } else {
            sel = match args.shot.as_deref() {
                Some("surfer") => 1,
                _ => 0,
            };
            modes[sel].reset(audio.track());
            Screen::Playing
        }
    } else {
        if let Some(f) = &args.file {
            if let Err(e) = audio.load_file(f) {
                error = Some(e);
            }
        }
        audio.set_paused(true); // the menu is silent until you start
        Screen::Menu
    };

    loop {
        let dt = if headless { 1.0 / 60.0 } else { get_frame_time().min(0.05) };
        ui_time += dt;
        let mut goto: Option<Screen> = None;

        match &mut screen {
            // ---------------------------------------------------------------- menu
            Screen::Menu => {
                if is_key_pressed(KeyCode::Down) || is_key_pressed(KeyCode::S) {
                    sel = (sel + 1) % modes.len();
                }
                if is_key_pressed(KeyCode::Up) || is_key_pressed(KeyCode::W) {
                    sel = (sel + modes.len() - 1) % modes.len();
                }
                if is_key_pressed(KeyCode::Key1) {
                    sel = 0;
                }
                if is_key_pressed(KeyCode::Key2) && modes.len() > 1 {
                    sel = 1;
                }
                if is_key_pressed(KeyCode::O) {
                    if let Some(path) = rfd::FileDialog::new()
                        .add_filter("audio", &["mp3", "wav", "flac", "ogg", "m4a"])
                        .pick_file()
                    {
                        error = None;
                        goto = Some(Screen::Loading(spawn_load(path, ui_time)));
                    }
                }
                if is_key_pressed(KeyCode::Enter) || is_key_pressed(KeyCode::Space) {
                    audio.restart(); // un-pauses and plays from the top
                    modes[sel].reset(audio.track());
                    last_t = 0.0;
                    goto = Some(Screen::Playing);
                }

                let names: Vec<&str> = modes.iter().map(|m| m.name()).collect();
                draw_menu(&names, sel, &audio.track().name, &error);
            }

            // ------------------------------------------------------------- loading
            Screen::Loading(job) => {
                match job.rx.try_recv() {
                    Ok(Ok(t)) => {
                        audio.set_track(t); // stays paused (menu)
                        goto = Some(Screen::Menu);
                    }
                    Ok(Err(e)) => {
                        error = Some(e);
                        goto = Some(Screen::Menu);
                    }
                    Err(TryRecvError::Disconnected) => {
                        error = Some("decoder thread stopped unexpectedly".into());
                        goto = Some(Screen::Menu);
                    }
                    Err(TryRecvError::Empty) => {}
                }
                draw_loading(&job.name, ui_time - job.t0);
            }

            // ------------------------------------------------------------- playing
            Screen::Playing => {
                if !headless {
                    if is_key_pressed(KeyCode::Escape) || is_key_pressed(KeyCode::M) {
                        audio.set_paused(true);
                        goto = Some(Screen::Menu);
                    }
                    if is_key_pressed(KeyCode::Key1) {
                        sel = 0;
                        modes[sel].reset(audio.track());
                    }
                    if is_key_pressed(KeyCode::Key2) && modes.len() > 1 {
                        sel = 1;
                        modes[sel].reset(audio.track());
                    }
                    if is_key_pressed(KeyCode::Tab) {
                        sel = (sel + 1) % modes.len();
                        modes[sel].reset(audio.track());
                    }
                    if is_key_pressed(KeyCode::Space) {
                        audio.toggle_pause();
                    }
                    if is_key_pressed(KeyCode::R) {
                        audio.restart();
                        modes[sel].reset(audio.track());
                        last_t = 0.0;
                    }
                }

                audio.tick(dt);

                let t = audio.position();
                audio.track().window_at(t, &mut window);
                let mut feat = analyser.analyze(&window, audio.track().sr, dt);
                if t < last_t {
                    last_t = 0.0;
                    modes[sel].reset(audio.track());
                }
                feat.beat = audio.track().profile.beat_in(last_t, t);
                last_t = t;

                let ctx = FrameCtx { wave: &window, feat: &feat, track: audio.track(), time: t, dt };
                if !audio.is_paused() {
                    modes[sel].update(&ctx);
                }
                modes[sel].draw(&ctx);

                draw_text(&format!("Cherry · {}", modes[sel].name()), 14.0, 26.0, 24.0, INK);
                draw_text(&audio.status_line(), 14.0, screen_height() - 14.0, 18.0, INK);
                let help = "Esc menu   1/2/Tab mode   Space pause   R restart";
                let dim = measure_text(help, None, 18, 1.0);
                draw_text(help, screen_width() - dim.width - 14.0, screen_height() - 14.0, 18.0, INK_DIM);
            }
        }

        if let Some(s) = goto {
            screen = s;
        }

        if headless {
            frame += 1;
            if frame >= SHOT_FRAMES {
                let label = match &screen {
                    Screen::Menu => "menu".to_string(),
                    _ => modes[sel].name().to_lowercase().replace(' ', "-"),
                };
                get_screen_data().export_png(&format!("shot-{label}.png"));
                println!("wrote shot-{label}.png");
                std::process::exit(0);
            }
        }
        next_frame().await
    }
}

fn text_center(s: &str, cx: f32, y: f32, size: u16, color: Color) {
    let dim = measure_text(s, None, size, 1.0);
    draw_text(s, cx - dim.width / 2.0, y, size as f32, color);
}

fn draw_menu(mode_names: &[&str], sel: usize, track_name: &str, error: &Option<String>) {
    clear_background(BG);
    let (sw, sh) = (screen_width(), screen_height());
    let cx = sw * 0.5;

    text_center("CHERRY", cx, sh * 0.20, 66, INK);
    text_center("a music visualizer the song plays", cx, sh * 0.20 + 34.0, 20, INK_DIM);

    text_center(&format!("Track:   {track_name}"), cx, sh * 0.41, 26, INK);
    text_center("press  O  to choose an audio file   (or just play the demo)", cx, sh * 0.41 + 26.0, 18, INK_DIM);

    text_center("MODE", cx, sh * 0.56, 18, INK_DIM);
    for (i, n) in mode_names.iter().enumerate() {
        let y = sh * 0.56 + 34.0 + i as f32 * 38.0;
        let selected = i == sel;
        let label = if selected { format!(">  {n}") } else { (*n).to_string() };
        let color = if selected { HILITE } else { INK_DIM };
        text_center(&label, cx, y, 30, color);
    }

    text_center("Up / Down  select        Enter  start        O  open file", cx, sh * 0.90, 18, INK_DIM);
    if let Some(e) = error {
        text_center(&format!("could not load: {e}"), cx, sh * 0.95, 16, Color::new(0.9, 0.5, 0.5, 0.9));
    }
}

fn draw_loading(name: &str, elapsed: f32) {
    clear_background(BG);
    let (sw, sh) = (screen_width(), screen_height());
    let cx = sw * 0.5;

    text_center(&format!("Loading   {name}"), cx, sh * 0.46, 28, INK);

    // Indeterminate bar: a bright segment sweeps back and forth.
    let bw = sw * 0.42;
    let bh = 12.0;
    let bx = cx - bw / 2.0;
    let by = sh * 0.54;
    draw_rectangle(bx, by, bw, bh, Color::new(1.0, 1.0, 1.0, 0.10));
    let seg_w = bw * 0.28;
    let p = (elapsed * 0.8).fract();
    let tri = if p < 0.5 { p * 2.0 } else { 2.0 - p * 2.0 };
    draw_rectangle(bx + (bw - seg_w) * tri, by, seg_w, bh, HILITE);

    text_center("decoding & analyzing...", cx, sh * 0.62, 18, INK_DIM);
}
