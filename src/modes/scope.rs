//! Oscilloscope — a TRIGGERED phosphor scope trace, ported from how real audio
//! oscilloscopes actually work (Signalizer, OSCOPE, willpatton/Motion_Oscilloscope,
//! and the canonical Web-Audio `getByteTimeDomainData` example) rather than tuned
//! by feel:
//!   - a ZERO-CROSSING TRIGGER phase-locks each sweep so the waveform HOLDS STILL
//!     instead of scrolling/jittering frame to frame (this was the missing piece);
//!   - TIMEBASE ("Time") sets the horizontal zoom (how much of the window shows);
//!   - AMPLITUDE is the vertical gain;
//!   - plus line width, a trigger on/off, and CRT persistence (past sweeps fade).
//!
//! The PCM amplitude IS the loudness (per-track calibrated — see
//! `Track::window_at`), so quiet stays small and loud fills the screen with no
//! coupling hacks.

use std::collections::VecDeque;

use macroquad::prelude::*;

use crate::modes::{FrameCtx, Mode, Param};
use crate::style::{amber, mix, smoothstep, teal, teal_deep, with_alpha};
use crate::track::Track;
use crate::view::{View, AH, AW};

const NPTS: usize = 512;

pub struct Scope {
    /// The current sweep, refreshed every render frame (stays smooth at any
    /// refresh rate).
    live: Vec<f32>,
    /// Past sweeps (the phosphor ghosts), committed on a fixed 60 Hz cadence
    /// so "Persistence" spans the same real time at 30 or 60 fps.
    history: VecDeque<Vec<f32>>,
    acc: f32,
    amp: f32,      // vertical gain
    time: f32,     // timebase: fraction of the window shown (horizontal zoom)
    thick: f32,    // line width
    persist: usize,
    trigger: bool, // zero-crossing sync
}

impl Scope {
    pub fn new() -> Self {
        Scope {
            live: Vec::new(),
            history: VecDeque::new(),
            acc: 0.0,
            amp: 4.0,
            time: 0.5,
            thick: 1.0,
            persist: 14,
            trigger: true,
        }
    }

    /// One triggered sweep: find the first RISING zero-crossing (so the trace
    /// phase-locks and holds still), then read `span` samples from it across the
    /// display. Returns raw PCM (amplitude is applied in draw).
    fn sample(&self, wave: &[f32]) -> Vec<f32> {
        let n = wave.len();
        if n < 8 {
            return vec![0.0; NPTS];
        }
        let span = (n as f32 * self.time).clamp(64.0, n as f32 * 0.85) as usize;
        let mut start = 0usize;
        if self.trigger {
            // Scan the head of the window for a rising edge through zero.
            for i in 1..(n - span).max(1) {
                if wave[i - 1] < 0.0 && wave[i] >= 0.0 {
                    start = i;
                    break;
                }
            }
        }
        let span = span.min(n - start).max(1);
        (0..NPTS)
            .map(|j| {
                let f = j as f32 / (NPTS - 1) as f32;
                wave[(start + (f * (span - 1) as f32) as usize).min(n - 1)]
            })
            .collect()
    }
}

impl Mode for Scope {
    fn name(&self) -> &'static str {
        "Oscilloscope"
    }
    fn about(&self) -> &'static str {
        "A triggered scope trace — zero-cross synced so the waveform holds still."
    }
    fn trail(&self) -> f32 {
        0.10
    }

    fn params(&self) -> Vec<Param> {
        vec![
            Param::float("Amplitude", self.amp, 0.5, 8.0),
            Param::float("Time", self.time, 0.1, 1.0),
            Param::float("Line width", self.thick, 0.4, 3.0),
            Param::int("Trigger", self.trigger as i32, 0, 1),
            Param::int("Persistence", self.persist as i32, 1, 36),
        ]
    }
    fn set_param(&mut self, name: &str, v: f32) {
        match name {
            "Amplitude" => self.amp = v,
            "Time" => self.time = v,
            "Line width" => self.thick = v,
            "Trigger" => self.trigger = v >= 0.5,
            "Persistence" => self.persist = (v.round() as usize).max(1),
            _ => {}
        }
    }

    fn reset(&mut self, _track: &Track) {
        self.live.clear();
        self.history.clear();
        self.acc = 0.0;
    }

    fn update(&mut self, ctx: &FrameCtx) {
        self.live = self.sample(ctx.wave);
        self.acc += ctx.dt;
        while self.acc >= 1.0 / 60.0 {
            self.acc -= 1.0 / 60.0;
            self.history.push_back(self.live.clone());
        }
        // Persistence counts TOTAL traces (live + ghosts), as it always did —
        // so the ghost buffer holds one less than the slider value.
        while self.history.len() > self.persist.saturating_sub(1) {
            self.history.pop_front();
        }
    }

    fn draw(&self, _ctx: &FrameCtx) {
        let v = View::fit_world(AW, AH);
        let cy = AH * 0.5;
        let amp = self.amp;

        // Faint baseline (zero line).
        v.line(0.0, cy, AW, cy, 1.0, with_alpha(teal_deep(), 0.22));

        let trace = |row: &[f32], width: f32, fade: f32| {
            for i in 1..NPTS {
                let x0 = (i - 1) as f32 / (NPTS - 1) as f32 * AW;
                let x1 = i as f32 / (NPTS - 1) as f32 * AW;
                let y0 = (cy + row[i - 1] * amp).clamp(0.2, AH - 0.2);
                let y1 = (cy + row[i] * amp).clamp(0.2, AH - 0.2);
                // The raw excursion is the loudness; loud crests tip warm.
                let mag = row[i - 1].abs().max(row[i].abs());
                let hot = smoothstep(0.45, 0.92, mag);
                let c = mix(teal(), amber(), hot);
                v.line(x0, y0, x1, y1, width, with_alpha(c, fade));
            }
        };
        let n = self.history.len().max(1);
        for (k, row) in self.history.iter().enumerate() {
            let age = (k + 1) as f32 / n as f32;
            trace(row, 1.1 * self.thick, age * age * 0.22);
        }
        if self.live.len() == NPTS {
            trace(&self.live, 2.2 * self.thick, 1.0);
        }
    }
}
