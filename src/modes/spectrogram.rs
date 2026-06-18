//! Spectrogram — a scrolling time/frequency waterfall in the master palette.
//!
//! Every frame the current 32-band spectrum becomes one column; columns scroll
//! left so the newest is at the right edge. Low frequencies sit at the bottom.
//! The heat ramp is the shared energy grade re-keyed so quiet bins recede into
//! the ink floor and only loud bins climb to amber and cream — no blue->red
//! rainbow. It opts out of the persistence trails (it IS the time axis) but
//! still takes the shared vignette + grain so it matches the family.

use macroquad::prelude::*;

use crate::analysis::N_BANDS;
use crate::modes::{FrameCtx, Mode, Param};
use crate::style::{self, hash01, mix, INK};
use crate::track::Track;
use crate::view::{View, AH, AW};

pub struct Spectrogram {
    cols: std::collections::VecDeque<[f32; N_BANDS]>,
    width: usize,
    gain: f32,
}

impl Spectrogram {
    pub fn new() -> Self {
        Spectrogram { cols: std::collections::VecDeque::new(), width: 260, gain: 1.2 }
    }
}

/// Intensity (0..1) -> heat, re-keyed to the palette: quiet recedes into ink,
/// then teal, then amber, then cream at the hottest.
fn heat(v: f32) -> Color {
    let v = v.clamp(0.0, 1.0);
    let floor = style::smoothstep(0.0, 0.12, v); // quiet bins fade to background
    mix(INK, style::grade(v), floor)
}

impl Mode for Spectrogram {
    fn name(&self) -> &'static str {
        "Spectrogram"
    }

    fn about(&self) -> &'static str {
        "A time/frequency waterfall graded like heat — quiet recedes, loud burns amber."
    }

    fn params(&self) -> Vec<Param> {
        vec![
            Param::float("Gain", self.gain, 0.4, 3.0),
            Param::int("History", self.width as i32, 80, 480),
        ]
    }

    fn set_param(&mut self, name: &str, v: f32) {
        match name {
            "Gain" => self.gain = v,
            "History" => self.width = (v.round() as usize).max(8),
            _ => {}
        }
    }

    fn reset(&mut self, _track: &Track) {
        self.cols.clear();
    }

    fn update(&mut self, ctx: &FrameCtx) {
        let mut col = [0.0f32; N_BANDS];
        for i in 0..N_BANDS {
            // sqrt keeps quiet detail visible against the ink floor.
            col[i] = (ctx.feat.bands[i] * self.gain).clamp(0.0, 1.0).sqrt();
        }
        self.cols.push_back(col);
        while self.cols.len() > self.width {
            self.cols.pop_front();
        }
    }

    fn draw(&self, ctx: &FrameCtx) {
        let v = View::fit_world(AW, AH);
        clear_background(INK);

        let cw = AW / self.width as f32;
        let rh = AH / N_BANDS as f32;
        let start_x = AW - self.cols.len() as f32 * cw; // right-aligned (newest at right)
        let n = self.cols.len();
        for (ci, col) in self.cols.iter().enumerate() {
            let x = start_x + ci as f32 * cw;
            // ±2% column jitter + a hotter live edge break the mechanical grid.
            let jit = 0.98 + hash01(ci as i32 * 5 + 1) * 0.04;
            let live = if ci + 1 == n { 1.18 } else { 1.0 };
            for b in 0..N_BANDS {
                let y_top = (b + 1) as f32 * rh;
                v.rect(x, y_top, cw + 0.01, rh + 0.01, heat(col[b] * jit * live));
            }
        }

        style::finish(ctx.time);
    }
}
