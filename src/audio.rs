//! Playback and the master clock.
//!
//! The clock is a plain dt accumulator (`pos`), reset to 0 on load/restart and
//! frozen while paused. rodio is used only to make sound — a FRESH player is
//! created per track so playback always starts clean (rodio's `get_pos` is
//! per-player state and does not reset across `stop`/`append`, which is exactly
//! the trap that froze track-switching before). The visual clock being our own
//! monotonic value means load, restart, pause and loop are all exact.

use crate::track::Track;
use std::num::NonZero;
use std::path::Path;

struct Sound {
    /// The device sink; must stay alive for the duration of playback.
    out: rodio::MixerDeviceSink,
    /// Current player. Replaced (and the old one dropped/stopped) on each start.
    player: rodio::Player,
}

pub struct AudioEngine {
    track: Track,
    sound: Option<Sound>,
    /// Visual playhead in seconds — the single source of truth for all modes.
    pos: f32,
    paused: bool,
    volume: f32,
}

impl AudioEngine {
    /// `audible = false` skips the audio device (headless `--shot` captures).
    pub fn new(audible: bool) -> Self {
        let sound = if audible { open_sound() } else { None };
        let mut engine =
            AudioEngine { track: Track::synth_demo(), sound, pos: 0.0, paused: false, volume: 0.85 };
        engine.start_playback();
        engine
    }

    pub fn volume(&self) -> f32 {
        self.volume
    }

    pub fn set_volume(&mut self, v: f32) {
        self.volume = v.clamp(0.0, 1.0);
        if let Some(s) = &self.sound {
            s.player.set_volume(self.volume);
        }
    }

    /// Jump the playhead (and audio) to `t` seconds.
    pub fn seek(&mut self, t: f32) {
        let t = t.clamp(0.0, self.duration());
        self.pos = t;
        if let Some(s) = &self.sound {
            let _ = s.player.try_seek(std::time::Duration::from_secs_f32(t));
        }
    }

    pub fn track(&self) -> &Track {
        &self.track
    }

    pub fn duration(&self) -> f32 {
        self.track.duration()
    }

    pub fn is_paused(&self) -> bool {
        self.paused
    }

    pub fn position(&self) -> f32 {
        self.pos
    }

    /// Advance the clock and loop at end of track. No-op while paused.
    pub fn tick(&mut self, dt: f32) {
        if self.paused {
            return;
        }
        self.pos += dt;
        if self.pos >= self.track.duration() {
            self.pos = 0.0;
            self.start_playback();
        }
    }

    pub fn toggle_pause(&mut self) {
        self.set_paused(!self.paused);
    }

    pub fn set_paused(&mut self, paused: bool) {
        self.paused = paused;
        if let Some(s) = &self.sound {
            if paused {
                s.player.pause();
            } else {
                s.player.play();
            }
        }
    }

    /// Install an already-decoded track (e.g. from a background loader thread)
    /// and cue it from the top, honoring the current pause state.
    pub fn set_track(&mut self, track: Track) {
        self.track = track;
        self.pos = 0.0;
        self.start_playback();
    }

    /// Restart the current track from the top.
    pub fn restart(&mut self) {
        self.pos = 0.0;
        self.paused = false;
        self.start_playback();
    }

    /// Decode `path`, swap it in, and play it from the top.
    pub fn load_file(&mut self, path: &Path) -> Result<(), String> {
        self.track = Track::from_file(path)?;
        self.restart();
        Ok(())
    }

    /// (Re)start audio output for the current track with a fresh player.
    fn start_playback(&mut self) {
        let source = mono_source(&self.track);
        if let Some(s) = self.sound.as_mut() {
            // A brand-new player guarantees a clean, playing state; assigning it
            // drops the previous player, which stops its audio.
            let player = rodio::Player::connect_new(s.out.mixer());
            player.set_volume(self.volume);
            player.append(source);
            if self.paused {
                player.pause();
            }
            s.player = player;
        }
    }
}

fn open_sound() -> Option<Sound> {
    let out = rodio::DeviceSinkBuilder::open_default_sink().ok()?;
    let player = rodio::Player::connect_new(out.mixer());
    Some(Sound { out, player })
}

fn mono_source(track: &Track) -> rodio::buffer::SamplesBuffer {
    rodio::buffer::SamplesBuffer::new(
        NonZero::new(1u16).unwrap(),
        NonZero::new(track.sr).unwrap(),
        track.pcm.clone(),
    )
}
