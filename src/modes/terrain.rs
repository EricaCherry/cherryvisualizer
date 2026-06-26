//! Terrain — an audio landscape. Each moment the 32-band spectrum becomes a new
//! ridgeline at the FRONT of a heightmap that scrolls away to the horizon, so the
//! music extrudes a flowing mountain range (bass piles up the central ridge,
//! treble ripples the flanks). It's a real lit mesh — per-vertex world normals
//! through the PBR material, graded by energy, fogged into a dusk sky.

use std::collections::VecDeque;

use macroquad::prelude::*;

use crate::analysis::N_BANDS;
use crate::material3d;
use crate::modes::{Category, FrameCtx, Mode, Param};
use crate::style::{self, amber, grade, hash01, mix, spec, teal_deep};
use crate::track::Track;
use crate::view;

const ROWS: usize = 56; // depth / history length
const COLS: usize = 64; // width (the spectrum mirrored into a ridge)
const TERRAIN_W: f32 = 24.0;
const NEAR: f32 = 5.0;
const DEPTH: f32 = 68.0;
const SKY_HORIZON: Color = Color::new(0.10, 0.095, 0.12, 1.0);

pub struct Terrain {
    rows: VecDeque<[f32; COLS]>, // [0] = newest (front), receding to the back
    cur: [f32; COLS],            // live, smoothed front ridge
    scroll: f32,                 // sub-row scroll phase for smooth motion
    height: f32,
    speed: f32,
}

impl Terrain {
    pub fn new() -> Self {
        let mut rows = VecDeque::with_capacity(ROWS + 1);
        for _ in 0..ROWS {
            rows.push_back([0.0; COLS]);
        }
        Terrain { rows, cur: [0.0; COLS], scroll: 0.0, height: 5.2, speed: 22.0 }
    }

    /// Column -> band so the centre is bass (the tall ridge) and the edges treble.
    fn band_of(j: usize) -> usize {
        let d = ((j as f32 / (COLS - 1) as f32) - 0.5).abs() * 2.0; // 0 centre .. 1 edge
        ((d * (N_BANDS - 1) as f32) as usize).min(N_BANDS - 1)
    }
}

impl Mode for Terrain {
    fn name(&self) -> &'static str {
        "Terrain"
    }
    fn about(&self) -> &'static str {
        "An audio landscape — the spectrum extrudes flowing mountains that scroll to the horizon."
    }
    fn category(&self) -> Category {
        Category::Visualizer
    }
    fn own_background(&self) -> bool {
        true
    }

    fn params(&self) -> Vec<Param> {
        vec![
            Param::float("Height", self.height, 1.0, 9.0),
            Param::float("Scroll speed", self.speed, 8.0, 40.0),
        ]
    }
    fn set_param(&mut self, name: &str, v: f32) {
        match name {
            "Height" => self.height = v,
            "Scroll speed" => self.speed = v,
            _ => {}
        }
    }

    fn reset(&mut self, _t: &Track) {
        self.rows.clear();
        for _ in 0..ROWS {
            self.rows.push_back([0.0; COLS]);
        }
        self.cur = [0.0; COLS];
        self.scroll = 0.0;
    }

    fn update(&mut self, ctx: &FrameCtx) {
        let k = 1.0 - 0.5f32.powf(ctx.dt * 60.0); // light EMA on the live front ridge
        for j in 0..COLS {
            let target = ctx.feat.bands[Self::band_of(j)];
            self.cur[j] += (target - self.cur[j]) * k;
        }
        self.scroll += ctx.dt * self.speed;
        while self.scroll >= 1.0 {
            self.rows.push_front(self.cur);
            if self.rows.len() > ROWS {
                self.rows.pop_back();
            }
            self.scroll -= 1.0;
        }
    }

    fn draw(&self, ctx: &FrameCtx) {
        let feat = ctx.feat;
        let t = ctx.time;
        let sky_top = style::ink();

        // ---- 2D backdrop (dusk sky, stars, a low sun glow) ------------------
        view::apply_screen_camera();
        clear_background(sky_top);
        let (sw, sh) = (view::screen_w(), view::screen_h());
        let horizon_y = sh * 0.42;
        let strips = 12;
        for i in 0..strips {
            let f = i as f32 / strips as f32;
            let c = Color::new(
                sky_top.r + (SKY_HORIZON.r - sky_top.r) * f,
                sky_top.g + (SKY_HORIZON.g - sky_top.g) * f,
                sky_top.b + (SKY_HORIZON.b - sky_top.b) * f,
                1.0,
            );
            draw_rectangle(0.0, horizon_y * f, sw, horizon_y / strips as f32 + 1.0, c);
        }
        let star_px = (sh / 760.0 * 2.0).max(1.5);
        for i in 0..44 {
            let x = hash01(i * 3 + 1) * sw;
            let y = hash01(i * 3 + 2) * horizon_y * 0.8;
            let tw = 0.5 + 0.5 * ((t * (1.0 + hash01(i) * 2.0) + i as f32).sin());
            let a = (0.06 + 0.25 * feat.treble) * tw;
            draw_rectangle(x, y, star_px, star_px, Color::new(spec().r, spec().g, spec().b, a));
        }
        draw_rectangle(0.0, horizon_y, sw, sh - horizon_y, SKY_HORIZON);
        let (sx, sy) = (sw * 0.5, horizon_y * 0.92);
        let sun_r = sh * (0.11 + 0.03 * feat.bass);
        draw_circle(sx, sy, sun_r * 1.3, Color::new(amber().r, amber().g, amber().b, 0.22));
        draw_circle(sx, sy, sun_r, Color::new(amber().r, amber().g, amber().b, 0.92));
        draw_circle(sx, sy, sun_r * 0.6, Color::new(style::amber_glow().r, style::amber_glow().g, style::amber_glow().b, 0.95));
        draw_circle(sx, sy, sun_r * 0.3, Color::new(spec().r, spec().g, spec().b, 0.9));

        // ---- 3D pass --------------------------------------------------------
        let cam_pos = vec3(0.0, 5.2 + feat.bass * 0.6, 9.0);
        set_camera(&Camera3D {
            position: cam_pos,
            target: vec3(0.0, 1.6, -30.0),
            up: vec3(0.0, 1.0, 0.0),
            fovy: (58.0 + feat.rms * 6.0).to_radians(),
            aspect: Some(view::screen_w() / view::screen_h()),
            render_target: view::export_target(),
            ..Default::default()
        });

        // Build the heightmap mesh from the spectrum rows.
        let rs = DEPTH / ROWS as f32;
        let mut verts: Vec<Vertex> = Vec::with_capacity(ROWS * COLS);
        let mut idx: Vec<u16> = Vec::with_capacity((ROWS - 1) * (COLS - 1) * 6);
        for (i, row) in self.rows.iter().enumerate() {
            let z = NEAR - (i as f32 + self.scroll) * rs;
            for (j, &e) in row.iter().enumerate() {
                let x = (j as f32 / (COLS - 1) as f32 - 0.5) * TERRAIN_W;
                let u = j as f32 / (COLS - 1) as f32 * 4.0;
                let vv = (i as f32 + self.scroll) * 0.12;
                verts.push(Vertex::new(x, e * self.height, z, u, vv, grade(0.2 + e * 0.72)));
            }
        }
        let nrows = self.rows.len();
        for i in 0..nrows - 1 {
            for j in 0..COLS - 1 {
                let a = (i * COLS + j) as u16;
                let b = (i * COLS + j + 1) as u16;
                let c = ((i + 1) * COLS + j + 1) as u16;
                let d = ((i + 1) * COLS + j) as u16;
                idx.extend_from_slice(&[a, b, c, a, c, d]);
            }
        }
        // Per-vertex world normals (accumulated from the faces) so it catches light.
        let mut nrm = vec![Vec3::ZERO; verts.len()];
        let mut ti = 0;
        while ti + 2 < idx.len() {
            let (ia, ib, ic) = (idx[ti] as usize, idx[ti + 1] as usize, idx[ti + 2] as usize);
            let fnv = (verts[ib].position - verts[ia].position).cross(verts[ic].position - verts[ia].position);
            nrm[ia] += fnv;
            nrm[ib] += fnv;
            nrm[ic] += fnv;
            ti += 3;
        }
        for (v, n) in verts.iter_mut().zip(nrm) {
            let u = n.normalize_or_zero();
            v.normal = vec4(u.x, u.y, u.z, 0.0);
        }

        material3d::bind(
            material3d::Surface::Ribbon,
            &material3d::LitParams {
                cam: cam_pos,
                light_dir: vec3(-0.3, -0.85, -0.35),
                light_color: { let a = mix(amber(), spec(), 0.4); vec3(a.r, a.g, a.b) },
                ambient: { let s = teal_deep(); vec3(s.r, s.g, s.b) * 1.0 },
                horizon: SKY_HORIZON,
                metal: 0.3,
                rough: 0.95,
                tile: vec2(1.0, 1.0),
                pulse: feat.bass * 0.5 + feat.beat.unwrap_or(0.0) * 0.4,
            },
        );
        draw_mesh(&Mesh { vertices: verts, indices: idx, texture: None });
        material3d::unbind();

        view::apply_screen_camera();
        style::finish();
        set_default_camera();
    }
}
