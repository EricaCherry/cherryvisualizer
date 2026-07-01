//! Tiny persisted settings — theme, volume, chrome and the recent-files list
//! survive a restart. A hand-rolled `key=value` file (no serde dependency) in
//! the platform config dir; unknown keys are ignored so old builds read new
//! files and vice versa. All IO is best-effort: a missing or unreadable config
//! just yields defaults, and saving silently gives up rather than bothering
//! the user about their settings file.

use std::path::{Path, PathBuf};

const MAX_RECENT: usize = 8;

#[derive(Clone, PartialEq)]
pub struct Config {
    pub theme: usize,
    pub volume: f32,
    pub sidebar: bool,
    pub export_res: u32,
    pub export_fps: u32,
    /// The Custom theme's four anchor colors (background, body, hero,
    /// highlight) — without these a persisted `theme=<custom>` would come back
    /// as the fallback palette.
    pub custom: [[u8; 3]; 4],
    /// Most recent first.
    pub recent: Vec<PathBuf>,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            theme: 0,
            volume: 0.85,
            sidebar: true,
            export_res: 1080,
            export_fps: 60,
            // the Dusk Encom seeds, same as the color pickers' initial state
            custom: [[0x0b, 0x10, 0x14], [0x3f, 0x9a, 0xa0], [0xe0, 0x8a, 0x3c], [0xec, 0xe3, 0xcf]],
            recent: Vec::new(),
        }
    }
}

/// Path equality for the recents list — case-insensitive on Windows, where
/// `C:\Music\A.mp3` (drag-drop) and `c:\music\a.mp3` (dialog) are one file.
fn same_path(a: &Path, b: &Path) -> bool {
    if cfg!(windows) {
        a.as_os_str().to_string_lossy().to_lowercase() == b.as_os_str().to_string_lossy().to_lowercase()
    } else {
        a == b
    }
}

impl Config {
    /// Remember `p` as the most recent track (absolute, deduped, capped) — a
    /// relative `--file` path would otherwise stop resolving from a different
    /// working directory.
    pub fn touch_recent(&mut self, p: &Path) {
        let p = std::path::absolute(p).unwrap_or_else(|_| p.to_path_buf());
        self.recent.retain(|r| !same_path(r, &p));
        self.recent.insert(0, p);
        self.recent.truncate(MAX_RECENT);
    }

    fn to_text(&self) -> String {
        let mut s = String::new();
        s.push_str(&format!("theme={}\n", self.theme));
        s.push_str(&format!("volume={:.3}\n", self.volume));
        s.push_str(&format!("sidebar={}\n", self.sidebar as u8));
        s.push_str(&format!("export_res={}\n", self.export_res));
        s.push_str(&format!("export_fps={}\n", self.export_fps));
        let hex = |c: [u8; 3]| format!("{:02x}{:02x}{:02x}", c[0], c[1], c[2]);
        s.push_str(&format!(
            "custom={},{},{},{}\n",
            hex(self.custom[0]),
            hex(self.custom[1]),
            hex(self.custom[2]),
            hex(self.custom[3])
        ));
        for r in &self.recent {
            s.push_str(&format!("recent={}\n", r.display()));
        }
        s
    }

    fn from_text(text: &str) -> Self {
        let mut c = Config { recent: Vec::new(), ..Config::default() };
        for line in text.lines() {
            let Some((key, val)) = line.split_once('=') else { continue };
            match key.trim() {
                "theme" => c.theme = val.trim().parse().unwrap_or(c.theme),
                "volume" => {
                    // filter() drops NaN/Inf: NaN would stick in the audio
                    // engine AND defeat the debounce's != check (a save every
                    // second, forever).
                    c.volume = val
                        .trim()
                        .parse::<f32>()
                        .ok()
                        .filter(|v| v.is_finite())
                        .unwrap_or(c.volume)
                        .clamp(0.0, 1.0)
                }
                "sidebar" => c.sidebar = val.trim() != "0",
                "export_res" => {
                    let v = val.trim().parse().unwrap_or(c.export_res);
                    if [720, 1080, 1440, 2160].contains(&v) {
                        c.export_res = v;
                    }
                }
                "export_fps" => {
                    let v = val.trim().parse().unwrap_or(c.export_fps);
                    if [30, 60].contains(&v) {
                        c.export_fps = v;
                    }
                }
                "custom" => {
                    let mut a = c.custom;
                    let mut ok = 0;
                    for (i, part) in val.trim().split(',').take(4).enumerate() {
                        if let Ok(v) = u32::from_str_radix(part.trim(), 16) {
                            a[i] = [(v >> 16) as u8, (v >> 8) as u8, v as u8];
                            ok += 1;
                        }
                    }
                    if ok == 4 {
                        c.custom = a;
                    }
                }
                "recent" if c.recent.len() < MAX_RECENT => {
                    c.recent.push(PathBuf::from(val.trim()))
                }
                _ => {}
            }
        }
        c
    }
}

/// `%APPDATA%\cherry-visualizer\config.txt` on Windows, `$XDG_CONFIG_HOME` or
/// `~/.config` elsewhere.
fn config_path() -> Option<PathBuf> {
    let base = std::env::var_os("APPDATA")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("XDG_CONFIG_HOME").map(PathBuf::from))
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))?;
    Some(base.join("cherry-visualizer").join("config.txt"))
}

/// Read the config, pruning recent entries whose files no longer exist.
pub fn load() -> Config {
    let mut c = config_path()
        .and_then(|p| std::fs::read_to_string(p).ok())
        .map(|t| Config::from_text(&t))
        .unwrap_or_default();
    c.recent.retain(|p| p.exists());
    c
}

pub fn save(c: &Config) {
    let Some(path) = config_path() else { return };
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let _ = std::fs::write(path, c.to_text());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_round_trips() {
        let mut c = Config {
            theme: 4,
            volume: 0.62,
            sidebar: false,
            export_res: 1440,
            export_fps: 30,
            custom: [[1, 2, 3], [4, 5, 6], [7, 8, 9], [250, 251, 252]],
            recent: Vec::new(),
        };
        c.touch_recent(Path::new("C:/music/a song.mp3"));
        c.touch_recent(Path::new("C:/music/b.flac"));
        c.touch_recent(Path::new("C:/music/a song.mp3")); // dedupe + move to front
        let back = Config::from_text(&c.to_text());
        assert!(back == c);
        assert_eq!(back.recent.len(), 2);
        assert_eq!(back.recent[0], PathBuf::from("C:/music/a song.mp3"));
    }

    #[test]
    fn config_ignores_junk_and_clamps() {
        let c = Config::from_text(
            "theme=99\nvolume=7.5\nexport_res=123\nexport_fps=45\ncustom=zz,00,11\nwat\nx=y\n",
        );
        assert_eq!(c.theme, 99); // clamped later by style::set_theme
        assert!(c.volume <= 1.0);
        assert_eq!(c.export_res, 1080);
        assert_eq!(c.export_fps, 60);
        assert_eq!(c.custom, Config::default().custom); // partial custom= rejected
        // Non-finite volume is rejected outright — NaN would stick in the
        // audio engine and defeat the save-debounce's != check.
        assert_eq!(Config::from_text("volume=NaN\n").volume, 0.85);
    }

    #[test]
    fn recents_dedupe_case_insensitively_on_windows() {
        let mut c = Config::default();
        c.touch_recent(Path::new("C:/Music/Song.mp3"));
        c.touch_recent(Path::new("c:/music/song.mp3"));
        if cfg!(windows) {
            assert_eq!(c.recent.len(), 1);
        }
    }
}
