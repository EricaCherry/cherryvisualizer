//! Starfield — the demoscene/screensaver warp-stars, flown by the music.
//!
//! Stars stream toward the camera at a speed set by loudness; on a beat the
//! field punches into a brief warp, stretching each star into a streak. It's a
//! pure 2D projection (no 3D pass), so it composes and exports like the other
//! flat modes. The RNG is a seeded LCG so an export is reproducible.

use macroquad::prelude::*;

use crate::modes::{FrameCtx, Mode, Param};
use crate::track::Track;
use crate::view::{View, AH, AW, BG};

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
        let x = (self.rand() * 2.0 - 1.0) * self.spread;
        let y = (self.rand() * 2.0 - 1.0) * self.spread;
        // `fresh` spreads the initial field through depth; recycled stars start far.
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

/// Project a star to world space (centered) with a focal scale; returns the
/// point and its on-screen radius, or None if behind the camera.
fn project(x: f32, y: f32, z: f32) -> Option<(f32, f32, f32)> {
    if z <= NEAR * 0.5 {
        return None;
    }
    let f = AW * 0.9 / z;
    let px = AW * 0.5 + x * f;
    let py = AH * 0.5 + y * f;
    let r = (0.13 * (1.0 - z / FAR)).max(0.018);
    Some((px, py, r))
}

impl Mode for Starfield {
    fn name(&self) -> &'static str {
        "Starfield"
    }

    fn about(&self) -> &'static str {
        "Warp through a field of stars; loudness sets the speed and beats punch into hyperspace."
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
        clear_background(BG);
        // The streak length grows with warp so beats feel like a punch.
        let streak = (0.08 + self.warp * 0.9) * self.speed * ctx.dt;

        for s in &self.stars {
            let Some((px, py, r)) = project(s.x, s.y, s.z) else { continue };
            let depth = 1.0 - s.z / FAR; // 0 far .. 1 near
            let bright = (0.42 + depth * 0.78).min(1.0);
            // Stars cool from warm-white toward blue with distance.
            let c = Color::new(bright, bright * (0.92 + 0.08 * depth), (bright * 1.05).min(1.0), 1.0);

            if streak > 0.02 {
                // Tail: where the star was a moment ago (further away).
                if let Some((tx, ty, _)) = project(s.x, s.y, s.z + streak) {
                    v.line(tx, ty, px, py, (r * 90.0).max(1.0), Color::new(c.r, c.g, c.b, 0.5 * bright));
                }
            }
            v.circle(px, py, r, c);
        }
    }
}
