//! A shared scrolling audio heightfield: a ring buffer of spectrum rows (newest
//! at the front, receding to the back) built into a lit triangle mesh each frame.
//! It is both the Terrain visualizer's landscape and the ground the Rail Shooter
//! flies over, so the game's terrain literally IS the music.

use std::collections::VecDeque;

use macroquad::prelude::*;

use crate::analysis::N_BANDS;
use crate::style::grade;

pub const ROWS: usize = 64; // depth / history length
pub const COLS: usize = 96; // width (the spectrum mirrored into a ridge)

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

    /// Column -> fractional band (centre = bass ridge, edges = treble), with
    /// linear interpolation between the two straddled bands. Nearest-neighbour
    /// gave runs of columns identical heights — terraced "teeth", not ridges.
    fn sample(bands: &[f32; N_BANDS], j: usize) -> f32 {
        let d = ((j as f32 / (COLS - 1) as f32) - 0.5).abs() * 2.0;
        let fb = d * (N_BANDS - 1) as f32;
        let i0 = fb as usize;
        let i1 = (i0 + 1).min(N_BANDS - 1);
        let fr = fb - i0 as f32;
        bands[i0] * (1.0 - fr) + bands[i1] * fr
    }

    /// Advance the live front ridge toward the current spectrum and scroll.
    pub fn update(&mut self, bands: &[f32; N_BANDS], dt: f32, speed: f32) {
        let k = 1.0 - 0.5f32.powf(dt * 60.0);
        // Shape the dB-scale bands (music sits ~0.6-1.0 across the board) into
        // a landscape: the gamma opens real valleys between ridges, and the
        // edge taper grounds the flanks so the centre range reads as mountains
        // rather than a full-width wall of cliffs at the camera's eye line.
        let mut tgt = [0.0f32; COLS];
        for (j, t) in tgt.iter_mut().enumerate() {
            let d = ((j as f32 / (COLS - 1) as f32) - 0.5).abs() * 2.0;
            // Pin the outermost columns flat so the mesh boundary can never
            // show up as a lit side wall at the frame edge.
            let edge = 1.0 - ((d - 0.92) / 0.08).clamp(0.0, 1.0);
            *t = Self::sample(bands, j).powf(2.2) * (1.0 - d * d * 0.75) * edge;
        }
        for j in 0..COLS {
            // One [1,2,1] pass: a single hot band becomes a peak with
            // shoulders instead of a lone comb tooth.
            let l = tgt[j.saturating_sub(1)];
            let r = tgt[(j + 1).min(COLS - 1)];
            let sm = l * 0.25 + tgt[j] * 0.5 + r * 0.25;
            self.cur[j] += (sm - self.cur[j]) * k;
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
                // v advances fast enough that slope texels don't stretch into
                // lengthwise streaks.
                let vv = (i as f32 + self.scroll) * 0.22;
                // Cap the crest grade at amber — full cream turns the lit
                // silhouette facets into blown-out chips.
                verts.push(Vertex::new(x, e * height, z, u, vv, grade(0.15 + e * 0.62)));
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
