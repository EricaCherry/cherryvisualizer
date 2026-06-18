//! The mode system: every visualizer is one `Mode` behind one tiny trait.
//!
//! A mode never touches the audio device, decoding, or the window — it gets a
//! `FrameCtx` (current waveform window, features, the track's offline profile)
//! and draws in world units via [`crate::view::View`]. Adding a mode = one file
//! here + one line in `main.rs`.

pub mod breakout;
pub mod scope;
pub mod spectrogram;
pub mod spectrum;
pub mod starfield;
pub mod surfer;

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
    /// One-line description shown in the mode picker.
    fn about(&self) -> &'static str {
        ""
    }
    /// Called when a (new) track starts: precompute anything track-dependent.
    fn reset(&mut self, track: &Track);
    fn update(&mut self, ctx: &FrameCtx);
    fn draw(&self, ctx: &FrameCtx);

    /// Live-tunable parameters, rendered as sliders/checkboxes in the UI.
    fn params(&self) -> Vec<Param> {
        Vec::new()
    }
    /// Apply a changed parameter (value stored as f32; bools are 0/1).
    fn set_param(&mut self, _name: &str, _value: f32) {}

    /// Feedback-trail strength, 0..~0.3 (0 = none). When > 0 the mode is drawn
    /// through the feedback buffer: each frame the previous frame decays toward
    /// the backdrop by this fraction, so motion leaves echoes. Waterfall/3D
    /// modes return 0 (feedback would smear the time axis / depth).
    fn trail(&self) -> f32 {
        0.0
    }

    /// True if the mode paints its own opaque background (e.g. Surfer's sky), so
    /// the post pipeline draws it directly instead of wrapping it in the shared
    /// backdrop + feedback.
    fn own_background(&self) -> bool {
        false
    }
}

/// What kind of control a [`Param`] renders as.
#[derive(Clone, Copy, PartialEq)]
pub enum ParamKind {
    Float,
    Int,
}

/// One tunable knob a mode exposes to the settings UI.
#[derive(Clone)]
pub struct Param {
    pub name: &'static str,
    pub kind: ParamKind,
    pub value: f32,
    pub min: f32,
    pub max: f32,
}

impl Param {
    pub fn float(name: &'static str, value: f32, min: f32, max: f32) -> Self {
        Param { name, kind: ParamKind::Float, value, min, max }
    }
    pub fn int(name: &'static str, value: i32, min: i32, max: i32) -> Self {
        Param { name, kind: ParamKind::Int, value: value as f32, min: min as f32, max: max as f32 }
    }
}
