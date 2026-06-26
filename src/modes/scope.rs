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
use crate::style::{amber, mix, smoothstep, teal, teal_deep, with_alpha};
use crate::track::Track;
use crate::view::{View, AH, AW};

const NPTS: usize = 300;

pub struct Scope {
    history: VecDeque<Vec<f32>>,
    rms_s: f32, // smoothed loudness driver (snappy but not jittery)
    amp: f32,
    persist: usize,
}

impl Scope {
    pub fn new() -> Self {
        Scope { history: VecDeque::new(), rms_s: 0.0, amp: 2.6, persist: 16 }
    }

    fn sample(wave: &[f32]) -> Vec<f32> {
        let n = wave.len().max(1);
        // RAW PCM, no smoothing — a crisp time-domain trace like a real scope and
        // the web visualizers. (The old [1,2,1] blur read as mush.) Loudness still
        // reaches the trace via the rms-scaled amplitude in draw(), so quiet stays
        // small. Fixed scale, not per-window peak (which cancelled loudness).
        let norm = 2.4;
        (0..NPTS)
            .map(|i| {
                let f = i as f32 / (NPTS - 1) as f32;
                let idx = ((f * (n - 1) as f32) as usize).min(n - 1);
                wave[idx] * norm
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

    fn trail(&self) -> f32 {
        0.12
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
        self.rms_s += (ctx.feat.rms - self.rms_s) * (1.0 - (-ctx.dt / 0.08).exp());
        self.history.push_back(Self::sample(ctx.wave));
        while self.history.len() > self.persist {
            self.history.pop_front();
        }
    }

    fn draw(&self, _ctx: &FrameCtx) {
        let v = View::fit_world(AW, AH);
        let cy = AH * 0.46; // off dead-center; dark space above and below
        let amp = self.amp * (0.28 + self.rms_s * 1.4);

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
                let y0 = (cy + row[i - 1] * amp).clamp(0.3, AH - 0.3);
                let y1 = (cy + row[i] * amp).clamp(0.3, AH - 0.3);
                // Only the very loudest crests tip amber (tighter amber discipline).
                let mag = row[i - 1].abs().max(row[i].abs());
                let hot = smoothstep(0.70, 0.98, mag * (0.6 + self.rms_s));
                let c = mix(teal(), amber(), hot);
                v.line(x0, y0, x1, y1, width, with_alpha(c, fade));
            }
        }
    }
}
