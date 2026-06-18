//! Starfield — warp-stars flown by the music, in the master palette.
//!
//! Stars stream toward the camera at a speed set by loudness; on a beat the
//! field punches into a brief warp that stretches each star into a streak.
//! Color encodes depth + speed WITHIN the family: far/slow stars are dim teal
//! that sinks into the background, near/fast stars warm toward amber, and only
//! the closest few tip to cream. Pure 2D projection, so it composes and exports
//! like the other flat modes. The RNG is a seeded LCG so an export is
//! reproducible.

use macroquad::prelude::*;

use crate::modes::{FrameCtx, Mode, Param};
use crate::style::{self, amber, mix, smoothstep, teal, teal_deep, with_alpha};
use crate::track::Track;
use crate::view::{View, AH, AW};

const FAR: f32 = 14.0;
const NEAR: f32 = 0.18;

struct Star {
    x: f32,
    y: f32,
    z: f32,
}

pub struct Starfield {
    stars: Vec<Star>,
    seed: u32,
    warp: f32,
    // live-tunable
    count: usize,
    speed: f32,
    spread: f32,
}

impl Starfield {
    pub fn new() -> Self {
        let mut s = Starfield {
            stars: Vec::new(),
            seed: 0x9e3779b9,
            warp: 0.0,
            count: 440,
            speed: 6.0,
            spread: 7.0,
        };
        s.respawn_all();
        s
    }

    fn rand(&mut self) -> f32 {
        self.seed = self.seed.wrapping_mul(1664525).wrapping_add(1013904223);
        (self.seed >> 8) as f32 / 16_777_216.0
    }

    fn new_star(&mut self, fresh: bool) -> Star {
        // Bias density OUTWARD so there's no bright pile-up at the vanishing
        // point (a radial-symmetry tell); the field fills, the center breathes.
        let r = 0.2 + 0.8 * self.rand();
        let ang = self.rand() * std::f32::consts::TAU;
        let x = ang.cos() * r * self.spread;
        let y = ang.sin() * r * self.spread;
        let z = if fresh { NEAR + self.rand() * (FAR - NEAR) } else { FAR };
        Star { x, y, z }
    }

    fn respawn_all(&mut self) {
        self.stars.clear();
        for _ in 0..self.count {
            let s = self.new_star(true);
            self.stars.push(s);
        }
    }
}

/// Project a star to world space (off-center vanishing point) with a focal
/// scale; returns the point and its on-screen radius, or None if behind.
fn project(x: f32, y: f32, z: f32) -> Option<(f32, f32, f32)> {
    if z <= NEAR * 0.5 {
        return None;
    }
    let f = AW * 0.9 / z;
    let px = AW * 0.41 + x * f; // well off dead-center -> not a bullseye
    let py = AH * 0.43 + y * f;
    let r = (0.15 * (1.0 - z / FAR)).max(0.014);
    Some((px, py, r))
}

impl Mode for Starfield {
    fn name(&self) -> &'static str {
        "Starfield"
    }

    fn about(&self) -> &'static str {
        "Warp through stars; loudness sets the speed and beats punch into hyperspace."
    }

    fn trail(&self) -> f32 {
        0.18
    }

    fn params(&self) -> Vec<Param> {
        vec![
            Param::int("Stars", self.count as i32, 60, 700),
            Param::float("Speed", self.speed, 1.0, 16.0),
            Param::float("Spread", self.spread, 3.0, 16.0),
        ]
    }

    fn set_param(&mut self, name: &str, v: f32) {
        match name {
            "Stars" => {
                let n = (v.round() as usize).max(1);
                if n != self.count {
                    self.count = n;
                    self.respawn_all();
                }
            }
            "Speed" => self.speed = v,
            "Spread" => self.spread = v,
            _ => {}
        }
    }

    fn reset(&mut self, _track: &Track) {
        self.seed = 0x9e3779b9;
        self.warp = 0.0;
        self.respawn_all();
    }

    fn update(&mut self, ctx: &FrameCtx) {
        let dt = ctx.dt;
        self.warp = (self.warp - dt * 2.2).max(0.0);
        if let Some(s) = ctx.feat.beat {
            self.warp = self.warp.max((s * 0.5).min(1.6));
        }
        let v = self.speed * (0.5 + ctx.feat.rms * 1.6) * (1.0 + self.warp);
        let n = self.stars.len();
        for i in 0..n {
            self.stars[i].z -= v * dt;
            if self.stars[i].z <= NEAR {
                self.stars[i] = self.new_star(false);
            }
        }
    }

    fn draw(&self, ctx: &FrameCtx) {
        let v = View::fit_world(AW, AH);
        // The streak length grows with warp so beats feel like a punch; a small
        // floor keeps a hint of motion even in a still frame.
        let streak = (0.05 + self.warp * 0.9) * self.speed * ctx.dt;

        // The single nearest star is the hero (one glow-cored anchor).
        let hero = self
            .stars
            .iter()
            .enumerate()
            .filter(|(_, s)| s.z > NEAR * 0.5)
            .min_by(|(_, a), (_, b)| a.z.total_cmp(&b.z))
            .map(|(i, _)| i);

        // Beats let a few more stars warm; otherwise the field stays cool and
        // only the nearest stars carry any amber (warm coverage stays small).
        let warm_gate = 0.80 - self.warp * 0.22;

        for (i, s) in self.stars.iter().enumerate() {
            let Some((px, py, r)) = project(s.x, s.y, s.z) else { continue };
            let depth = 1.0 - s.z / FAR; // 0 far .. 1 near
            // Cool field; only near stars warm toward amber.
            let base = mix(teal_deep(), teal(), smoothstep(0.0, 1.0, depth));
            let c = mix(base, amber(), smoothstep(warm_gate, 1.0, depth));
            // Steep falloff so far stars sink into ink instead of all glowing.
            let bright = (0.12 + depth * depth * 1.1).min(1.0);

            if streak > 0.01 {
                if let Some((tx, ty, _)) = project(s.x, s.y, s.z + streak) {
                    v.line(tx, ty, px, py, (r * 90.0).max(1.0), with_alpha(c, 0.4 * bright));
                }
            }
            if Some(i) == hero {
                style::glow_core(&v, px, py, r.max(0.05), amber());
            } else {
                v.circle(px, py, r, with_alpha(c, bright));
            }
        }
    }
}
