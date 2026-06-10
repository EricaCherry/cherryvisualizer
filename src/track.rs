//! A loaded piece of music: mono PCM + an offline pre-analysis `Profile`.
//!
//! The profile is computed once at load time by walking the whole track:
//!   - a beat grid (timestamps + strengths) from low-passed energy peaks,
//!   - a per-hop loudness curve (so modes can know how loud the *future* is).
//!
//! Offline analysis is the trick that lets modes be played BY the music:
//! the runner places an obstacle at every future beat so it arrives exactly
//! on the beat; breakout kicks the ball on beats with frame accuracy.

use std::f32::consts::PI;
use std::fs::File;
use std::io::{BufReader, Write};
use std::path::Path;

/// One detected beat.
pub struct Beat {
    /// Seconds from track start.
    pub t: f32,
    /// How much the beat stood out from the local average (~1.3 .. 4).
    pub strength: f32,
}

/// Whole-track pre-analysis at `hop_dt` resolution.
///
/// Three stacked band curves (plus the beat grid) give modes independent
/// "layers" of the music to animate different things with: bass drives the
/// beat grid, `mid` carries the body of the song, `treb` carries hats/sparkle.
pub struct Profile {
    pub hop_dt: f32,
    /// Per-hop full-band RMS loudness.
    pub rms: Vec<f32>,
    pub max_rms: f32,
    /// Per-hop mid-band energy (~180 Hz – 2 kHz), normalized 0..1.
    /// (Choreography layer for future modes; treble and bass are in use today.)
    #[allow(dead_code)]
    pub mid: Vec<f32>,
    /// Per-hop treble energy (above ~2 kHz), normalized 0..1.
    pub treb: Vec<f32>,
    pub beats: Vec<Beat>,
}

impl Profile {
    pub fn analyze(pcm: &[f32], sr: u32) -> Self {
        const HOP: usize = 512;
        let hop_dt = HOP as f32 / sr as f32;
        let n_hops = pcm.len() / HOP;

        // Two one-pole low-passes split the signal into three stacked bands:
        // bass (< ~180 Hz, the kick register), mid (~180 Hz – 2 kHz), and
        // treble (the residue above ~2 kHz, hats and sparkle).
        let a_lo = 1.0 - (-2.0 * PI * 180.0 / sr as f32).exp();
        let a_hi = 1.0 - (-2.0 * PI * 2000.0 / sr as f32).exp();
        let (mut lp_lo, mut lp_hi) = (0.0f32, 0.0f32);

        let mut rms = Vec::with_capacity(n_hops);
        let mut low = Vec::with_capacity(n_hops);
        let mut mid = Vec::with_capacity(n_hops);
        let mut treb = Vec::with_capacity(n_hops);
        for h in 0..n_hops {
            let mut full = 0.0f32;
            let mut bass = 0.0f32;
            let mut mid_e = 0.0f32;
            let mut treb_e = 0.0f32;
            for &x in &pcm[h * HOP..(h + 1) * HOP] {
                lp_lo += a_lo * (x - lp_lo);
                lp_hi += a_hi * (x - lp_hi);
                let m = lp_hi - lp_lo;
                let t = x - lp_hi;
                full += x * x;
                bass += lp_lo * lp_lo;
                mid_e += m * m;
                treb_e += t * t;
            }
            rms.push((full / HOP as f32).sqrt());
            low.push((bass / HOP as f32).sqrt());
            mid.push((mid_e / HOP as f32).sqrt());
            treb.push((treb_e / HOP as f32).sqrt());
        }
        let max_rms = rms.iter().fold(0.0f32, |m, &v| m.max(v)).max(1e-6);
        for v in [&mut mid, &mut treb] {
            let max = v.iter().fold(0.0f32, |m, &x| m.max(x)).max(1e-6);
            for x in v.iter_mut() {
                *x /= max;
            }
        }

        // Beats: bass energy spikes over a ~1s trailing average, 220ms apart min.
        let mut beats = Vec::new();
        let mut hist_sum = 0.0f32;
        let mut hist = std::collections::VecDeque::new();
        let mut last_beat = -1.0f32;
        for (h, &e) in low.iter().enumerate() {
            let t = h as f32 * hop_dt;
            let avg = if hist.is_empty() { e } else { hist_sum / hist.len() as f32 };
            if e > avg * 1.35 && e > 0.015 && (last_beat < 0.0 || t - last_beat > 0.22) {
                beats.push(Beat { t, strength: (e / avg.max(1e-6)).min(4.0) });
                last_beat = t;
            }
            hist.push_back(e);
            hist_sum += e;
            if hist.len() > 43 {
                hist_sum -= hist.pop_front().unwrap();
            }
        }

        Profile { hop_dt, rms, max_rms, mid, treb, beats }
    }

    /// First beat in the half-open interval (t0, t1], if any.
    pub fn beat_in(&self, t0: f32, t1: f32) -> Option<f32> {
        let i = self.beats.partition_point(|b| b.t <= t0);
        let b = self.beats.get(i)?;
        (b.t <= t1).then_some(b.strength)
    }

    /// Loudness at time `t`, normalized 0..1 against the track's own peak.
    pub fn loudness_at(&self, t: f32) -> f32 {
        Self::sample(&self.rms, self.hop_dt, t) / self.max_rms
    }

    /// Normalized treble energy at time `t` (coin/sparkle layer).
    pub fn treble_at(&self, t: f32) -> f32 {
        Self::sample(&self.treb, self.hop_dt, t)
    }

    fn sample(curve: &[f32], hop_dt: f32, t: f32) -> f32 {
        if curve.is_empty() {
            return 0.0;
        }
        let f = (t / hop_dt).max(0.0);
        let i = (f as usize).min(curve.len() - 1);
        let j = (i + 1).min(curve.len() - 1);
        let frac = (f - i as f32).min(1.0);
        curve[i] * (1.0 - frac) + curve[j] * frac
    }
}

pub struct Track {
    pub name: String,
    pub pcm: Vec<f32>,
    pub sr: u32,
    pub profile: Profile,
}

impl Track {
    pub fn duration(&self) -> f32 {
        self.pcm.len() as f32 / self.sr as f32
    }

    /// Copy the window of samples starting at time `t` into `out` (zero-padded).
    pub fn window_at(&self, t: f32, out: &mut [f32]) {
        let start = ((t * self.sr as f32) as usize).min(self.pcm.len());
        let avail = (self.pcm.len() - start).min(out.len());
        out[..avail].copy_from_slice(&self.pcm[start..start + avail]);
        out[avail..].fill(0.0);
    }

    /// Decode any rodio-supported file (mp3/wav/flac/ogg/m4a) to a mono track.
    pub fn from_file(path: &Path) -> Result<Track, String> {
        use rodio::Source;
        let file = File::open(path).map_err(|e| format!("open: {e}"))?;
        let dec = rodio::Decoder::new(BufReader::new(file)).map_err(|e| format!("decode: {e}"))?;
        let channels = dec.channels().get() as usize;
        let sr = dec.sample_rate().get();
        let interleaved: Vec<f32> = dec.collect();
        if interleaved.is_empty() {
            return Err("decoded zero samples".into());
        }
        let pcm: Vec<f32> = interleaved
            .chunks(channels.max(1))
            .map(|frame| frame.iter().sum::<f32>() / frame.len() as f32)
            .collect();
        let profile = Profile::analyze(&pcm, sr);
        let name = path
            .file_name()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| "track".into());
        Ok(Track { name, pcm, sr, profile })
    }

    /// A 64-second synthetic groove (120 BPM) so Cherry is alive with no file.
    pub fn synth_demo() -> Track {
        let sr = 44_100u32;
        let secs = 64.0f32;
        let n = (sr as f32 * secs) as usize;
        let mut pcm = Vec::with_capacity(n);
        for i in 0..n {
            let t = i as f32 / sr as f32;
            let phase = (t * 2.0).fract(); // 120 BPM
            let kick = (1.0 - phase).powi(4);
            let bar = (t * 0.5).fract();
            let bass = (2.0 * PI * 55.0 * t).sin() * kick * 0.85;
            let arp_f = [220.0, 277.18, 329.63, 415.3][((t * 4.0) as usize) % 4];
            let arp = (2.0 * PI * arp_f * t).sin() * 0.14 * (1.0 - (t * 4.0).fract() * 0.7);
            let pad = (2.0 * PI * 110.0 * t).sin() * 0.05 * (0.5 + 0.5 * (bar * 2.0 * PI).sin());
            let hat = ((2.0 * PI * 6000.0 * t).sin() * (1.0 - (t * 4.0 + 0.5).fract()).powi(6)) * 0.05;
            pcm.push((bass + arp + pad + hat).clamp(-1.0, 1.0));
        }
        let profile = Profile::analyze(&pcm, sr);
        Track { name: "demo groove (press O to open a song)".into(), pcm, sr, profile }
    }
}

/// Write a small test WAV (s16 mono) so the decode path can be exercised in CI.
pub fn write_test_wav(path: &Path) -> std::io::Result<()> {
    let sr = 44_100u32;
    let secs = 12.0f32;
    let n = (sr as f32 * secs) as usize;
    let mut data = Vec::with_capacity(n * 2);
    for i in 0..n {
        let t = i as f32 / sr as f32;
        let phase = (t * 2.0).fract();
        let kick = (1.0 - phase).powi(4);
        let s = ((2.0 * PI * 55.0 * t).sin() * kick * 0.8
            + (2.0 * PI * 220.0 * t).sin() * 0.15)
            .clamp(-1.0, 1.0);
        data.extend_from_slice(&((s * 32_000.0) as i16).to_le_bytes());
    }
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let mut f = File::create(path)?;
    let byte_len = data.len() as u32;
    f.write_all(b"RIFF")?;
    f.write_all(&(36 + byte_len).to_le_bytes())?;
    f.write_all(b"WAVEfmt ")?;
    f.write_all(&16u32.to_le_bytes())?;
    f.write_all(&1u16.to_le_bytes())?; // PCM
    f.write_all(&1u16.to_le_bytes())?; // mono
    f.write_all(&sr.to_le_bytes())?;
    f.write_all(&(sr * 2).to_le_bytes())?; // byte rate
    f.write_all(&2u16.to_le_bytes())?; // block align
    f.write_all(&16u16.to_le_bytes())?; // bits
    f.write_all(b"data")?;
    f.write_all(&byte_len.to_le_bytes())?;
    f.write_all(&data)?;
    Ok(())
}
