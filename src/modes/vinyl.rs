//! Vinyl — a spinning record whose grooves trace the live waveform. Loudness
//! drags the platter faster, the outer grooves ride the treble and the inner the
//! bass, an amber label pulses on the beat, and a tonearm rests on the edge.

use macroquad::prelude::*;

use crate::modes::{FrameCtx, Mode, Param};
use crate::style::{self, amber, grade, ink, mix, slate, spec, teal, with_alpha};
use crate::track::Track;
use crate::view::{View, AH, AW};

pub struct Vinyl {
    angle: f32,
    label_pulse: f32,
    rpm: f32,
    groove: f32,
}

impl Vinyl {
    pub fn new() -> Self {
        Vinyl { angle: 0.0, label_pulse: 0.0, rpm: 33.0, groove: 0.7 }
    }
}

impl Mode for Vinyl {
    fn name(&self) -> &'static str {
        "Vinyl"
    }
    fn about(&self) -> &'static str {
        "A spinning record whose grooves trace the live waveform; loudness drags the platter."
    }
    fn trail(&self) -> f32 {
        0.06
    }

    fn params(&self) -> Vec<Param> {
        vec![Param::float("RPM", self.rpm, 12.0, 60.0), Param::float("Groove depth", self.groove, 0.1, 1.6)]
    }
    fn set_param(&mut self, name: &str, v: f32) {
        match name {
            "RPM" => self.rpm = v,
            "Groove depth" => self.groove = v,
            _ => {}
        }
    }

    fn reset(&mut self, _t: &Track) {
        self.angle = 0.0;
        self.label_pulse = 0.0;
    }

    fn update(&mut self, ctx: &FrameCtx) {
        self.angle += ctx.dt * std::f32::consts::TAU * (self.rpm / 60.0) * (0.6 + ctx.feat.rms * 0.8);
        self.label_pulse = (self.label_pulse - ctx.dt * 3.0).max(0.0);
        if let Some(s) = ctx.feat.beat {
            self.label_pulse = self.label_pulse.max((s * 0.3).min(0.6));
        }
    }

    fn draw(&self, ctx: &FrameCtx) {
        let v = View::fit_world(AW, AH);
        let (cx, cy) = (AW * 0.5, AH * 0.5);
        let outer = 3.6;
        let wave = ctx.wave;
        let norm = 1.0 / wave.iter().fold(0.0f32, |m, &x| m.max(x.abs())).max(0.05);

        // Disc body: a dark filled platter with a faint sheen ring.
        v.circle(cx, cy, outer, mix(ink(), slate(), 0.75));
        v.circle(cx, cy, outer * 0.66, mix(ink(), slate(), 0.45));

        // Grooves: concentric segmented circles, radius perturbed by the PCM, so
        // the record literally traces the waveform. Outer = treble, inner = bass.
        let rings = 44;
        let segs = 80;
        for r in 0..rings {
            let fr = r as f32 / (rings - 1) as f32;
            let base_r = 1.05 + fr * (outer - 1.05);
            let energy = 0.18 + if fr > 0.5 { ctx.feat.treble } else { ctx.feat.bass } * 0.65;
            let c = with_alpha(grade(energy), 0.5);
            let mut prev: Option<(f32, f32)> = None;
            for s in 0..=segs {
                let a = s as f32 / segs as f32 * std::f32::consts::TAU + self.angle;
                let wi = (s + r * 5) % wave.len();
                let rr = base_r + wave[wi] * norm * self.groove * 0.06;
                let p = (cx + a.cos() * rr, cy + a.sin() * rr);
                if let Some((px, py)) = prev {
                    v.line(px, py, p.0, p.1, 1.3, c);
                }
                prev = Some(p);
            }
        }

        // The label (amber) pulses on the beat; the spindle is the spec hero.
        let lr = 0.95 * (1.0 + self.label_pulse * 0.1);
        v.circle(cx, cy, lr, amber());
        v.circle(cx, cy, lr * 0.66, mix(amber(), spec(), 0.3));
        style::glow_core(&v, cx, cy, 0.1, spec());

        // Tonearm resting on the outer edge (upper-right).
        let base = (cx + outer * 1.05, cy + outer * 0.85);
        let tip = (cx + outer * 0.62, cy + outer * 0.62);
        v.line(base.0, base.1, tip.0, tip.1, 3.0, with_alpha(teal(), 0.8));
        v.circle(tip.0, tip.1, 0.08, spec());
    }
}
