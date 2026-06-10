//! Cherry — a native, modular music visualizer. The audio plays the game:
//! open a track, pick a mode, and watch the music play it.
//!
//! Controls:  O open a song · 1/2 or Tab switch mode · Space pause · R restart
//!
//! CLI: `cherry [--file <audio>]`
//!      `cherry --shot [breakout|surfer] [--file <audio>]`  -> renders 180
//!      frames headlessly (silent fixed clock), saves shot-<mode>.png, exits.
//!      `cherry --gen-wav <path>`  -> writes a small test WAV and exits.

mod analysis;
mod audio;
mod modes;
mod track;
mod view;

use macroquad::prelude::*;
use std::path::PathBuf;

use analysis::Analyser;
use audio::AudioEngine;
use modes::{breakout::Breakout, surfer::Surfer, FrameCtx, Mode};
use view::{INK, INK_DIM};

const FFT_LEN: usize = 2048;
const SHOT_FRAMES: u32 = 180;

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
                let mode = it
                    .peek()
                    .filter(|v| !v.starts_with("--"))
                    .cloned()
                    .map(|v| {
                        it.next();
                        v
                    })
                    .unwrap_or_else(|| "breakout".into());
                out.shot = Some(mode);
            }
            "--file" => out.file = it.next().map(PathBuf::from),
            "--gen-wav" => out.gen_wav = it.next().map(PathBuf::from),
            _ => {}
        }
    }
    out
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
    if let Some(path) = &args.file {
        if let Err(e) = audio.load_file(path) {
            eprintln!("could not load {}: {e}", path.display());
        }
    }

    let mut analyser = Analyser::new(FFT_LEN);
    let mut modes: Vec<Box<dyn Mode>> = vec![Box::new(Breakout::new()), Box::new(Surfer::new())];
    let mut cur = match args.shot.as_deref() {
        Some("surfer") => 1,
        _ => 0,
    };
    for m in modes.iter_mut() {
        m.reset(audio.track());
    }

    let mut window = vec![0.0f32; FFT_LEN];
    let mut last_t = 0.0f32;
    let mut frame = 0u32;

    loop {
        let dt = if headless { 1.0 / 60.0 } else { get_frame_time().min(0.05) };
        audio.tick(dt);

        if !headless {
            let switch_to = if is_key_pressed(KeyCode::Key1) {
                Some(0)
            } else if is_key_pressed(KeyCode::Key2) {
                Some(1)
            } else if is_key_pressed(KeyCode::Tab) {
                Some((cur + 1) % modes.len())
            } else {
                None
            };
            if let Some(i) = switch_to {
                cur = i;
                modes[cur].reset(audio.track());
            }
            if is_key_pressed(KeyCode::O) {
                let picked = rfd::FileDialog::new()
                    .add_filter("audio", &["mp3", "wav", "flac", "ogg", "m4a"])
                    .pick_file();
                if let Some(path) = picked {
                    match audio.load_file(&path) {
                        Ok(()) => {
                            for m in modes.iter_mut() {
                                m.reset(audio.track());
                            }
                            last_t = 0.0;
                        }
                        Err(e) => eprintln!("could not load {}: {e}", path.display()),
                    }
                }
            }
            if is_key_pressed(KeyCode::Space) {
                audio.toggle_pause();
            }
            if is_key_pressed(KeyCode::R) {
                audio.restart();
                modes[cur].reset(audio.track());
                last_t = 0.0;
            }
        }

        // One features snapshot per frame; the beat comes from the offline grid.
        let t = audio.position();
        audio.track().window_at(t, &mut window);
        let mut feat = analyser.analyze(&window, audio.track().sr, dt);
        if t < last_t {
            last_t = 0.0; // track looped or restarted
        }
        feat.beat = audio.track().profile.beat_in(last_t, t);
        last_t = t;

        let ctx = FrameCtx {
            wave: &window,
            feat: &feat,
            track: audio.track(),
            time: t,
            dt,
        };
        modes[cur].update(&ctx);
        modes[cur].draw(&ctx);

        // HUD.
        draw_text(&format!("Cherry · {}", modes[cur].name()), 14.0, 26.0, 24.0, INK);
        draw_text(&audio.status_line(), 14.0, screen_height() - 14.0, 18.0, INK);
        let help = "O open song   1/2/Tab mode   Space pause   R restart";
        let dim = measure_text(help, None, 18, 1.0);
        draw_text(
            help,
            screen_width() - dim.width - 14.0,
            screen_height() - 14.0,
            18.0,
            INK_DIM,
        );

        if headless {
            frame += 1;
            if frame >= SHOT_FRAMES {
                let name = format!(
                    "shot-{}.png",
                    modes[cur].name().to_lowercase().replace(' ', "-")
                );
                get_screen_data().export_png(&name);
                println!("wrote {name}");
                std::process::exit(0);
            }
        }

        next_frame().await
    }
}
