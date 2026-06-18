//! Oscilloscope — the raw waveform as a phosphor scope trace.
//!
//! One crisp teal line, drawn once. Loud excursions tip warm toward amber so
//! the trace carries its own energy color instead of a flat hue. A short ring
//! of past sweeps, drawn fainter and graded (never gray), gives the CRT
//! persistence smear — no stacked-glow passes. The whole thing rides the shared
//! graded backdrop + filmic finish.

use std::collections::VecDeque;

use macroquad::prelude::*;

use crate::modes::{FrameCtx, Mode, Param};
use crate::style::{self, amber, mix, smoothstep, teal, teal_deep, with_alpha};
use crate::track::Track;
use crate::view::{View, AH, AW};

const NPTS: usize = 300;

pub struct Scope {
    history: VecDeque<Vec<f32>>,
    amp: f32,
    persist: usize,
}

impl Scope {
    pub fn new() -> Self {
        Scope { history: VecDeque::new(), amp: 2.6, persist: 16 }
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
        "A phosphor scope trace — one teal line whose loud crests glow amber."
    }

    fn params(&self) -> Vec<Param> {
        vec![
            Param::float("Amplitude", self.amp, 0.5, 5.0),
            Param::int("Persistence", self.persist as i32, 1, 36),
        ]
    }

    fn set_param(&mut self, name: &str, v: f32) {
        match name {
            "Amplitude" => self.amp = v,
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
        style::backdrop();
        let feat = ctx.feat;
        let cy = AH * 0.46; // off dead-center; dark space above and below
        let amp = self.amp * (0.7 + feat.rms * 0.9);

        // Dim reference line — kept faint so it doesn't pull the eye back to
        // center against the deliberately off-center band.
        v.line(0.0, cy, AW, cy, 1.0, with_alpha(teal_deep(), 0.25));

        let n = self.history.len().max(1);
        for (k, row) in self.history.iter().enumerate() {
            let newest = k + 1 == n;
            let age = (k + 1) as f32 / n as f32; // 0 oldest .. 1 newest
            // Sharpen the hero (live trace) vs. body (older sweeps) value split.
            let width = if newest { 2.5 } else { 1.2 };
            let fade = if newest { 1.0 } else { age * age * 0.22 };
            for i in 1..NPTS {
                let x0 = (i - 1) as f32 / (NPTS - 1) as f32 * AW;
                let x1 = i as f32 / (NPTS - 1) as f32 * AW;
                let y0 = cy + row[i - 1] * amp;
                let y1 = cy + row[i] * amp;
                // Only the very loudest crests tip amber (tighter amber discipline).
                let mag = row[i - 1].abs().max(row[i].abs());
                let hot = smoothstep(0.70, 0.98, mag * (0.6 + feat.rms));
                let c = mix(teal(), amber(), hot);
                v.line(x0, y0, x1, y1, width, with_alpha(c, fade));
            }
        }

        style::finish(ctx.time);
    }
}
