//! Playback and the master clock.
//!
//! Two backends behind one interface so every mode syncs to the same clock:
//!   - `Rodio`: audible playback; the clock is the player's real position.
//!   - `Manual`: a silent fixed-step clock (used by `--shot` captures, or as a
//!     graceful fallback when no audio device exists).

use crate::track::Track;
use std::num::NonZero;
use std::path::Path;

enum Backend {
    Rodio {
        /// Must stay alive for the duration of playback.
        _out: rodio::MixerDeviceSink,
        player: rodio::Player,
    },
    Manual {
        pos: f32,
    },
}

pub struct AudioEngine {
    track: Track,
    backend: Backend,
    paused: bool,
}

impl AudioEngine {
    /// `audible = false` skips the audio device entirely (headless captures).
    pub fn new(audible: bool) -> Self {
        let track = Track::synth_demo();
        let backend = if audible {
            open_output().unwrap_or(Backend::Manual { pos: 0.0 })
        } else {
            Backend::Manual { pos: 0.0 }
        };
        let mut engine = AudioEngine { track, backend, paused: false };
        engine.restart();
        engine
    }

    pub fn track(&self) -> &Track {
        &self.track
    }

    pub fn duration(&self) -> f32 {
        self.track.duration()
    }

    pub fn is_audible(&self) -> bool {
        matches!(self.backend, Backend::Rodio { .. })
    }

    /// Current playhead in seconds.
    pub fn position(&self) -> f32 {
        match &self.backend {
            Backend::Rodio { player, .. } => {
                player.get_pos().as_secs_f32().min(self.duration())
            }
            Backend::Manual { pos } => *pos,
        }
    }

    /// Advance the manual clock and loop at end of track.
    pub fn tick(&mut self, dt: f32) {
        if let Backend::Manual { pos } = &mut self.backend {
            if !self.paused {
                *pos += dt;
            }
        }
        if self.position() >= self.duration() - 0.02 {
            self.restart();
        }
    }

    pub fn toggle_pause(&mut self) {
        self.paused = !self.paused;
        if let Backend::Rodio { player, .. } = &self.backend {
            if self.paused {
                player.pause();
            } else {
                player.play();
            }
        }
    }

    /// Restart the current track from the top.
    pub fn restart(&mut self) {
        match &mut self.backend {
            Backend::Manual { pos } => *pos = 0.0,
            Backend::Rodio { player, .. } => {
                player.stop();
                player.append(mono_source(&self.track));
                if self.paused {
                    player.pause();
                } else {
                    player.play();
                }
            }
        }
    }

    /// Decode `path`, swap it in, and start playing it.
    pub fn load_file(&mut self, path: &Path) -> Result<(), String> {
        self.track = Track::from_file(path)?;
        self.restart();
        Ok(())
    }

    pub fn status_line(&self) -> String {
        let t = self.position();
        let d = self.duration();
        let fmt = |s: f32| format!("{}:{:02}", (s / 60.0) as u32, (s % 60.0) as u32);
        let state = if self.paused {
            "paused"
        } else if self.is_audible() {
            "playing"
        } else {
            "silent clock"
        };
        format!("{}  ·  {} / {}  ·  {}", self.track.name, fmt(t), fmt(d), state)
    }
}

fn open_output() -> Option<Backend> {
    let out = rodio::DeviceSinkBuilder::open_default_sink().ok()?;
    let player = rodio::Player::connect_new(out.mixer());
    Some(Backend::Rodio { _out: out, player })
}

fn mono_source(track: &Track) -> rodio::buffer::SamplesBuffer {
    rodio::buffer::SamplesBuffer::new(
        NonZero::new(1u16).unwrap(),
        NonZero::new(track.sr).unwrap(),
        track.pcm.clone(),
    )
}
