//! The mode system: every visualizer is one `Mode` behind one tiny trait.
//!
//! A mode never touches the audio device, decoding, or the window — it gets a
//! `FrameCtx` (current waveform window, features, the track's offline profile)
//! and draws in world units via [`crate::view::View`]. Adding a mode = one file
//! here + one line in `main.rs`.

pub mod breakout;
pub mod runner;

use crate::analysis::Features;
use crate::track::Track;

/// Everything a mode sees each frame.
pub struct FrameCtx<'a> {
    /// The PCM window at the playhead (what the listener hears right now).
    pub wave: &'a [f32],
    /// Spectral features of that window (+ beat from the offline grid).
    pub feat: &'a Features,
    /// The whole track, including the offline profile (future beats!).
    /// Part of the mode contract; not every mode reads every field.
    #[allow(dead_code)]
    pub track: &'a Track,
    /// Playhead in seconds.
    pub time: f32,
    /// Frame delta in seconds.
    pub dt: f32,
}

pub trait Mode {
    fn name(&self) -> &'static str;
    /// Called when a (new) track starts: precompute anything track-dependent.
    fn reset(&mut self, track: &Track);
    fn update(&mut self, ctx: &FrameCtx);
    fn draw(&self, ctx: &FrameCtx);
}
