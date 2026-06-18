//! Spectrogram — a scrolling waterfall of the spectrum over time.
//!
//! Every frame the current 32-band spectrum becomes one vertical column;
//! columns scroll left so the newest is at the right edge. Low frequencies sit
//! at the bottom. Intensity maps through a heat ramp (dark blue -> cyan ->
//! amber -> white), the classic spectrogram look.

use std::collections::VecDeque;

use macroquad::prelude::*;

use crate::analysis::N_BANDS;
use crate::modes::{FrameCtx, Mode, Param};
use crate::track::Track;
use crate::view::{hsl, View, AH, AW};

pub struct Spectrogram {
    cols: VecDeque<[f32; N_BANDS]>,
    width: usize,
    gain: f32,
}

impl Spectrogram {
    pub fn new() -> Self {
        Spectrogram { cols: VecDeque::new(), width: 260, gain: 1.2 }
    }
}

/// Intensity (0..1) -> heat color: dark blue, cyan, amber, near-white.
fn heat(v: f32) -> Color {
    let v = v.clamp(0.0, 1.0);
    // Hue sweeps blue(0.66) -> red(0.0); lightness rises so loud bins glow.
    let c = hsl(0.66 - 0.66 * v, 0.82, 0.06 + 0.55 * v);
    // Lift the very top toward white for the hottest bins.
    let w = (v - 0.65).max(0.0) / 0.35 * 0.7;
    Color::new((c.r + w).min(1.0), (c.g + w).min(1.0), (c.b + w).min(1.0), 1.0)
}

impl Mode for Spectrogram {
    fn name(&self) -> &'static str {
        "Spectrogram"
    }

    fn about(&self) -> &'static str {
        "A scrolling time-frequency waterfall — the whole spectrum, painted as heat."
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
            // sqrt compresses the range so quiet detail stays visible.
            col[i] = (ctx.feat.bands[i] * self.gain).clamp(0.0, 1.0).sqrt();
        }
        self.cols.push_back(col);
        while self.cols.len() > self.width {
            self.cols.pop_front();
        }
    }

    fn draw(&self, _ctx: &FrameCtx) {
        let v = View::fit_world(AW, AH);
        clear_background(Color::new(0.02, 0.02, 0.04, 1.0));

        let cw = AW / self.width as f32;
        let rh = AH / N_BANDS as f32;
        // Right-align so the newest column is at the right edge.
        let start_x = AW - self.cols.len() as f32 * cw;
        for (ci, col) in self.cols.iter().enumerate() {
            let x = start_x + ci as f32 * cw;
            for b in 0..N_BANDS {
                let y_top = (b + 1) as f32 * rh;
                // +overlap avoids hairline seams between cells.
                v.rect(x, y_top, cw + 0.01, rh + 0.01, heat(col[b]));
            }
        }
    }
}
