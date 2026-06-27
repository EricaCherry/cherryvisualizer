//! Beat Surfer — a Vib-Ribbon-style auto-runner the music plays.
//!
//! Nobody is holding the controller. At load the whole track is turned into a
//! single ribbon course by [`crate::modes::course`]: each beat becomes a TYPED,
//! well-spaced obstacle — an arch to jump (heavy/bass), a ring to spin through
//! (sustained mid), low teeth to roll over (treble), or a gap to stride (the
//! music dropping out). A min-gap sweep in distance guarantees the obstacles
//! never crowd each other, so the avatar's pose is a clean pure function of how
//! far it has travelled — no stacked jump parabolas, no mid-air snapping.
//!
//! The track itself is one undulating ribbon mesh whose centreline IS the
//! obstacle height field (so a jump arch lifts the road and a gap dips it),
//! rippled live by the waveform. Coins ride the player's own future path. The
//! sky, sun, stars and skyline are a 2D backdrop the 3D world fades into.

use macroquad::prelude::*;

use crate::material3d;
use crate::modes::course::{Ev, Kind, build_course};
use crate::modes::{Category, FrameCtx, Mode};
use crate::style::{self, amber, grade, hash01, mix, spec, teal};
use crate::track::Track;
use crate::view;

// ---- world tuning -----------------------------------------------------------
const ROAD_HALF: f32 = 3.4;
const FAR: f32 = 78.0;
const BASE_SPEED: f32 = 14.0; // m/s at average loudness
const APEX: f32 = 1.7; // jump apex height (clears the hurdle bar with margin)
const ARC_M: f32 = 4.0; // half-length (m) of one obstacle's clear motion
const MIN_GAP_M: f32 = 9.0; // > 2*ARC_M, so obstacle windows are disjoint

// Palette — keyed to the shared "Dusk Encom" master palette so Surfer reads as
// the same film as the 2D modes (dusk, flat, no neon).
const HORIZON: Color = Color::new(0.115, 0.10, 0.105, 1.0);

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
    course: Vec<Ev>,
    coins: Vec<CoinSpot>,
    sparkles: Vec<Sparkle>,
    prev_collected: usize,
    cam_kick: f32,
    // Smoothed followers — every per-frame camera/pose quantity is low-passed in
    // update() so the ride glides instead of popping on beats / snapping on
    // obstacle entry. draw() reads only these.
    cam_kick_s: f32,
    fov_s: f32,
    bank_s: f32,
    px_s: f32,
    py_s: f32,
    squash_s: f32,
    leg_s: f32,
    run_phase: f32,
}

impl Surfer {
    pub fn new() -> Self {
        Surfer {
            hop_dt: 1.0 / 60.0,
            dist: vec![0.0],
            course: Vec::new(),
            coins: Vec::new(),
            sparkles: Vec::new(),
            prev_collected: 0,
            cam_kick: 0.0,
            cam_kick_s: 0.0,
            fov_s: 58.0,
            bank_s: 0.0,
            px_s: 0.0,
            py_s: 0.0,
            squash_s: 1.0,
            leg_s: 1.0,
            run_phase: 0.0,
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

    /// Approximate time at a cumulative distance (the ribbon colours by loudness).
    fn t_at_dist(&self, d: f32) -> f32 {
        if self.dist.len() < 2 {
            return (d / BASE_SPEED).max(0.0);
        }
        let i = self.dist.partition_point(|&dd| dd < d).min(self.dist.len() - 1);
        i as f32 * self.hop_dt
    }

    /// A gentle, music-independent side-to-side weave so the runner feels alive
    /// without it being a dodge (Vib-Ribbon is a single track).
    fn weave(&self, d: f32) -> f32 {
        (d * 0.045).sin() * 0.7 + (d * 0.013).sin() * 0.35
    }

    /// The ribbon centreline height at distance `d`: jump arches lift it, gaps
    /// dip it. Because obstacles are >= MIN_GAP_M apart and each window is
    /// 2*ARC_M wide (< MIN_GAP_M), at most one obstacle ever contributes here —
    /// the curve is smooth and never restarts mid-motion (the old bug).
    fn surface_y(&self, d: f32) -> f32 {
        let mut y = 0.0f32;
        let lo = self.course.partition_point(|e| e.d < d - ARC_M);
        let mut i = lo;
        while i < self.course.len() && self.course[i].d <= d + ARC_M {
            let e = &self.course[i];
            let k = ((d - (e.d - ARC_M)) / (2.0 * ARC_M)).clamp(0.0, 1.0);
            let bump = (k * std::f32::consts::PI).sin();
            // A HOP that lifts only the avatar over a ground-level hurdle (the
            // ground/ribbon itself stays flat). A gap changes no height — the legs
            // stretch into a stride instead.
            if !matches!(e.kind, Kind::Pit) {
                y = y.max(bump * APEX * (0.85 + 0.07 * e.strength));
            }
            i += 1;
        }
        y
    }

    /// The obstacle the avatar is currently clearing, plus its progress 0..1.
    fn active_event(&self, d: f32) -> Option<(&Ev, f32)> {
        let lo = self.course.partition_point(|e| e.d < d - ARC_M);
        let mut i = lo;
        while i < self.course.len() && self.course[i].d <= d + ARC_M {
            let e = &self.course[i];
            if (d - e.d).abs() <= ARC_M {
                let k = ((d - (e.d - ARC_M)) / (2.0 * ARC_M)).clamp(0.0, 1.0);
                return Some((e, k));
            }
            i += 1;
        }
        None
    }
}

/// Distance fog: exponential-squared lerp toward the horizon color.
fn fog(c: Color, dist: f32) -> Color {
    let x = dist.max(0.0) * 0.030;
    let f = (1.0 - (-x * x).exp()).clamp(0.0, 1.0);
    Color::new(c.r + (HORIZON.r - c.r) * f, c.g + (HORIZON.g - c.g) * f, c.b + (HORIZON.b - c.b) * f, c.a)
}

/// Rotate a vector: roll about X (lateral), then spin about Z (forward).
fn rotv(v: Vec3, spin: f32, roll: f32) -> Vec3 {
    let (sr, cr) = roll.sin_cos();
    let p = vec3(v.x, v.y * cr - v.z * sr, v.y * sr + v.z * cr);
    let (ss, cs) = spin.sin_cos();
    vec3(p.x * cs - p.y * ss, p.x * ss + p.y * cs, p.z)
}

/// A bold 3D segment (a thin oriented box) — the building block for the glyphs.
fn seg3d(a: Vec3, b: Vec3, thick: f32, c: Color) {
    let ex = b - a;
    let len = ex.length();
    if len < 1e-4 {
        return;
    }
    let dir = ex / len;
    let up = if dir.y.abs() > 0.9 { vec3(1.0, 0.0, 0.0) } else { vec3(0.0, 1.0, 0.0) };
    let ey = dir.cross(up).normalize() * thick;
    let ez = dir.cross(ey).normalize() * thick;
    let origin = a - ey * 0.5 - ez * 0.5;
    draw_affine_parallelepiped(origin, ex, ey, ez, None, fog(c, -(a.z + b.z) * 0.5));
}

/// A minimal Vib-Ribbon-style vector creature (a line-art bunny) drawn from bold
/// 3D segments. The whole figure rotates as one (`spin` about forward, `roll`
/// about lateral) and squashes / stretches its legs, so it morphs through each
/// obstacle. Local coords: x right, y up, z toward the camera; origin at the
/// body centre, placed at `pivot`.
fn draw_creature(pivot: Vec3, spin: f32, roll: f32, squash: f32, leg: f32, run: f32, grounded: bool, c: Color) {
    let s = 1.5;
    let th = 0.085;
    let poly = |pts: &[Vec3]| {
        for w in pts.windows(2) {
            seg3d(pivot + rotv(w[0] * s, spin, roll), pivot + rotv(w[1] * s, spin, roll), th, c);
        }
    };
    // Body: an egg outline, wider at the bottom, squashed on a roll.
    let (bw, bh) = (0.28, 0.40 * squash);
    let mut body = Vec::with_capacity(17);
    for i in 0..=16 {
        let a = i as f32 / 16.0 * std::f32::consts::TAU;
        let r = 1.0 - 0.12 * a.cos();
        body.push(vec3(a.sin() * bw * r, -0.05 + a.cos() * bh, 0.0));
    }
    poly(&body);
    // Head circle on top.
    let hr = 0.16 * squash;
    let hy = bh * 0.92 + 0.12;
    let mut head = Vec::with_capacity(13);
    for i in 0..=12 {
        let a = i as f32 / 12.0 * std::f32::consts::TAU;
        head.push(vec3(a.sin() * hr, hy + a.cos() * hr, 0.0));
    }
    poly(&head);
    // Two long ears.
    for side in [-1.0f32, 1.0] {
        poly(&[
            vec3(side * hr * 0.5, hy + hr * 0.5, 0.0),
            vec3(side * hr * 0.95, hy + hr * 1.8, 0.0),
            vec3(side * hr * 0.7, hy + hr * 3.1, 0.0),
        ]);
    }
    // Two feet from the body bottom — they swing while running, elongate over a gap.
    let by = -0.05 - bh;
    for (i, side) in [-1.0f32, 1.0].into_iter().enumerate() {
        let sw = if grounded { run * 0.12 * if i == 0 { 1.0 } else { -1.0 } } else { 0.0 };
        poly(&[
            vec3(side * bw * 0.5, by, 0.0),
            vec3(side * bw * 0.55, by - 0.20 * leg, sw),
            vec3(side * bw * 0.95, by - 0.22 * leg, sw + 0.06),
        ]);
    }
}

// ---- obstacle glyphs (all built from bold segments, no plain cubes) ---------

/// A solid, LOW HURDLE to hop: two short posts and a bold top bar across the
/// track. Kept low (bar at base+0.5) so the hop clears it with clear margin.
fn draw_hurdle(x: f32, base: f32, z: f32, w: f32, c: Color) {
    let bar = base + 0.5;
    seg3d(vec3(x - w, base, z), vec3(x - w, bar, z), 0.12, c);
    seg3d(vec3(x + w, base, z), vec3(x + w, bar, z), 0.12, c);
    seg3d(vec3(x - w, bar, z), vec3(x + w, bar, z), 0.16, c); // bold cross-bar
    seg3d(vec3(x - w, base + 0.24, z), vec3(x + w, base + 0.24, z), 0.10, c); // mid rail
}

/// PIT → the ribbon already dips; edge the mouth with two lip lines.
fn draw_pit(x: f32, base: f32, z: f32, w: f32, c: Color) {
    let lip = mix(c, teal(), 0.4);
    seg3d(vec3(x - w, base, z + 0.7), vec3(x + w, base, z + 0.7), 0.09, lip);
    seg3d(vec3(x - w, base, z - 0.7), vec3(x + w, base, z - 0.7), 0.09, lip);
}

impl Mode for Surfer {
    fn name(&self) -> &'static str {
        "Beat Surfer"
    }

    fn about(&self) -> &'static str {
        "A Vib-Ribbon-style auto-runner the music plays: beats become a typed obstacle course."
    }

    fn category(&self) -> Category {
        Category::Game
    }

    // Surfer paints its own sky and finish (3D + fog carry the motion; feedback
    // would smear the depth), so it's drawn directly, not through the pipeline.
    fn own_background(&self) -> bool {
        true
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

        // The song designs the level: one typed, well-spaced obstacle course.
        self.course = build_course(p, &self.dist, track.duration(), MIN_GAP_M);

        // Coins: offline treble runs laid along the player's own future path, so
        // each coin is collected exactly on the music — arcing over jumps and
        // dipping through gaps via the same surface the avatar reads.
        self.coins.clear();
        let mut last_coin = -1.0f32;
        for h in 0..p.rms.len() {
            let t = h as f32 * p.hop_dt;
            if t < 1.2 || p.treble_at(t) < 0.5 || t - last_coin < 0.13 {
                continue;
            }
            let cd = self.dist_at(t);
            self.coins.push(CoinSpot { t, d: cd, x: self.weave(cd), y: 0.6 + self.surface_y(cd) });
            last_coin = t;
        }

        self.sparkles.clear();
        self.prev_collected = 0;
        self.cam_kick = 0.0;
        self.cam_kick_s = 0.0;
        self.fov_s = 58.0;
        self.bank_s = 0.0;
        self.px_s = 0.0;
        self.py_s = 0.0;
        self.squash_s = 1.0;
        self.leg_s = 1.0;
        self.run_phase = 0.0;
    }

    fn update(&mut self, ctx: &FrameCtx) {
        let dt = ctx.dt;
        // Beats add an impulse into a fast-decaying envelope; the camera reads a
        // low-passed FOLLOWER of it, so a beat nudges instead of popping.
        self.cam_kick = (self.cam_kick - dt * 4.0).max(0.0);
        if let Some(s) = ctx.feat.beat {
            self.cam_kick = (self.cam_kick + s * 0.10).min(0.35);
        }
        self.cam_kick_s += (self.cam_kick - self.cam_kick_s) * (1.0 - (-dt * 12.0).exp());

        // Smooth every per-frame camera/pose quantity here (draw() takes &self).
        let d_now = self.dist_at(ctx.time);
        let px = self.weave(d_now);
        let py = self.surface_y(d_now);
        // A wider finite-difference stencil + a hard low-pass kills bank jitter.
        let raw_bank = ((self.weave(d_now + 1.5) - self.weave(d_now - 1.5)) * 0.05).clamp(-0.10, 0.10);
        let raw_fov = 58.0 + ctx.feat.rms * 9.0 + self.cam_kick_s * 14.0;
        let ks = 1.0 - (-dt * 10.0).exp();
        self.px_s += (px - self.px_s) * ks;
        self.py_s += (py - self.py_s) * ks;
        self.bank_s += (raw_bank - self.bank_s) * (1.0 - (-dt * 6.0).exp());
        self.fov_s += (raw_fov - self.fov_s) * (1.0 - (-dt * 8.0).exp());
        // Subtle pose, eased: a small squash on the airborne hop, legs stretching
        // over a gap. NO spinning or rolling — the runner just hops the hurdle.
        let (mut tsq, mut tleg) = (1.0f32, 1.0f32);
        if let Some((e, k)) = self.active_event(d_now) {
            let bump = (k * std::f32::consts::PI).sin();
            match e.kind {
                Kind::Pit => tleg *= 1.0 + 0.6 * bump, // stride over the gap
                _ => tsq *= 1.0 - 0.12 * bump,          // light squash on the hop
            }
        }
        self.squash_s += (tsq - self.squash_s) * ks;
        self.leg_s += (tleg - self.leg_s) * ks;
        // Run cycle advances smoothly (decoupled from absolute distance).
        self.run_phase += dt * BASE_SPEED * 1.1;

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
        let prof = &ctx.track.profile;
        let sky_top = style::ink();
        let coin = style::amber();
        let d_now = self.dist_at(t);
        // Smoothed followers (updated in update()) — the ride glides.
        let px = self.px_s;
        let py = self.py_s;
        let bank = self.bank_s;

        // ================= 2D backdrop (drawn before the 3D pass) ============
        view::apply_screen_camera();
        clear_background(sky_top);
        let (sw, sh) = (view::screen_w(), view::screen_h());
        let horizon_y = sh * 0.52;
        let strips = 14;
        for i in 0..strips {
            let k = i as f32 / strips as f32;
            let c = Color::new(
                sky_top.r + (HORIZON.r - sky_top.r) * k,
                sky_top.g + (HORIZON.g - sky_top.g) * k,
                sky_top.b + (HORIZON.b - sky_top.b) * k,
                1.0,
            );
            draw_rectangle(0.0, horizon_y * k, sw, horizon_y / strips as f32 + 1.0, c);
        }
        let star_px = (sh / 760.0 * 2.0).max(1.5);
        for i in 0..54 {
            let cx = if hash01(i * 7) < 0.66 { sw * 0.22 } else { sw * 0.72 };
            let x = cx + (hash01(i * 3 + 1) - 0.5) * sw * 0.5;
            let y = hash01(i * 3 + 2) * horizon_y * 0.8;
            let tw = 0.5 + 0.5 * ((t * (1.0 + hash01(i) * 3.0) + i as f32).sin());
            let a = (0.08 + 0.32 * feat.treble) * tw;
            draw_rectangle(x, y, star_px, star_px, Color::new(spec().r, spec().g, spec().b, a));
        }
        let n = feat.bands.len();
        let bw = sw / n as f32;
        for (i, &e) in feat.bands.iter().enumerate() {
            let h = sh * (0.012 + e * e * 0.075);
            draw_rectangle(i as f32 * bw, horizon_y - h, bw * 0.92, h, Color::new(0.11, 0.09, 0.14, 1.0));
        }
        draw_rectangle(0.0, horizon_y, sw, sh - horizon_y, HORIZON);

        // ================= 3D pass ===========================================
        let fov = self.fov_s.to_radians();
        let cam_pos = vec3(px * 0.7, 2.7 + py * 0.25 + self.cam_kick_s * 0.12, 5.4);
        set_camera(&Camera3D {
            position: cam_pos,
            target: vec3(px * 0.85, 1.1 + py * 0.45, -8.0),
            up: vec3(bank, 1.0, 0.0).normalize(),
            fovy: fov,
            aspect: Some(view::screen_w() / view::screen_h()),
            render_target: view::export_target(),
            ..Default::default()
        });

        // ---- the ribbon: one FLAT triangle-strip road, rippled by the live
        // waveform. It stays flat (the avatar hops; the road does not lift), which
        // keeps the obstacles on the ground and the lit texture from swimming.
        const RN: usize = 72;
        let wave = ctx.wave;
        let mut verts: Vec<Vertex> = Vec::with_capacity(RN * 3);
        let mut idx: Vec<u16> = Vec::new();
        let mut edges: Vec<(Vec3, Vec3)> = Vec::with_capacity(RN);
        for i in 0..RN {
            let f = i as f32 / (RN - 1) as f32;
            let z = 2.0 - f * 70.0;
            let d = d_now - z; // z = -(d - d_now)
            let ripple = wave[(i * 3) % wave.len()] * 0.14 * (0.4 + feat.treble);
            let h = ripple; // FLAT road + a subtle live ripple (no obstacle humps)
            let load = prof.loudness_at(self.t_at_dist(d));
            // UN-fogged albedo: the PBR material lights + fogs it in the shader.
            let col = grade(0.08 + load * 0.5);
            for j in 0..3 {
                let fj = j as f32 / 2.0;
                let xo = (fj - 0.5) * 2.0 * ROAD_HALF;
                let lift = (fj - 0.5).abs() * 0.18; // gentle raised edges, won't poke the camera
                verts.push(Vertex::new(xo, h + lift, z, fj, d, col));
            }
            edges.push((vec3(-ROAD_HALF, h + 0.18, z), vec3(ROAD_HALF, h + 0.18, z)));
        }
        for i in 0..RN - 1 {
            for j in 0..2u16 {
                let a = (i * 3) as u16 + j;
                let b = (i * 3) as u16 + j + 1;
                let c = ((i + 1) * 3) as u16 + j + 1;
                let dd = ((i + 1) * 3) as u16 + j;
                idx.extend_from_slice(&[a, b, c, a, c, dd]);
            }
        }
        // Real per-vertex world normals (accumulated from the faces) so the
        // ribbon catches the light and the bump map has a frame to perturb.
        let mut nrm = vec![Vec3::ZERO; verts.len()];
        let mut tri = 0;
        while tri + 2 < idx.len() {
            let (ia, ib, ic) = (idx[tri] as usize, idx[tri + 1] as usize, idx[tri + 2] as usize);
            let fnv = (verts[ib].position - verts[ia].position).cross(verts[ic].position - verts[ia].position);
            nrm[ia] += fnv;
            nrm[ib] += fnv;
            nrm[ic] += fnv;
            tri += 3;
        }
        for (v, nn) in verts.iter_mut().zip(nrm) {
            let u = nn.normalize_or_zero();
            v.normal = vec4(u.x, u.y, u.z, 0.0);
        }
        material3d::bind(
            material3d::Surface::Ribbon,
            &material3d::LitParams {
                cam: cam_pos,
                light_dir: vec3(-0.35, -0.85, -0.4),
                light_color: { let a = mix(amber(), spec(), 0.5); vec3(a.r, a.g, a.b) },
                ambient: { let s = style::teal_deep(); vec3(s.r, s.g, s.b) * 0.55 },
                horizon: HORIZON,
                metal: 0.85,
                rough: 1.0,
                tile: vec2(2.0, 0.16),
                pulse: feat.bass * 0.6 + feat.beat.unwrap_or(0.0) * 0.4,
            },
        );
        draw_mesh(&Mesh { vertices: verts, indices: idx, texture: None });
        material3d::unbind();
        // Bright edge rails trace the ribbon for definition.
        for i in 0..RN - 1 {
            let z = 2.0 - i as f32 / (RN - 1) as f32 * 70.0;
            let ec = fog(Color::new((teal().r + 0.3 * feat.treble).min(1.0), teal().g, teal().b, 1.0), -z);
            draw_line_3d(edges[i].0, edges[i + 1].0, ec);
            draw_line_3d(edges[i].1, edges[i + 1].1, ec);
        }

        // ---- typed obstacle glyphs welded onto the ribbon -------------------
        for e in &self.course {
            let z = -(e.d - d_now);
            if z > 2.0 || z < -FAR {
                continue;
            }
            let x = self.weave(e.d);
            let base = 0.0; // hurdles sit on the FLAT ground; the avatar hops over them
            let near = (1.0 - ((e.t - t).abs() / 0.18).min(1.0)).max(0.0);
            let sn = ((e.strength - 1.3) / 2.7).clamp(0.0, 1.0);
            let c = mix(grade(0.58 + sn * 0.4), amber(), near * 0.55);
            // One clear, solid obstacle vocabulary: a HURDLE the runner hops, or a
            // GAP it strides over. No rings/teeth (those implied spins/rolls).
            match e.kind {
                Kind::Pit => draw_pit(x, base, z, ROAD_HALF * 0.7, c),
                _ => draw_hurdle(x, base, z, 1.3 + sn * 0.3, c),
            }
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
            let gold =
                fog(Color::new(coin.r + 0.08 * feat.treble, coin.g + 0.10 * feat.treble, coin.b, 1.0), -z);
            let origin = vec3(c.x, c.y, z) - e1 / 2.0 - e2 / 2.0 - e3 / 2.0;
            draw_affine_parallelepiped(origin, e1, e2, e3, None, gold);
        }

        // Collected-coin sparkles.
        for s in &self.sparkles {
            let z = -(s.d - d_now);
            let k = (s.life / 0.5).clamp(0.0, 1.0);
            draw_cube(vec3(s.x, s.y, z), vec3(0.07, 0.07, 0.07) * k, None, fog(Color::new(0.98, 0.85, 0.45, 1.0), -z));
        }

        // ---- the avatar: a line-art creature that HOPS the hurdles -----------
        // The hop reads the height field DIRECTLY (it's a smooth sine, no jitter)
        // so the apex is reached fully and clears the bar — the camera-smoothed
        // py would flatten the peak and the bunny would clip the hurdle.
        let pyj = self.surface_y(d_now);
        let (squash, leg) = (self.squash_s, self.leg_s);
        let grounded = pyj <= 0.05;
        let run = self.run_phase.sin();
        let bob = if grounded { run.abs() * 0.05 } else { 0.0 };
        let pivot = vec3(px, pyj + bob + 0.95, 0.0);

        // Shadow grounds the creature.
        draw_plane(vec3(px, 0.015, 0.0), vec2(0.34 * (1.0 - (pyj / 4.0).min(0.8)), 0.26), None, Color::new(0.0, 0.0, 0.0, 0.32));
        let beat = feat.beat.unwrap_or(0.0);
        let creature_c = mix(amber(), spec(), 0.45 + 0.25 * beat.min(1.0));
        draw_creature(pivot, 0.0, 0.0, squash, leg, run, grounded, creature_c);

        // Vignette over the composited 3D frame.
        view::apply_screen_camera();
        style::finish();
        set_default_camera();
    }
}
