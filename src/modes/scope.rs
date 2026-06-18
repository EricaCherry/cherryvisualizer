//! Oscilloscope — the raw waveform, drawn as a glowing scope trace.
//!
//! Each frame the PCM window is resampled to a fixed set of points and pushed
//! onto a short history ring; older traces are drawn fainter so motion leaves a
//! persistence smear, exactly like phosphor on a CRT scope. The live trace gets
//! a layered glow (a wide faint pass under a thin bright one) and its color
//! drifts with the treble.

use std::collections::VecDeque;

use macroquad::prelude::*;

use crate::modes::{FrameCtx, Mode, Param};
use crate::track::Track;
use crate::view::{hsl, View, AH, AW, BG};

const NPTS: usize = 300;

pub struct Scope {
    history: VecDeque<Vec<f32>>,
    amp: f32,
    glow: f32,
    persist: usize,
}

impl Scope {
    pub fn new() -> Self {
        Scope { history: VecDeque::new(), amp: 2.6, glow: 1.0, persist: 14 }
    }

    fn sample(wave: &[f32]) -> Vec<f32> {
        let n = wave.len().max(1);
        (0..NPTS)
            .map(|i| {
                let f = i as f32 / (NPTS - 1) as f32;
                let idx = ((f * (n - 1) as f32) as usize).min(n - 1);
                wave[idx]
            })
            .collect()
    }
}

impl Mode for Scope {
    fn name(&self) -> &'static str {
        "Oscilloscope"
    }

    fn about(&self) -> &'static str {
        "A glowing waveform scope with phosphor-style persistence — the signal itself."
    }

    fn params(&self) -> Vec<Param> {
        vec![
            Param::float("Amplitude", self.amp, 0.5, 5.0),
            Param::float("Glow", self.glow, 0.0, 2.0),
            Param::int("Persistence", self.persist as i32, 1, 30),
        ]
    }

    fn set_param(&mut self, name: &str, v: f32) {
        match name {
            "Amplitude" => self.amp = v,
            "Glow" => self.glow = v,
            "Persistence" => self.persist = (v.round() as usize).max(1),
            _ => {}
        }
    }

    fn reset(&mut self, _track: &Track) {
        self.history.clear();
    }

    fn update(&mut self, ctx: &FrameCtx) {
        self.history.push_back(Self::sample(ctx.wave));
        while self.history.len() > self.persist {
            self.history.pop_front();
        }
    }

    fn draw(&self, ctx: &FrameCtx) {
        let v = View::fit_world(AW, AH);
        clear_background(BG);
        let feat = ctx.feat;
        let cy = AH * 0.5;
        let amp = self.amp * (0.7 + feat.rms * 0.9);
        let hue = 0.55 - feat.treble * 0.18;

        let base = hsl(hue, 0.55, 0.6);
        let n = self.history.len().max(1);
        for (k, row) in self.history.iter().enumerate() {
            let newest = k + 1 == n;
            let age = (k + 1) as f32 / n as f32; // 0 oldest .. 1 newest

            // The live trace gets layered glow; older traces are a single
            // faint line that fades with age (the persistence smear).
            let passes: &[(f32, f32)] = if newest {
                &[(9.0, 0.07 * self.glow), (4.5, 0.18 * self.glow), (2.2, 1.0)]
            } else {
                &[(1.6, age * age * 0.22)]
            };
            for &(width, alpha) in passes {
                let c = Color::new(base.r, base.g, base.b, alpha.clamp(0.0, 1.0));
                for i in 1..NPTS {
                    let x0 = (i - 1) as f32 / (NPTS - 1) as f32 * AW;
                    let x1 = i as f32 / (NPTS - 1) as f32 * AW;
                    let y0 = cy + row[i - 1] * amp;
                    let y1 = cy + row[i] * amp;
                    v.line(x0, y0, x1, y1, width, c);
                }
            }
        }

        // Center reference line.
        v.line(0.0, cy, AW, cy, 1.0, Color::new(0.4, 0.45, 0.55, 0.25));
    }
}
