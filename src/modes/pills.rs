//! Glow Pills — a soft cloud of glowing orbs. A focus point orbits the centre
//! (its radius driven by the bass); orbs near it flare bright and warm, and each
//! orb also lights with its own frequency band, so the cloud shimmers to the
//! music. Beats bloom the whole field. Additive halos over the feedback trail.

use macroquad::prelude::*;

use crate::analysis::N_BANDS;
use crate::modes::{FrameCtx, Mode, Param};
use crate::style::{self, amber, grade, hash01, ink, mix, with_alpha};
use crate::track::Track;
use crate::view::{View, AH, AW};

struct Orb {
    x: f32,
    y: f32,
    depth: f32,
    band: usize,
}

pub struct Pills {
    orbs: Vec<Orb>,
    focus_ang: f32,
    bloom: f32,
    n: i32,
    orbit: f32,
    bloom_amt: f32,
}

impl Pills {
    pub fn new() -> Self {
        let mut p = Pills { orbs: Vec::new(), focus_ang: 0.0, bloom: 0.0, n: 600, orbit: 1.0, bloom_amt: 1.0 };
        p.build();
        p
    }
    fn build(&mut self) {
        self.orbs.clear();
        for i in 0..self.n {
            // Gaussian-ish cluster around the centre (sum of two uniforms).
            let x = AW * 0.5 + (hash01(i * 3) + hash01(i * 5) - 1.0) * 5.5;
            let y = AH * 0.5 + (hash01(i * 7) + hash01(i * 9) - 1.0) * 3.2;
            let depth = 0.4 + hash01(i * 11) * 0.6;
            self.orbs.push(Orb { x, y, depth, band: (i as usize) % N_BANDS });
        }
    }
}

impl Mode for Pills {
    fn name(&self) -> &'static str {
        "Glow Pills"
    }
    fn about(&self) -> &'static str {
        "A cloud of glowing orbs that flare as a bass-driven focus point sweeps through them."
    }
    fn trail(&self) -> f32 {
        0.18
    }

    fn params(&self) -> Vec<Param> {
        vec![
            Param::int("Orbs", self.n, 200, 900),
            Param::float("Orbit", self.orbit, 0.0, 2.0),
            Param::float("Bloom", self.bloom_amt, 0.0, 1.5),
        ]
    }
    fn set_param(&mut self, name: &str, v: f32) {
        match name {
            "Orbs" => {
                self.n = (v.round() as i32).clamp(200, 900);
                self.build();
            }
            "Orbit" => self.orbit = v,
            "Bloom" => self.bloom_amt = v,
            _ => {}
        }
    }

    fn reset(&mut self, _t: &Track) {
        self.focus_ang = 0.0;
        self.bloom = 0.0;
    }

    fn update(&mut self, ctx: &FrameCtx) {
        self.focus_ang += ctx.dt * 0.6;
        self.bloom = (self.bloom - ctx.dt * 3.0).max(0.0);
        if let Some(s) = ctx.feat.beat {
            if s > 1.8 {
                self.bloom = (s * 0.3).min(0.6) * self.bloom_amt;
            }
        }
    }

    fn draw(&self, ctx: &FrameCtx) {
        let v = View::fit_world(AW, AH);
        let feat = ctx.feat;
        let (cx, cy) = (AW * 0.5, AH * 0.5);
        let orbit_r = (2.0 + feat.bass * 2.5) * self.orbit;
        let fx = cx + self.focus_ang.cos() * orbit_r;
        let fy = cy + self.focus_ang.sin() * orbit_r * 0.6;
        let focus_r = 2.0;

        let mut hero = 0usize;
        let mut hb = 0.0f32;
        for (i, o) in self.orbs.iter().enumerate() {
            let e = feat.bands[o.band];
            let d = ((o.x - fx).hypot(o.y - fy) / focus_r).min(1.0);
            let near = 1.0 - d;
            let bright = ((0.22 + e * 0.95 + near * 0.6 + self.bloom) * o.depth).min(1.5);
            let c = grade((0.28 + e * 0.6 + near * 0.35).min(1.0));
            v.circle(o.x, o.y, 0.24 * o.depth, with_alpha(mix(ink(), c, 0.6), 0.28 * bright));
            v.circle(o.x, o.y, 0.12 * o.depth, with_alpha(c, (0.95 * bright).min(1.0)));
            if bright > hb {
                hb = bright;
                hero = i;
            }
        }
        let h = &self.orbs[hero];
        style::glow_core(&v, h.x, h.y, 0.12, amber());
    }
}
