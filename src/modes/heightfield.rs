//! A shared scrolling audio heightfield: a ring buffer of spectrum rows (newest
//! at the front, receding to the back) built into a lit triangle mesh each frame.
//! It is both the Terrain visualizer's landscape and the ground the Rail Shooter
//! flies over, so the game's terrain literally IS the music.

use std::collections::VecDeque;

use macroquad::prelude::*;

use crate::analysis::N_BANDS;
use crate::style::grade;

pub const ROWS: usize = 56; // depth / history length
pub const COLS: usize = 64; // width (the spectrum mirrored into a ridge)

pub struct HeightField {
    rows: VecDeque<[f32; COLS]>, // [0] = newest (front)
    cur: [f32; COLS],            // live smoothed front ridge
    scroll: f32,                 // sub-row scroll phase for smooth motion
}

impl HeightField {
    pub fn new() -> Self {
        let mut rows = VecDeque::with_capacity(ROWS + 1);
        for _ in 0..ROWS {
            rows.push_back([0.0; COLS]);
        }
        HeightField { rows, cur: [0.0; COLS], scroll: 0.0 }
    }

    pub fn reset(&mut self) {
        self.rows.clear();
        for _ in 0..ROWS {
            self.rows.push_back([0.0; COLS]);
        }
        self.cur = [0.0; COLS];
        self.scroll = 0.0;
    }

    /// Column -> band so the centre is bass (the tall ridge) and the edges treble.
    pub fn band_of(j: usize) -> usize {
        let d = ((j as f32 / (COLS - 1) as f32) - 0.5).abs() * 2.0;
        ((d * (N_BANDS - 1) as f32) as usize).min(N_BANDS - 1)
    }

    /// Advance the live front ridge toward the current spectrum and scroll.
    pub fn update(&mut self, bands: &[f32; N_BANDS], dt: f32, speed: f32) {
        let k = 1.0 - 0.5f32.powf(dt * 60.0);
        for j in 0..COLS {
            let target = bands[Self::band_of(j)];
            self.cur[j] += (target - self.cur[j]) * k;
        }
        self.scroll += dt * speed;
        while self.scroll >= 1.0 {
            self.rows.push_front(self.cur);
            if self.rows.len() > ROWS {
                self.rows.pop_back();
            }
            self.scroll -= 1.0;
        }
    }

    /// Build the lit heightmap mesh (per-vertex world normals + scrolling UVs).
    /// The mesh spans `terrain_w` wide and recedes from `near` over `depth`, with
    /// energy scaled to `height`. Bind a material around the returned mesh.
    pub fn build_mesh(&self, terrain_w: f32, near: f32, depth: f32, height: f32) -> (Vec<Vertex>, Vec<u16>) {
        let rs = depth / ROWS as f32;
        let mut verts: Vec<Vertex> = Vec::with_capacity(ROWS * COLS);
        let mut idx: Vec<u16> = Vec::with_capacity((ROWS - 1) * (COLS - 1) * 6);
        for (i, row) in self.rows.iter().enumerate() {
            let z = near - (i as f32 + self.scroll) * rs;
            for (j, &e) in row.iter().enumerate() {
                let x = (j as f32 / (COLS - 1) as f32 - 0.5) * terrain_w;
                let u = j as f32 / (COLS - 1) as f32 * 4.0;
                let vv = (i as f32 + self.scroll) * 0.12;
                verts.push(Vertex::new(x, e * height, z, u, vv, grade(0.2 + e * 0.72)));
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
        (verts, idx)
    }
}
