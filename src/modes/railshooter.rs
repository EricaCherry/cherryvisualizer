//! Rail Shooter — a StarFox-style on-rails space flight the music flies.
//!
//! Nobody holds the stick: the offline beat grid is the pilot. Exactly like
//! `surfer`, a loudness->speed curve is integrated into a cumulative distance
//! array so every event lives in distance-space and lands frame-exact on its
//! beat. Strong beats stream out enemy fighters; the Arwing's twin lasers are
//! fired EARLY (lead = the bolt's travel time) so the tracer strikes the enemy
//! exactly on the beat and it bursts into debris. Checkpoint rings flow past on
//! an even cadence, the canyon walls breathe with the bass, and the rarest hits
//! snap a full barrel roll.
//!
//! Rendering is surfer's recipe: a 2D sky/star/sun backdrop under the screen
//! camera, then a `Camera3D` drawing flat low-poly `box_outlined` shapes with
//! hand-rolled distance fog, then a 2D HUD reticle. Every color comes from the
//! theme accessors, so a theme switch re-skins the whole squadron.

use macroquad::prelude::*;

use crate::material3d;
use crate::modes::course::{Kind, build_course};
use crate::modes::{Category, FrameCtx, Mode, Param};
use crate::style::{self, amber, amber_glow, hash01, ink, mix, slate, spec, teal, teal_deep, with_alpha};
use crate::track::Track;
use crate::view;

// ---- world tuning -----------------------------------------------------------
const BASE_SPEED: f32 = 22.0; // m/s at average loudness (flight sim pace)
const ROAD_HALF: f32 = 4.4; // corridor half-width
const FAR: f32 = 92.0;
const LEAD: f32 = 0.36; // laser flight time: fire this early so it hits on the beat
const KILL_AHEAD: f32 = 11.0; // enemies die this far in front of the ship
const ROLL_DUR: f32 = 0.72;
const COURSE_GAP: f32 = 14.0; // min obstacle spacing (m); exceeds one action's travel

struct Enemy {
    d: f32,
    x: f32,
    y: f32,
    hit_t: f32,
    boss: bool,
}

struct Shot {
    fire_t: f32,
    hit_t: f32,
    tx: f32,
    ty: f32,
    td: f32,
    twin: bool,
    power: f32,
}

struct Ring {
    d: f32,
    gold: bool,
}

struct Roll {
    t: f32,
}

struct Rock {
    d: f32,
    x: f32,
    y: f32,
    r: f32,
    kind: i32,
}

struct Burst {
    d: f32,
    x: f32,
    y: f32,
    vx: f32,
    vy: f32,
    vz: f32,
    life: f32,
    hot: f32,
}

struct Shell {
    d: f32,
    x: f32,
    y: f32,
    age: f32,
}

pub struct RailShooter {
    hop_dt: f32,
    dist: Vec<f32>,
    course: Vec<crate::modes::course::Ev>,
    enemies: Vec<Enemy>,
    shots: Vec<Shot>,
    rings: Vec<Ring>,
    rolls: Vec<Roll>,
    rocks: Vec<Rock>,
    bursts: Vec<Burst>,
    shells: Vec<Shell>,
    prev_dead: usize,
    flash: f32,
    // live-tunable
    p_fire: f32,
    p_density: f32,
    p_roll: f32,
    p_reticle: f32,
}

impl RailShooter {
    pub fn new() -> Self {
        RailShooter {
            hop_dt: 1.0 / 60.0,
            dist: vec![0.0],
            course: Vec::new(),
            enemies: Vec::new(),
            shots: Vec::new(),
            rings: Vec::new(),
            rolls: Vec::new(),
            rocks: Vec::new(),
            bursts: Vec::new(),
            shells: Vec::new(),
            prev_dead: 0,
            flash: 0.0,
            p_fire: 1.0,
            p_density: 1.0,
            p_roll: 1.0,
            p_reticle: 1.0,
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

    /// Gentle sinusoidal weave of the ship (no input — it drifts like a pilot).
    fn ship_x(&self, t: f32) -> f32 {
        (t * 0.45).sin() * 1.4 + (t * 0.27).sin() * 0.6
    }

    /// The active barrel-roll angle at time `t` (0 when not rolling).
    fn roll_angle(&self, t: f32) -> f32 {
        let i = self.rolls.partition_point(|r| r.t <= t);
        if i == 0 {
            return 0.0;
        }
        let r = &self.rolls[i - 1];
        let k = (t - r.t) / ROLL_DUR;
        if !(0.0..1.0).contains(&k) {
            return 0.0;
        }
        std::f32::consts::TAU * (k * k * (3.0 - 2.0 * k)) // smoothstep -> one clean spin
    }

}

/// Distance fog toward a theme-derived deep-space horizon (exp-squared, soft).
fn fog(c: Color, dist: f32) -> Color {
    let horizon = mix(ink(), slate(), 0.7);
    let x = dist.max(0.0) * 0.024;
    let f = (1.0 - (-x * x).exp()).clamp(0.0, 1.0);
    mix(c, horizon, f)
}

/// Flat-shaded box with a darker wire outline (the low-poly read), fogged.
fn box_outlined(center: Vec3, size: Vec3, c: Color) {
    let dist = -center.z;
    draw_cube(center, size, None, fog(c, dist));
    if dist < FAR * 0.55 {
        draw_cube_wires(center, size * 1.01, fog(Color::new(c.r * 0.45, c.g * 0.45, c.b * 0.45, 1.0), dist));
    }
}

/// Enemy slot (x, y) within a wave of `n`, arranged by the event kind so each
/// volley reads as a recognisable formation rather than a random scatter.
fn formation(kind: Kind, k: i32, n: i32) -> (f32, f32) {
    let off = k as f32 - (n - 1) as f32 * 0.5;
    match kind {
        Kind::Block => (off * 1.5, 1.4),                  // a line abreast
        Kind::Wave => (off * 1.3, 1.0 + off.abs() * 0.7), // a V
        Kind::Loop => {
            let a = off * 0.5; // an arc
            (a.sin() * 2.2, 1.6 + (1.0 - a.cos()) * 1.8)
        }
        Kind::Pit => (0.0, 1.4),
    }
}

/// An angular fighter: a forward fuselage with two swept-back wings (built from
/// sheared parallelepipeds, so it reads as a dart, not a box).
fn draw_wedge(center: Vec3, s: f32, body: Color, wing: Color) {
    let d = -center.z;
    let pp = |ex: Vec3, ey: Vec3, ez: Vec3, off: Vec3, col: Color| {
        let o = center + off - ex * 0.5 - ey * 0.5 - ez * 0.5;
        draw_affine_parallelepiped(o, ex, ey, ez, None, fog(col, d));
    };
    // Fuselage, nose forward (+z).
    pp(vec3(0.2 * s, 0.0, 0.0), vec3(0.0, 0.16 * s, 0.0), vec3(0.0, 0.0, 1.0 * s), Vec3::ZERO, body);
    // Swept wings (extend out and back).
    for side in [-1.0f32, 1.0] {
        pp(
            vec3(side * 0.78 * s, 0.0, -0.45 * s),
            vec3(0.0, 0.05 * s, 0.0),
            vec3(0.0, 0.0, 0.34 * s),
            vec3(side * 0.12 * s, 0.0, -0.12 * s),
            wing,
        );
    }
}

impl Mode for RailShooter {
    fn name(&self) -> &'static str {
        "Rail Shooter"
    }

    fn about(&self) -> &'static str {
        "A StarFox-style rails shooter the music flies: beats fire the lasers and stream the enemies."
    }

    fn category(&self) -> Category {
        Category::Game
    }

    fn own_background(&self) -> bool {
        true
    }

    fn params(&self) -> Vec<Param> {
        vec![
            Param::float("Fire rate", self.p_fire, 0.3, 2.0),
            Param::float("Enemy density", self.p_density, 0.3, 2.0),
            Param::float("Roll", self.p_roll, 0.0, 2.0),
            Param::int("Reticle", self.p_reticle as i32, 0, 1),
        ]
    }

    fn set_param(&mut self, name: &str, v: f32) {
        match name {
            "Fire rate" => self.p_fire = v,
            "Enemy density" => self.p_density = v,
            "Roll" => self.p_roll = v,
            "Reticle" => self.p_reticle = v,
            _ => {}
        }
    }

    fn reset(&mut self, track: &Track) {
        let p = &track.profile;
        self.hop_dt = p.hop_dt;

        // World speed follows loudness; integrate into cumulative distance.
        self.dist.clear();
        self.dist.push(0.0);
        let mut d = 0.0f32;
        for h in 0..p.rms.len() {
            let speed = BASE_SPEED * (0.6 + 0.9 * p.loudness_at(h as f32 * p.hop_dt));
            d += speed * p.hop_dt;
            self.dist.push(d);
        }

        // The song designs the course: typed, distance-spaced events (the same
        // generator Beat Surfer uses, so the spacing is guaranteed non-crowding).
        self.course = build_course(p, &self.dist, track.duration(), COURSE_GAP);

        // ---- each event -> an enemy WAVE (formation by kind) + lead-fired
        //      volley; flow rings on the open events; a roll on the big doubles.
        self.enemies.clear();
        self.shots.clear();
        self.rolls.clear();
        self.rings.clear();
        let mut last_roll = -10.0f32;
        for (ei, e) in self.course.iter().enumerate() {
            let power = p.loudness_at(e.t);
            let twin = power > 0.45;
            let dd = e.d + KILL_AHEAD;
            // A whole volley shares one hit_t, so the wave reads as a musical
            // PHRASE rather than a spray. Pit is an open gate: a ring, no ships.
            let n = if e.kind == Kind::Pit {
                0
            } else {
                ((1.0 + (e.strength - 1.5) * 1.3) * self.p_density).round().clamp(1.0, 5.0) as i32
            };
            for k in 0..n {
                let (x, y) = formation(e.kind, k, n);
                self.enemies.push(Enemy { d: dd, x, y, hit_t: e.t, boss: e.double.is_some() });
                self.shots.push(Shot { fire_t: e.t - LEAD, hit_t: e.t, tx: x, ty: y, td: dd, twin, power });
            }
            // Flow rings on the open/melodic events (Loop/Pit) or every 4th hit.
            if matches!(e.kind, Kind::Loop | Kind::Pit) || ei % 4 == 3 {
                self.rings.push(Ring { d: e.d, gold: e.double.is_some() });
            }
            // The strongest doubles snap a barrel roll — distance-spaced for free.
            if e.double.is_some() && e.t - last_roll > 2.5 {
                self.rolls.push(Roll { t: e.t });
                last_roll = e.t;
            }
        }

        // ---- asteroid scenery on mid-band runs -----------------------------
        self.rocks.clear();
        let mut last_rock = -1.0f32;
        for h in 0..p.rms.len() {
            let t = h as f32 * p.hop_dt;
            if t < 1.0 || p.mid_at(t) < 0.5 || t - last_rock < 0.3 {
                continue;
            }
            last_rock = t;
            let hi = h as i32;
            let side = if hash01(hi) < 0.5 { -1.0 } else { 1.0 };
            self.rocks.push(Rock {
                d: self.dist_at(t) + hash01(hi * 3) * 6.0,
                x: side * (ROAD_HALF + 3.0 + hash01(hi * 5) * 7.0),
                y: -1.0 + hash01(hi * 7) * 6.0,
                r: 0.6 + hash01(hi * 11) * 1.4,
                kind: hi,
            });
        }

        self.bursts.clear();
        self.shells.clear();
        self.prev_dead = 0;
        self.flash = 0.0;
    }

    fn update(&mut self, ctx: &FrameCtx) {
        let dt = ctx.dt;
        let t = ctx.time;
        self.flash = (self.flash - dt * 2.5).max(0.0);

        // Enemies whose hit moment just passed -> spawn debris + a shockwave.
        let dead = self.enemies.partition_point(|e| e.hit_t <= t);
        if dead > self.prev_dead {
            for e in &self.enemies[self.prev_dead..dead] {
                let n = if e.boss { 16 } else { 11 };
                for k in 0..n {
                    let a = k as f32 / n as f32 * std::f32::consts::TAU;
                    self.bursts.push(Burst {
                        d: e.d,
                        x: e.x,
                        y: e.y,
                        vx: a.cos() * (1.5 + hash01(k * 7) * 2.5),
                        vy: a.sin() * (1.5 + hash01(k * 13) * 2.5),
                        vz: (hash01(k * 17) - 0.5) * 6.0,
                        life: 0.55 + hash01(k * 3) * 0.25,
                        hot: 1.0,
                    });
                }
                self.shells.push(Shell { d: e.d, x: e.x, y: e.y, age: 0.0 });
                if e.boss {
                    self.flash = self.flash.max(0.5);
                }
            }
        }
        self.prev_dead = dead;

        for b in &mut self.bursts {
            b.x += b.vx * dt;
            b.y += b.vy * dt;
            b.d += b.vz * dt;
            b.vy -= 9.0 * dt;
            b.life -= dt;
            b.hot = (b.hot - dt * 2.0).max(0.0);
        }
        self.bursts.retain(|b| b.life > 0.0);
        for s in &mut self.shells {
            s.age += dt;
        }
        self.shells.retain(|s| s.age < 0.7);
    }

    fn draw(&self, ctx: &FrameCtx) {
        let t = ctx.time;
        let feat = ctx.feat;
        let d_now = self.dist_at(t);
        let px = self.ship_x(t);
        let bank = (self.ship_x(t + 0.03) - self.ship_x(t - 0.03)) / 0.06 * 0.10;
        let roll = if self.p_roll < 0.05 { 0.0 } else { self.roll_angle(t) };

        // ================= 2D backdrop (screen camera) =======================
        view::apply_screen_camera();
        clear_background(ink());
        let (sw, sh) = (view::screen_w(), view::screen_h());
        let horizon_y = sh * 0.5;
        let deep = mix(ink(), slate(), 0.8);
        for i in 0..14 {
            let k = i as f32 / 14.0;
            draw_rectangle(0.0, horizon_y * k, sw, horizon_y / 14.0 + 1.0, mix(ink(), deep, k));
        }
        // Stars, gathered into two loose drifts, twinkling on treble.
        let star_px = (sh / 760.0 * 2.0).max(1.5);
        for i in 0..70 {
            let cx = if hash01(i * 7) < 0.6 { sw * 0.28 } else { sw * 0.74 };
            let x = cx + (hash01(i * 3 + 1) - 0.5) * sw * 0.55;
            let y = hash01(i * 3 + 2) * sh * 0.92;
            let tw = 0.5 + 0.5 * ((t * (1.0 + hash01(i) * 3.0) + i as f32).sin());
            draw_rectangle(x, y, star_px, star_px, with_alpha(spec(), (0.06 + 0.34 * feat.treble) * tw));
        }
        // Far asteroid-belt silhouette = the spectrum.
        let bw = sw / feat.bands.len() as f32;
        for (i, &e) in feat.bands.iter().enumerate() {
            let h = sh * (0.01 + e * e * 0.06);
            draw_rectangle(i as f32 * bw, horizon_y - h, bw * 0.92, h, mix(ink(), teal_deep(), 0.8));
        }

        // ================= 3D pass ===========================================
        // Static camera: a steady FOV and height — no beat zoom, no beat kick.
        // (px is the slow ship weave; roll is the deliberate barrel-roll move.)
        let fov = 62.0_f32.to_radians();
        let up = vec3((roll + bank).sin(), (roll + bank).cos(), 0.0).normalize();
        let cam_pos = vec3(px * 0.7, 2.3, 5.4);
        set_camera(&Camera3D {
            position: cam_pos,
            target: vec3(px * 0.85, 1.05, -8.0),
            up,
            fovy: fov,
            aspect: Some(view::screen_w() / view::screen_h()),
            render_target: view::export_target(),
            ..Default::default()
        });

        // Corridor floor + breathing walls — explicit meshes carrying real
        // normals, drawn through the PBR panel material (greeble normal map, lit
        // and fogged in-shader). UVs bake in the tiling so one draw covers the
        // whole run. The corridor geometry is STATIC — it does not pulse with the
        // beat (the music is read by the ship/enemies/lasers, not the walls).
        let floor_c = mix(teal_deep(), teal(), 0.18);
        let wall_c = mix(slate(), teal_deep(), 0.3);
        let wall_h = 3.4;
        let (zn, zf) = (6.0f32, -FAR);
        let vz = 0.12; // along-corridor tiling rate
        let mut cv: Vec<Vertex> = Vec::with_capacity(12);
        let mut ci: Vec<u16> = Vec::new();
        let mut quad = |c: [Vec3; 4], uv: [(f32, f32); 4], nrm: Vec3, col: Color| {
            let b = cv.len() as u16;
            for k in 0..4 {
                let mut v = Vertex::new(c[k].x, c[k].y, c[k].z, uv[k].0, uv[k].1, col);
                v.normal = vec4(nrm.x, nrm.y, nrm.z, 0.0);
                cv.push(v);
            }
            ci.extend_from_slice(&[b, b + 1, b + 2, b, b + 2, b + 3]);
        };
        quad(
            [vec3(-ROAD_HALF, -0.6, zn), vec3(ROAD_HALF, -0.6, zn), vec3(ROAD_HALF, -0.6, zf), vec3(-ROAD_HALF, -0.6, zf)],
            [(0.0, -zn * vz), (3.0, -zn * vz), (3.0, -zf * vz), (0.0, -zf * vz)],
            vec3(0.0, 1.0, 0.0),
            floor_c,
        );
        for side in [-1.0f32, 1.0] {
            let xw = side * (ROAD_HALF + 0.05);
            let (yb, yt) = (-0.6, wall_h - 0.6);
            quad(
                [vec3(xw, yb, zn), vec3(xw, yt, zn), vec3(xw, yt, zf), vec3(xw, yb, zf)],
                [(0.0, -zn * vz), (1.5, -zn * vz), (1.5, -zf * vz), (0.0, -zf * vz)],
                vec3(-side, 0.0, 0.0),
                wall_c,
            );
        }
        material3d::bind(
            material3d::Surface::Panel,
            &material3d::LitParams {
                cam: cam_pos,
                light_dir: vec3(-0.3, -0.7, -0.5),
                light_color: { let c = mix(teal(), spec(), 0.6); vec3(c.r, c.g, c.b) * 1.3 },
                ambient: { let s = mix(slate(), teal_deep(), 0.4); vec3(s.r, s.g, s.b) * 0.95 },
                horizon: mix(ink(), slate(), 0.7),
                metal: 0.6,
                rough: 0.9,
                tile: vec2(1.0, 1.0),
                pulse: 0.0, // corridor lighting is STATIC — it must not react to the beat
            },
        );
        draw_mesh(&Mesh { vertices: cv, indices: ci, texture: None });
        material3d::unbind();

        // Asteroid scenery (mid-band), tumbling at the sides.
        for r in &self.rocks {
            let z = -(r.d - d_now);
            if !(-FAR..=2.0).contains(&z) {
                continue;
            }
            let s = r.r * (0.7 + 0.45 * feat.mid);
            let spin = t * 0.6 + r.kind as f32;
            box_outlined(vec3(r.x, r.y, z), vec3(s, s * 0.8, s) * (1.0 + 0.1 * spin.sin()), mix(slate(), ink(), 0.5));
        }

        // Checkpoint rings (silver / rare gold), shimmering on treble.
        for r in &self.rings {
            let z = -(r.d - d_now);
            if !(-FAR..=2.0).contains(&z) {
                continue;
            }
            let passing = (d_now - r.d).abs() < 1.2;
            let base = if r.gold { amber() } else { teal() };
            let c = if passing { mix(base, spec(), 0.7) } else { mix(base, spec(), 0.2 + 0.35 * feat.treble) };
            let rr = 3.0 + 0.2 * feat.mid;
            let segs = 18;
            for k in 0..segs {
                let a = k as f32 / segs as f32 * std::f32::consts::TAU;
                box_outlined(vec3(a.cos() * rr, 1.3 + a.sin() * rr, z), vec3(0.22, 0.22, 0.5), c);
            }
        }

        // Enemy fighters (skip the already-shot), warm so they pop vs allies.
        for e in &self.enemies {
            if e.hit_t <= t {
                continue;
            }
            let z = -(e.d - d_now);
            if !(-FAR..=2.0).contains(&z) {
                continue;
            }
            let s = if e.boss { 1.35 + 0.3 * feat.bass } else { 1.0 };
            draw_wedge(vec3(e.x, e.y, z), s, amber(), mix(amber(), ink(), 0.35));
            if e.boss {
                draw_cube_wires(vec3(e.x, e.y, z), vec3(1.7, 1.1, 1.7) * s, with_alpha(amber_glow(), 0.5));
            }
        }

        // The Arwing — low-center, ahead of the camera.
        let o = vec3(px * 0.85, 1.0, -1.4);
        let bx = |c: Vec3, s: Vec3, col: Color| box_outlined(o + c, s, col);
        bx(vec3(0.0, 0.0, 0.0), vec3(0.45, 0.32, 2.0), slate()); // fuselage
        bx(vec3(0.0, 0.0, 1.2), vec3(0.16, 0.13, 0.7), ink()); // nose
        bx(vec3(0.0, 0.16, 0.35), vec3(0.26, 0.2, 0.5), teal_deep()); // canopy
        draw_cube(o + vec3(0.0, 0.28, 0.55), vec3(0.28, 0.04, 0.06), None, amber()); // canopy bar
        bx(vec3(-0.95, 0.0, -0.25), vec3(1.3, 0.07, 0.85), slate()); // left wing
        bx(vec3(0.95, 0.0, -0.25), vec3(1.3, 0.07, 0.85), slate()); // right wing
        bx(vec3(-1.5, 0.34, -0.4), vec3(0.07, 0.7, 0.5), ink()); // left fin
        bx(vec3(1.5, 0.34, -0.4), vec3(0.07, 0.7, 0.5), ink()); // right fin
        bx(vec3(-0.5, -0.18, -0.3), vec3(0.2, 0.16, 0.7), teal()); // left pod
        bx(vec3(0.5, -0.18, -0.3), vec3(0.2, 0.16, 0.7), teal()); // right pod
        let glow = mix(teal_deep(), amber(), feat.rms);
        let eg = 0.16 * (1.0 + 0.7 * feat.bass);
        draw_cube(o + vec3(-0.3, -0.05, -1.05), vec3(eg, eg, 0.12), None, glow);
        draw_cube(o + vec3(0.3, -0.05, -1.05), vec3(eg, eg, 0.12), None, glow);

        // Ally lasers — fired with lead so they strike on the beat.
        for s in &self.shots {
            if t < s.fire_t || t >= s.hit_t {
                continue;
            }
            let ez = -(s.td - d_now);
            if ez < -FAR {
                continue;
            }
            let k = ((t - s.fire_t) / (s.hit_t - s.fire_t)).clamp(0.0, 1.0);
            let target = vec3(s.tx, s.ty, ez);
            let c = style::grade((0.35 + 0.55 * s.power).clamp(0.0, 0.95));
            let muzzles: &[f32] = if s.twin { &[-0.95, 0.95] } else { &[0.0] };
            for &mxo in muzzles {
                let muzzle = o + vec3(mxo, 0.0, 0.9);
                let pos = muzzle + (target - muzzle) * k;
                draw_cube(pos, vec3(0.1, 0.1, 0.6), None, c);
                draw_line_3d(muzzle, pos, with_alpha(c, 0.5));
            }
        }

        // Explosions: debris cubes + an expanding wire shockwave.
        for b in &self.bursts {
            let z = -(b.d - d_now);
            if z < -FAR {
                continue;
            }
            let k = (b.life / 0.8).clamp(0.0, 1.0);
            let c = mix(style::active().ember_shadow, mix(amber(), spec(), b.hot), k);
            draw_cube(vec3(b.x, b.y, z), vec3(0.12, 0.12, 0.12) * k, None, fog(c, -z));
        }
        for s in &self.shells {
            let z = -(s.d - d_now);
            if z < -FAR {
                continue;
            }
            let rr = 0.3 + s.age * 2.2;
            let a = (1.0 - s.age / 0.7).clamp(0.0, 1.0) * 0.6;
            draw_cube_wires(vec3(s.x, s.y, z), vec3(rr, rr, rr), with_alpha(mix(amber(), spec(), 0.3), a));
        }

        // ================= 2D HUD ===========================================
        view::apply_screen_camera();
        if self.p_reticle > 0.5 {
            let (cx, cy) = (sw * 0.5, sh * 0.46);
            let r = sh * 0.05;
            let c = with_alpha(spec(), 0.7);
            for (dx, dy) in [(-1.0f32, -1.0f32), (1.0, -1.0), (-1.0, 1.0), (1.0, 1.0)] {
                draw_line(cx + dx * r, cy + dy * r, cx + dx * r * 0.55, cy + dy * r, 2.0, c);
                draw_line(cx + dx * r, cy + dy * r, cx + dx * r, cy + dy * r * 0.55, 2.0, c);
            }
            draw_circle_lines(cx, cy - sh * 0.10, sh * 0.018 + feat.treble * 6.0, 1.5, with_alpha(amber(), 0.7));
        }
        // Shield/boost bar (rms).
        draw_rectangle(sw * 0.04, sh * 0.93, sw * 0.16 * feat.rms.clamp(0.05, 1.0), sh * 0.012, with_alpha(teal(), 0.7));
        // Smart-bomb screen flash.
        if self.flash > 0.001 {
            draw_rectangle(0.0, 0.0, sw, sh, with_alpha(spec(), 0.5 * self.flash));
        }

        style::finish();
        set_default_camera();
    }
}
