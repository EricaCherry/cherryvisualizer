//! Shared audio→course generator for the game modes (Beat Surfer, Rail Shooter).
//!
//! Vib-Ribbon's idea: the *song* designs the level. The whole track is turned
//! ONCE into a list of TYPED, WELL-SPACED obstacles living in cumulative-distance
//! space. Each beat is classified by its spectral character (heavy → jump,
//! the music opening up → a gap, sustained mid → a spin ring, fast/treble → a
//! roll), snapped to the tempo grid, then a **min-gap sweep in distance** drops
//! anything that crowds its neighbour.
//!
//! That sweep is the structural fix for the old "jumping at weird timings"
//! glitch: because every surviving obstacle is at least `min_gap_m` metres from
//! the last, and one clear-animation spans *less* than that in travel, no two
//! animation windows can ever overlap — the avatar's pose becomes a clean pure
//! function of distance, with no stacked parabolas to snap between.

use crate::track::Profile;

/// What the avatar (or the rail formation) does at an obstacle.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    /// Heavy / bass-dominant hit → an arch to JUMP.
    Block,
    /// A sharp loudness drop (the music opening up) → a GAP to stride.
    Pit,
    /// Sustained mid energy → a RING to spin through.
    Loop,
    /// Fast / treble-dominant hit → a low ROLL.
    Wave,
}

/// One obstacle, placed in distance-space (metres along the track).
#[derive(Clone, Copy)]
pub struct Ev {
    pub d: f32,
    pub t: f32,
    pub kind: Kind,
    pub strength: f32,
    /// The strongest hits carry a second, complementary action.
    pub double: Option<Kind>,
}

const LEAD_DROP: f32 = 0.20; // seconds after a beat to test for a loudness drop

/// Build the obstacle course from the offline profile.
///
/// `dist` is the cumulative-distance LUT the mode already integrates (one entry
/// per analysis hop). `min_gap_m` is the minimum spacing between obstacles in
/// metres; it MUST exceed one clear-animation's travel so windows can't overlap.
pub fn build_course(p: &Profile, dist: &[f32], dur: f32, min_gap_m: f32) -> Vec<Ev> {
    let hop = p.hop_dt;
    let dist_at = |t: f32| -> f32 {
        if dist.len() < 2 {
            return 0.0;
        }
        let f = (t / hop).max(0.0);
        let i = (f as usize).min(dist.len() - 2);
        let fr = (f - i as f32).min(1.0);
        dist[i] * (1.0 - fr) + dist[i + 1] * fr
    };

    // 1) Tempo period from inter-onset intervals (median, folded into 0.30..0.85s).
    let mut iois: Vec<f32> =
        p.beats.windows(2).map(|w| w[1].t - w[0].t).filter(|x| *x > 0.05).collect();
    iois.sort_by(|a, b| a.total_cmp(b));
    let mut period = if iois.is_empty() { 0.5 } else { iois[iois.len() / 2] };
    while period > 0.85 {
        period *= 0.5;
    }
    while period < 0.30 {
        period *= 2.0;
    }

    // 2) Doubles threshold ≈ 90th percentile of beat strength.
    let mut st: Vec<f32> = p.beats.iter().map(|b| b.strength).collect();
    st.sort_by(|a, b| a.total_cmp(b));
    let dbl_thr =
        if st.is_empty() { 3.9 } else { st[(st.len() * 90 / 100).min(st.len() - 1)].clamp(3.0, 3.9) };

    // 3) Raw candidates: type each beat by the spectral character at its time.
    let mut raw: Vec<Ev> = Vec::new();
    for b in &p.beats {
        if b.t < 1.2 || b.t > dur - 1.2 {
            continue;
        }
        // The beat channel IS the bass-onset strength; normalise it to 0..1.
        let bass = ((b.strength - 1.3) / 2.7).clamp(0.0, 1.0);
        let mid = p.mid_at(b.t);
        let treb = p.treble_at(b.t);
        let drop = p.loudness_at(b.t) - p.loudness_at(b.t + LEAD_DROP);
        // Strong hits are the jumps (the hero beats); treble texture rolls; held
        // mids spin; a real loudness drop opens a gap. Tuned so a typical track
        // yields a mix rather than 80% of any one type.
        let kind = if drop > 0.24 {
            Kind::Pit
        } else if bass >= 0.5 {
            Kind::Block
        } else if treb >= mid {
            Kind::Wave
        } else {
            Kind::Loop
        };
        raw.push(Ev { d: 0.0, t: b.t, kind, strength: b.strength, double: None });
    }

    // 4) Quantise each time to the half-beat grid (only when close, so it stays
    //    tight to the music instead of jittery), then resolve to distance.
    let half = period * 0.5;
    for e in &mut raw {
        let q = (e.t / half).round() * half;
        if (q - e.t).abs() < half * 0.35 {
            e.t = q;
        }
        e.d = dist_at(e.t);
    }
    raw.sort_by(|a, b| a.d.total_cmp(&b.d));

    // 5) Min-gap sweep in distance → non-overlap by construction (the bug fix).
    let mut out: Vec<Ev> = Vec::new();
    for e in raw {
        match out.last_mut() {
            Some(prev) if e.d - prev.d < min_gap_m => {
                if e.strength > prev.strength {
                    *prev = e; // keep the stronger hit; drop the crowd-in
                }
            }
            _ => out.push(e),
        }
    }

    // 6) Doubles on the strongest survivors: pair a complementary second action.
    for e in &mut out {
        if e.strength >= dbl_thr {
            e.double = Some(match e.kind {
                Kind::Block => Kind::Wave,
                Kind::Wave => Kind::Block,
                Kind::Loop => Kind::Pit,
                Kind::Pit => Kind::Loop,
            });
        }
    }

    debug_assert!(
        out.windows(2).all(|w| w[1].d - w[0].d >= min_gap_m - 0.001),
        "course events must be at least min_gap_m apart — the non-overlap guarantee"
    );
    out
}
