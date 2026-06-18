//! Offline video export.
//!
//! The selected mode is re-rendered from the top on its own fresh instance (the
//! live view is never disturbed) into an offscreen render target at a fixed
//! resolution and frame rate. Each frame is read back and streamed as raw RGBA
//! to an `ffmpeg` child, which also reads the track's audio from a temp WAV and
//! muxes everything into an MP4.
//!
//! ffmpeg I/O runs on its own thread fed by a small bounded channel, so a slow
//! encode applies backpressure to rendering instead of blocking the UI thread.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{channel, sync_channel, Receiver, SyncSender, TryRecvError};
use std::time::Instant;

use macroquad::prelude::*;

use crate::analysis::Analyser;
use crate::modes::{FrameCtx, Mode};
use crate::track::Track;
use crate::view;

const FFT_LEN: usize = 2048;

#[derive(Clone, Copy)]
pub struct ExportSettings {
    pub width: u32,
    pub height: u32,
    pub fps: u32,
}

enum Msg {
    Frame(Vec<u8>),
    Finish,
}

pub struct Exporter {
    settings: ExportSettings,
    rt: RenderTarget,
    mode: Box<dyn Mode>,
    analyser: Analyser,
    window: Vec<f32>,
    total: u32,
    next: u32,
    tx: SyncSender<Msg>,
    result_rx: Receiver<Result<(), String>>,
    out_path: PathBuf,
    audio_path: PathBuf,
    finishing: bool,
}

impl Exporter {
    /// Begin an export. `mode` should be a fresh instance of the selected mode
    /// (with the user's live params already copied in). Returns an error if the
    /// temp audio cannot be written or ffmpeg cannot be launched.
    pub fn start(
        settings: ExportSettings,
        mut mode: Box<dyn Mode>,
        track: &Track,
        out_path: PathBuf,
    ) -> Result<Self, String> {
        let audio_path = std::env::temp_dir().join("cherry-export-audio.wav");
        write_wav(&audio_path, &track.pcm, track.sr).map_err(|e| format!("audio temp file: {e}"))?;

        let mut child = spawn_ffmpeg(&settings, &audio_path, &out_path)?;
        let mut stdin = child.stdin.take().ok_or("ffmpeg stdin unavailable")?;

        let (tx, rx) = sync_channel::<Msg>(3);
        let (result_tx, result_rx) = channel();
        std::thread::spawn(move || {
            let mut err = None;
            while let Ok(Msg::Frame(bytes)) = rx.recv() {
                if let Err(e) = stdin.write_all(&bytes) {
                    err = Some(format!("frame write: {e}"));
                    break;
                }
            }
            drop(stdin); // closing the pipe lets ffmpeg flush and exit
            let res = match (err, child.wait()) {
                (Some(e), _) => Err(e),
                (None, Ok(s)) if s.success() => Ok(()),
                (None, Ok(s)) => Err(format!("ffmpeg exited with {s}")),
                (None, Err(e)) => Err(format!("ffmpeg wait failed: {e}")),
            };
            let _ = result_tx.send(res);
        });

        let rt = render_target(settings.width, settings.height);
        rt.texture.set_filter(FilterMode::Linear);

        mode.reset(track);
        let total = ((track.duration() * settings.fps as f32).ceil() as u32).max(1);

        Ok(Exporter {
            settings,
            rt,
            mode,
            analyser: Analyser::new(FFT_LEN),
            window: vec![0.0; FFT_LEN],
            total,
            next: 0,
            tx,
            result_rx,
            out_path,
            audio_path,
            finishing: false,
        })
    }

    pub fn frames_done(&self) -> u32 {
        self.next
    }

    pub fn total(&self) -> u32 {
        self.total
    }

    pub fn progress(&self) -> f32 {
        self.next as f32 / self.total.max(1) as f32
    }

    /// Render frames into the export target for up to `budget_ms`, streaming
    /// each to the encoder. Returns `Some` once the encode has fully finished.
    pub fn step(&mut self, track: &Track, budget_ms: u64) -> Option<Result<PathBuf, String>> {
        if !self.finishing {
            let start = Instant::now();
            while self.next < self.total {
                let bytes = self.render_frame(track, self.next);
                if self.tx.send(Msg::Frame(bytes)).is_err() {
                    self.finishing = true; // encoder thread is gone
                    break;
                }
                self.next += 1;
                if start.elapsed().as_millis() as u64 >= budget_ms {
                    break;
                }
            }
            if self.next >= self.total {
                let _ = self.tx.send(Msg::Finish);
                self.finishing = true;
            }
        }

        if self.finishing {
            match self.result_rx.try_recv() {
                Ok(Ok(())) => return Some(Ok(self.out_path.clone())),
                Ok(Err(e)) => return Some(Err(e)),
                Err(TryRecvError::Disconnected) => return Some(Err("encoder stopped unexpectedly".into())),
                Err(TryRecvError::Empty) => {}
            }
        }
        None
    }

    fn render_frame(&mut self, track: &Track, i: u32) -> Vec<u8> {
        let fps = self.settings.fps as f32;
        let dt = 1.0 / fps;
        let t = i as f32 / fps;
        let prev_t = if i == 0 { 0.0 } else { (i - 1) as f32 / fps };

        track.window_at(t, &mut self.window);
        let mut feat = self.analyser.analyze(&self.window, track.sr, dt);
        feat.beat = track.profile.beat_in(prev_t, t);
        let ctx = FrameCtx { wave: &self.window, feat: &feat, track, time: t, dt };

        let (w, h) = (self.settings.width as f32, self.settings.height as f32);
        view::set_render_size(Some((w, h)));
        view::set_export_target(Some(self.rt.clone()));
        view::apply_screen_camera();
        self.mode.update(&ctx);
        self.mode.draw(&ctx);
        set_default_camera();
        view::set_export_target(None);
        view::set_render_size(None);

        // get_texture_data() returns rows bottom-up (GL order); ffmpeg's
        // rawvideo demuxer wants top-down, so flip. (Image::export_png flips
        // internally, which is why a saved PNG looks upright without this.)
        let img = self.rt.texture.get_texture_data();
        flip_vertical(img.bytes, img.width as usize, img.height as usize)
    }
}

fn flip_vertical(bytes: Vec<u8>, w: usize, h: usize) -> Vec<u8> {
    let stride = w * 4;
    if stride == 0 || bytes.len() < stride * h {
        return bytes;
    }
    let mut out = vec![0u8; stride * h];
    for y in 0..h {
        let src = (h - 1 - y) * stride;
        let dst = y * stride;
        out[dst..dst + stride].copy_from_slice(&bytes[src..src + stride]);
    }
    out
}

impl Drop for Exporter {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.audio_path);
    }
}

/// Render a single frame of `mode` at the given index into an offscreen target
/// and return the raw image (used by `--export-frame` for orientation checks).
/// Steps the mode from frame 0 so its state has evolved realistically.
pub fn render_preview(settings: ExportSettings, mut mode: Box<dyn Mode>, track: &Track, frame: u32) -> Image {
    let mut analyser = Analyser::new(FFT_LEN);
    let mut window = vec![0.0f32; FFT_LEN];
    let rt = render_target(settings.width, settings.height);
    rt.texture.set_filter(FilterMode::Linear);
    mode.reset(track);

    let fps = settings.fps as f32;
    let (w, h) = (settings.width as f32, settings.height as f32);
    for i in 0..=frame {
        let dt = 1.0 / fps;
        let t = i as f32 / fps;
        let prev_t = if i == 0 { 0.0 } else { (i - 1) as f32 / fps };
        track.window_at(t, &mut window);
        let mut feat = analyser.analyze(&window, track.sr, dt);
        feat.beat = track.profile.beat_in(prev_t, t);
        let ctx = FrameCtx { wave: &window, feat: &feat, track, time: t, dt };

        view::set_render_size(Some((w, h)));
        view::set_export_target(Some(rt.clone()));
        view::apply_screen_camera();
        mode.update(&ctx);
        mode.draw(&ctx);
        set_default_camera();
        view::set_export_target(None);
        view::set_render_size(None);
    }
    rt.texture.get_texture_data()
}

fn spawn_ffmpeg(s: &ExportSettings, audio: &Path, out: &Path) -> Result<Child, String> {
    let size = format!("{}x{}", s.width, s.height);
    let fps = s.fps.to_string();
    Command::new("ffmpeg")
        .args(["-y", "-f", "rawvideo", "-pix_fmt", "rgba", "-s", &size, "-r", &fps, "-i", "-"])
        .arg("-i")
        .arg(audio)
        .args([
            "-c:v", "libx264", "-pix_fmt", "yuv420p", "-preset", "medium", "-crf", "18",
            "-c:a", "aac", "-b:a", "192k", "-movflags", "+faststart", "-shortest",
        ])
        .arg(out)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                "ffmpeg not found — install it and put it on your PATH".to_string()
            } else {
                format!("could not start ffmpeg: {e}")
            }
        })
}

/// Write `pcm` (mono f32) as a 16-bit PCM WAV ffmpeg can read.
fn write_wav(path: &Path, pcm: &[f32], sr: u32) -> std::io::Result<()> {
    use std::fs::File;
    let mut data = Vec::with_capacity(pcm.len() * 2);
    for &s in pcm {
        let v = (s.clamp(-1.0, 1.0) * 32_767.0) as i16;
        data.extend_from_slice(&v.to_le_bytes());
    }
    let byte_len = data.len() as u32;
    let mut f = File::create(path)?;
    f.write_all(b"RIFF")?;
    f.write_all(&(36 + byte_len).to_le_bytes())?;
    f.write_all(b"WAVEfmt ")?;
    f.write_all(&16u32.to_le_bytes())?;
    f.write_all(&1u16.to_le_bytes())?; // PCM
    f.write_all(&1u16.to_le_bytes())?; // mono
    f.write_all(&sr.to_le_bytes())?;
    f.write_all(&(sr * 2).to_le_bytes())?; // byte rate
    f.write_all(&2u16.to_le_bytes())?; // block align
    f.write_all(&16u16.to_le_bytes())?; // bits
    f.write_all(b"data")?;
    f.write_all(&byte_len.to_le_bytes())?;
    f.write_all(&data)?;
    Ok(())
}
