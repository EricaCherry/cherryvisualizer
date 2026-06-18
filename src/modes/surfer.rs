//! Beat Surfer — a 3D lane runner (Subway-Surfers style) played by the music.
//!
//! Nobody is holding the phone. At load, the whole track's beat grid is turned
//! into choreography:
//!   - strong, spaced beats become TRAINS in the player's lane, with a swerve
//!     scheduled half a second before they arrive;
//!   - every other beat becomes a BARRIER, and the jump is timed so its apex
//!     lands exactly on the beat (T-Rex timing: ~0.55 s of air);
//!   - offline treble runs become COIN TRAILS laid along the player's own
//!     future path — curving through swerves, arcing over jumps — so every
//!     coin is collected exactly on the music.
//!
//! Live layers animate different parts of the world each frame:
//!   bass    -> portal pylons pulse, the sun swells
//!   mids    -> building heights breathe, train windows glow
//!   treble  -> coins shimmer and spin, stars twinkle, rails brighten
//!   rms     -> world speed (offline curve) + camera FOV pulse
//!   bands   -> the skyline silhouettes on the horizon
//!   beats   -> a small camera kick
//!
//! Rendering is immediate-mode 3D with hand-rolled distance fog (every draw
//! color is lerped toward the horizon color) and wire outlines for a flat,
//! readable low-poly look. No textures, no shaders, no neon.

use macroquad::prelude::*;

use crate::modes::{FrameCtx, Mode};
use crate::track::Track;
use crate::view;

// ---- world tuning -----------------------------------------------------------
const LANE_W: f32 = 2.3;
const ROAD_HALF: f32 = 3.55;
const FAR: f32 = 80.0;
const BASE_SPEED: f32 = 14.0; // m/s at average loudness

// Jump kinematics: keep the Chromium T-Rex feel (~0.55 s airtime).
const AIR_T: f32 = 0.55;
const APEX: f32 = 1.15;
const JUMP_G: f32 = 8.0 * APEX / (AIR_T * AIR_T);
const JUMP_V0: f32 = JUMP_G * AIR_T / 2.0;

// Choreography.
const TRAIN_STRENGTH: f32 = 2.0;
const TRAIN_MIN_GAP: f32 = 2.5;
const SWITCH_LEAD: f32 = 0.5;
const SWITCH_DUR: f32 = 0.35;

// Palette (dusk, flat, no neon).
const HORIZON: Color = Color::new(0.16, 0.13, 0.18, 1.0);
const SKY_TOP: Color = Color::new(0.05, 0.06, 0.10, 1.0);
const ROAD: Color = Color::new(0.13, 0.14, 0.17, 1.0);
const GROUND: Color = Color::new(0.10, 0.10, 0.13, 1.0);
const COIN: Color = Color::new(0.92, 0.75, 0.30, 1.0);
const PLAYER_BODY: Color = Color::new(0.78, 0.27, 0.30, 1.0); // cherry red
const PLAYER_SKIN: Color = Color::new(0.88, 0.78, 0.66, 1.0);

struct Switch {
    t: f32, // swerve starts here
    from_x: f32,
    to_x: f32,
}

struct Train {
    t: f32, // moment its front reaches the player
    d: f32,
    x: f32,
    len: f32,
    hue: usize,
}

struct Barrier {
    t: f32, // the beat it lands on
    d: f32,
    x: f32,
}

struct CoinSpot {
    t: f32,
    d: f32,
    x: f32,
    y: f32,
}

struct Sparkle {
    d: f32,
    x: f32,
    y: f32,
    vx: f32,
    vy: f32,
    vd: f32,
    life: f32,
}

pub struct Surfer {
    hop_dt: f32,
    dist: Vec<f32>,
    switches: Vec<Switch>,
    trains: Vec<Train>,
    barriers: Vec<Barrier>,
    coins: Vec<CoinSpot>,
    sparkles: Vec<Sparkle>,
    prev_collected: usize,
    cam_kick: f32,
}

impl Surfer {
    pub fn new() -> Self {
        Surfer {
            hop_dt: 1.0 / 60.0,
            dist: vec![0.0],
            switches: Vec::new(),
            trains: Vec::new(),
            barriers: Vec::new(),
            coins: Vec::new(),
            sparkles: Vec::new(),
            prev_collected: 0,
            cam_kick: 0.0,
        }
    }

    fn dist_at(&self, t: f32) -> f32 {
        if self.dist.len() < 2 {
            return BASE_SPEED * t;
        }
        let f = (t / self.hop_dt).max(0.0);
        let i = (f as usize).min(self.dist.len() - 2);
        let frac = (f - i as f32).min(1.0);
        self.dist[i] * (1.0 - frac) + self.dist[i + 1] * frac
    }

    /// Player x at time `t`, eased through the scheduled swerves.
    fn lane_x_at(&self, t: f32) -> f32 {
        let i = self.switches.partition_point(|s| s.t <= t);
        if i == 0 {
            return self.switches.first().map_or(0.0, |s| s.from_x);
        }
        let s = &self.switches[i - 1];
        let k = ((t - s.t) / SWITCH_DUR).clamp(0.0, 1.0);
        let k = k * k * (3.0 - 2.0 * k); // smoothstep
        s.from_x + (s.to_x - s.from_x) * k
    }

    /// Player height at time `t`: a jump parabola whose apex is on the beat.
    fn jump_y_at(&self, t: f32) -> f32 {
        let i = self.barriers.partition_point(|b| b.t - AIR_T / 2.0 <= t);
        if i == 0 {
            return 0.0;
        }
        let ts = self.barriers[i - 1].t - AIR_T / 2.0;
        let a = t - ts;
        if a >= AIR_T {
            return 0.0;
        }
        (JUMP_V0 * a - 0.5 * JUMP_G * a * a).max(0.0)
    }
}

// Tiny deterministic hash -> 0..1, for seeded scenery.
fn hash01(n: i32) -> f32 {
    let mut x = n.wrapping_mul(374761393).wrapping_add(668265263) as u32;
    x = (x ^ (x >> 13)).wrapping_mul(1274126177);
    ((x ^ (x >> 16)) & 0xffff) as f32 / 65535.0
}

/// Distance fog: exponential-squared lerp toward the horizon color (softer
/// than linear — objects fade in instead of popping at the fog boundary).
fn fog(c: Color, dist: f32) -> Color {
    let x = dist.max(0.0) * 0.029;
    let f = (1.0 - (-x * x).exp()).clamp(0.0, 1.0);
    Color::new(
        c.r + (HORIZON.r - c.r) * f,
        c.g + (HORIZON.g - c.g) * f,
        c.b + (HORIZON.b - c.b) * f,
        c.a,
    )
}

/// Flat-shaded box with a darker wire outline (readable low-poly look).
/// The wires are drawn 0.8% larger than the fill to avoid z-fighting flicker.
fn box_outlined(center: Vec3, size: Vec3, c: Color) {
    let dist = -center.z;
    let fill = fog(c, dist);
    draw_cube(center, size, None, fill);
    if dist < FAR * 0.55 {
        draw_cube_wires(
            center,
            size * 1.008,
            fog(Color::new(c.r * 0.45, c.g * 0.45, c.b * 0.45, 1.0), dist),
        );
    }
}

impl Mode for Surfer {
    fn name(&self) -> &'static str {
        "Beat Surfer"
    }

    fn about(&self) -> &'static str {
        "A 3D lane runner the music plays: beats become trains, jumps, and coin trails."
    }

    fn reset(&mut self, track: &Track) {
        let p = &track.profile;
        self.hop_dt = p.hop_dt;

        // World speed follows the loudness curve; integrate once so events and
        // the player live in cumulative-distance space (exact beat arrivals).
        self.dist.clear();
        self.dist.push(0.0);
        let mut d = 0.0f32;
        for h in 0..p.rms.len() {
            let speed = BASE_SPEED * (0.6 + 0.9 * p.loudness_at(h as f32 * p.hop_dt));
            d += speed * p.hop_dt;
            self.dist.push(d);
        }

        // ---- choreography: beats -> trains (swerve) or barriers (jump) -----
        self.switches.clear();
        self.trains.clear();
        self.barriers.clear();
        let mut lane = 0i32;
        let mut last_train = -10.0f32;
        for (i, b) in p.beats.iter().enumerate() {
            if b.t < 1.2 || b.t > track.duration() - 1.0 {
                continue;
            }
            if b.strength >= TRAIN_STRENGTH && b.t - last_train > TRAIN_MIN_GAP {
                // A train in the player's lane forces a dodge.
                let dir = match lane {
                    1 => -1,
                    -1 => 1,
                    _ => if i % 2 == 0 { 1 } else { -1 },
                };
                self.trains.push(Train {
                    t: b.t,
                    d: self.dist_at(b.t),
                    x: lane as f32 * LANE_W,
                    len: 11.0 + (b.strength - TRAIN_STRENGTH) * 4.0,
                    hue: i % 3,
                });
                self.switches.push(Switch {
                    t: b.t - SWITCH_LEAD,
                    from_x: lane as f32 * LANE_W,
                    to_x: (lane + dir) as f32 * LANE_W,
                });
                lane += dir;
                last_train = b.t;
            } else {
                // A barrier in the player's lane at this moment; jump it.
                self.barriers.push(Barrier {
                    t: b.t,
                    d: self.dist_at(b.t),
                    x: self.lane_x_at(b.t),
                });
            }
        }

        // ---- coins: offline treble runs, laid on the player's future path ---
        self.coins.clear();
        let mut last_coin = -1.0f32;
        let n_hops = p.rms.len();
        for h in 0..n_hops {
            let t = h as f32 * p.hop_dt;
            if t < 1.2 || p.treble_at(t) < 0.5 || t - last_coin < 0.13 {
                continue;
            }
            // Not inside a train.
            if self.trains.iter().any(|tr| t >= tr.t - 0.05 && t <= tr.t + tr.len / BASE_SPEED) {
                continue;
            }
            self.coins.push(CoinSpot {
                t,
                d: self.dist_at(t),
                x: self.lane_x_at(t),
                y: 0.7 + self.jump_y_at(t),
            });
            last_coin = t;
        }

        self.sparkles.clear();
        self.prev_collected = 0;
        self.cam_kick = 0.0;
    }

    fn update(&mut self, ctx: &FrameCtx) {
        let dt = ctx.dt;
        self.cam_kick = (self.cam_kick - dt * 3.0).max(0.0);
        if let Some(s) = ctx.feat.beat {
            self.cam_kick = (s * 0.12).min(0.4);
        }

        // Coins collected since last frame -> sparkles where they were.
        let collected = self.coins.partition_point(|c| c.t <= ctx.time);
        if collected > self.prev_collected {
            for c in &self.coins[self.prev_collected..collected] {
                for k in 0..6 {
                    let a = k as f32 / 6.0 * std::f32::consts::TAU;
                    self.sparkles.push(Sparkle {
                        d: c.d,
                        x: c.x + a.cos() * 0.1,
                        y: c.y + 0.1,
                        vx: a.cos() * 1.6,
                        vy: 2.0 + a.sin().abs() * 1.5,
                        vd: a.sin() * 1.2,
                        life: 0.5,
                    });
                }
            }
        }
        self.prev_collected = collected;

        for s in &mut self.sparkles {
            s.x += s.vx * dt;
            s.y += s.vy * dt;
            s.d += s.vd * dt;
            s.vy -= 9.0 * dt;
            s.life -= dt;
        }
        self.sparkles.retain(|s| s.life > 0.0);
    }

    fn draw(&self, ctx: &FrameCtx) {
        let t = ctx.time;
        let feat = ctx.feat;
        let d_now = self.dist_at(t);
        let px = self.lane_x_at(t);
        let py = self.jump_y_at(t);
        // Banking: roll into the swerve, capped at ~8 degrees.
        let lateral_v = (self.lane_x_at(t + 0.03) - self.lane_x_at(t - 0.03)) / 0.06;
        let bank = (lateral_v * 0.022).clamp(-0.14, 0.14);

        // ================= 2D backdrop (drawn before the 3D pass) ============
        // In normal play this is the default screen camera; during export it
        // points the 2D pass at the offscreen target.
        view::apply_screen_camera();
        clear_background(SKY_TOP);
        let (sw, sh) = (view::screen_w(), view::screen_h());
        let horizon_y = sh * 0.52;
        let strips = 14;
        for i in 0..strips {
            let k = i as f32 / strips as f32;
            let c = Color::new(
                SKY_TOP.r + (HORIZON.r - SKY_TOP.r) * k,
                SKY_TOP.g + (HORIZON.g - SKY_TOP.g) * k,
                SKY_TOP.b + (HORIZON.b - SKY_TOP.b) * k,
                1.0,
            );
            draw_rectangle(0.0, horizon_y * k, sw, horizon_y / strips as f32 + 1.0, c);
        }
        // Stars twinkle with the treble layer.
        for i in 0..70 {
            let x = hash01(i * 3 + 1) * sw;
            let y = hash01(i * 3 + 2) * horizon_y * 0.8;
            let tw = 0.5 + 0.5 * ((t * (1.0 + hash01(i) * 3.0) + i as f32).sin());
            let a = (0.10 + 0.40 * feat.treble) * tw;
            draw_rectangle(x, y, 2.0, 2.0, Color::new(0.9, 0.9, 1.0, a));
        }
        // The sun swells with bass.
        let sun_r = sh * (0.085 + 0.025 * feat.bass);
        let sun_c = Color::new(0.95, 0.60, 0.42, 0.9);
        draw_circle(sw * 0.5, horizon_y * 0.86, sun_r, sun_c);
        draw_circle(sw * 0.5, horizon_y * 0.86, sun_r * 0.72, Color::new(0.99, 0.78, 0.55, 0.95));
        // Far skyline silhouettes = the spectrum.
        let n = feat.bands.len();
        let bw = sw / n as f32;
        for (i, &e) in feat.bands.iter().enumerate() {
            let h = sh * (0.012 + e * e * 0.075);
            draw_rectangle(i as f32 * bw, horizon_y - h, bw * 0.92, h, Color::new(0.11, 0.09, 0.14, 1.0));
        }
        // Below the horizon, fill with the fog color (the 3D world fades into it).
        draw_rectangle(0.0, horizon_y, sw, sh - horizon_y, HORIZON);

        // ================= 3D pass ===========================================
        let fov = (58.0 + feat.rms * 9.0 + self.cam_kick * 14.0).to_radians();
        set_camera(&Camera3D {
            position: vec3(px * 0.72, 2.7 + py * 0.25 + self.cam_kick * 0.12, 5.4),
            target: vec3(px * 0.86, 1.15 + py * 0.45, -8.0),
            up: vec3(bank, 1.0, 0.0).normalize(),
            fovy: fov,
            // Lock the aspect to the logical frame and route to the export
            // target when one is active, so the 3D pass exports correctly.
            aspect: Some(view::screen_w() / view::screen_h()),
            render_target: view::export_target(),
            ..Default::default()
        });

        // Road + shoulders, drawn as depth strips so the fog can take them —
        // a flat untextured ground is what makes immediate-mode 3D look fake.
        let strip = 7.0;
        let mut z0 = 6.0;
        while z0 > -FAR {
            let zc = z0 - strip / 2.0;
            draw_plane(vec3(0.0, 0.0, zc), vec2(ROAD_HALF, strip / 2.0), None, fog(ROAD, -zc));
            for side in [-1.0f32, 1.0] {
                draw_plane(
                    vec3(side * (ROAD_HALF + 14.0), -0.05, zc),
                    vec2(14.0, strip / 2.0),
                    None,
                    fog(GROUND, -zc),
                );
            }
            z0 -= strip;
        }

        // Lane dashes scroll with travel; edge lines run solid.
        let dash_phase = d_now % 3.0;
        for lane_edge in [-LANE_W / 2.0 - LANE_W / 2.0, LANE_W / 2.0 + LANE_W / 2.0] {
            // (edges between lanes sit at +-1.15 * 2 = lane half offsets)
            let x = lane_edge / 2.0;
            let mut z = 4.0 - dash_phase;
            while z > -FAR {
                box_outlined(vec3(x, 0.012, z - 0.45), vec3(0.09, 0.02, 0.9), Color::new(0.55, 0.57, 0.62, 1.0));
                z -= 3.0;
            }
        }
        for side in [-1.0f32, 1.0] {
            let mut z = 4.0;
            while z > -FAR {
                let seg = vec3(side * ROAD_HALF, 0.012, z - 2.0);
                draw_cube(seg, vec3(0.07, 0.02, 4.0), None, fog(Color::new(0.45, 0.47, 0.52, 1.0), -seg.z));
                z -= 4.0;
            }
        }

        // The waveform runs along both road edges (the Cherry identity),
        // brightened by treble.
        let wave = ctx.wave;
        let segs = 44;
        for side in [-1.0f32, 1.0] {
            let mut prev: Option<Vec3> = None;
            for i in 0..segs {
                let f = i as f32 / (segs - 1) as f32;
                let z = 2.0 - f * 58.0;
                let si = ((f * (wave.len() - 1) as f32) as usize).min(wave.len() - 1);
                let p = vec3(side * (ROAD_HALF + 1.1), 0.45 + wave[si] * 1.1, z);
                if let Some(q) = prev {
                    let c = fog(
                        Color::new(0.50 + 0.3 * feat.treble, 0.68, 0.82, 0.9),
                        -z,
                    );
                    draw_line_3d(q, p, c);
                }
                prev = Some(p);
            }
        }

        // Portal pylons every 18 m pulse with bass (demoscene tunnel rhythm).
        let portal_step = 18.0;
        let first = (d_now / portal_step).floor() as i32;
        for k in first..first + 6 {
            let z = -(k as f32 * portal_step - d_now);
            // Cull early on the near side: a frame half-past the camera fills
            // the screen with a floating beam.
            if z > -3.5 || z < -FAR {
                continue;
            }
            let pulse = 0.55 + 0.45 * feat.bass;
            let c = Color::new(0.24 + 0.10 * feat.bass, 0.23, 0.32, 1.0);
            for side in [-1.0f32, 1.0] {
                box_outlined(vec3(side * (ROAD_HALF + 0.9), 1.9, z), vec3(0.26, 3.8, 0.26), c);
            }
            box_outlined(
                vec3(0.0, 3.95 + 0.22 * pulse, z),
                vec3((ROAD_HALF + 1.0) * 2.0, 0.16 + 0.18 * pulse, 0.24),
                c,
            );
        }

        // City blocks breathe with the mids; their heights mix in the bands.
        let row_step = 6.0;
        let first_row = (d_now / row_step).floor() as i32;
        for k in first_row..first_row + 14 {
            let z = -(k as f32 * row_step - d_now);
            if z > 2.0 || z < -FAR {
                continue;
            }
            for side in [-1i32, 1] {
                let h0 = hash01(k * 2 + side);
                if h0 < 0.22 {
                    continue;
                }
                let band = feat.bands[(8 + (k * 7 + side * 3).rem_euclid(20)) as usize];
                let h = (1.6 + h0 * h0 * 7.0) * (0.7 + 0.45 * feat.mid + 0.25 * band);
                let x = side as f32 * (ROAD_HALF + 5.5 + hash01(k * 5 + side) * 7.0);
                let w = 2.1 + hash01(k * 9 + side) * 2.0;
                let shade = 0.16 + hash01(k * 11 + side) * 0.08;
                box_outlined(
                    vec3(x, h / 2.0, z),
                    vec3(w, h, 2.6),
                    Color::new(shade, shade * 1.05, shade * 1.35, 1.0),
                );
            }
        }

        // Trains (the swerve events). Windows glow with the mids.
        let train_colors = [
            Color::new(0.55, 0.22, 0.24, 1.0),
            Color::new(0.20, 0.38, 0.42, 1.0),
            Color::new(0.28, 0.27, 0.45, 1.0),
        ];
        for tr in &self.trains {
            let z_front = -(tr.d - d_now);
            if z_front - tr.len > 2.0 || z_front < -FAR {
                continue;
            }
            let zc = z_front - tr.len / 2.0;
            let body = train_colors[tr.hue];
            box_outlined(vec3(tr.x, 1.3, zc), vec3(2.0, 2.6, tr.len), body);
            box_outlined(
                vec3(tr.x, 2.75, zc),
                vec3(1.7, 0.3, tr.len * 0.92),
                Color::new(body.r * 1.3, body.g * 1.3, body.b * 1.3, 1.0),
            );
            // Windows: a strip of lit panes, brightness from the mids.
            let glow = 0.35 + 0.55 * feat.mid;
            let n_win = (tr.len / 1.5) as i32;
            for w in 0..n_win {
                let wz = z_front - 0.8 - w as f32 * 1.5;
                if wz < -FAR * 0.7 {
                    break;
                }
                for side in [-1.0f32, 1.0] {
                    draw_cube(
                        vec3(tr.x + side * 1.02, 1.9, wz),
                        vec3(0.04, 0.40, 0.65),
                        None,
                        fog(Color::new(0.95 * glow, 0.85 * glow, 0.55 * glow, 1.0), -wz),
                    );
                }
            }
        }

        // Barriers (the jump events); they flash as their beat arrives.
        for b in &self.barriers {
            let z = -(b.d - d_now);
            if z > 2.0 || z < -FAR {
                continue;
            }
            let near = (1.0 - ((b.t - t).abs() / 0.18).min(1.0)) * 0.6;
            let c = Color::new(0.72 + near * 0.28, 0.55 + near * 0.3, 0.30, 1.0);
            for side in [-1.0f32, 1.0] {
                box_outlined(vec3(b.x + side * 0.8, 0.42, z), vec3(0.13, 0.84, 0.13), c);
            }
            box_outlined(vec3(b.x, 0.72, z), vec3(1.75, 0.16, 0.12), c);
        }

        // Coins ahead, spinning faster as the treble sparkles.
        let spin = t * 3.0 + feat.treble * 5.0;
        for (i, c) in self.coins.iter().enumerate().skip(self.prev_collected) {
            let z = -(c.d - d_now);
            if z > 1.5 || z < -FAR * 0.8 {
                continue;
            }
            let a = spin + i as f32 * 0.7;
            let r = 0.20;
            let e1 = vec3(a.cos() * r * 2.0, 0.0, a.sin() * r * 2.0);
            let e2 = vec3(0.0, r * 2.0, 0.0);
            let e3 = vec3(-a.sin() * 0.05, 0.0, a.cos() * 0.05);
            let gold = fog(
                Color::new(
                    COIN.r + 0.08 * feat.treble,
                    COIN.g + 0.10 * feat.treble,
                    COIN.b,
                    1.0,
                ),
                -z,
            );
            let origin = vec3(c.x, c.y, z) - e1 / 2.0 - e2 / 2.0 - e3 / 2.0;
            draw_affine_parallelepiped(origin, e1, e2, e3, None, gold);
        }

        // Collected-coin sparkles.
        for s in &self.sparkles {
            let z = -(s.d - d_now);
            let k = (s.life / 0.5).clamp(0.0, 1.0);
            draw_cube(
                vec3(s.x, s.y, z),
                vec3(0.07, 0.07, 0.07) * k,
                None,
                fog(Color::new(0.98, 0.85, 0.45, 1.0), -z),
            );
        }

        // Player: a small cherry-red runner. Lean = banking, bob = run phase.
        let phase = d_now * 2.2;
        let bob = if py <= 0.0 { (phase * std::f32::consts::PI).sin().abs() * 0.06 } else { 0.0 };
        let base = py + bob;
        let lean = bank * 1.4;
        // Shadow grounds the player.
        draw_plane(
            vec3(px, 0.015, 0.0),
            vec2(0.42 * (1.0 - py / 4.0), 0.3),
            None,
            Color::new(0.0, 0.0, 0.0, 0.35),
        );
        // Legs (alternating while grounded, tucked in the air).
        let leg = Color::new(0.16, 0.17, 0.22, 1.0);
        if py <= 0.0 {
            let s0 = (phase * std::f32::consts::PI * 2.0).sin();
            box_outlined(vec3(px - 0.12, base + 0.22 + s0.max(0.0) * 0.08, 0.05 * s0), vec3(0.14, 0.44, 0.16), leg);
            box_outlined(vec3(px + 0.12, base + 0.22 + (-s0).max(0.0) * 0.08, -0.05 * s0), vec3(0.14, 0.44, 0.16), leg);
        } else {
            box_outlined(vec3(px - 0.12, base + 0.30, 0.06), vec3(0.14, 0.30, 0.16), leg);
            box_outlined(vec3(px + 0.12, base + 0.30, -0.02), vec3(0.14, 0.30, 0.16), leg);
        }
        // Torso + head, leaning into the swerve.
        box_outlined(vec3(px + lean * 0.10, base + 0.74, 0.0), vec3(0.46, 0.58, 0.30), PLAYER_BODY);
        box_outlined(vec3(px + lean * 0.22, base + 1.20, 0.0), vec3(0.27, 0.27, 0.27), PLAYER_SKIN);

        set_default_camera();
    }
}
