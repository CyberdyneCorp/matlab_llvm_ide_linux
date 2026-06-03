//! User preferences persisted to `~/.config/matforge/config.toml`: appearance
//! (theme / accent / font), toolchain paths, the layout snapshot, and recent /
//! open files for session restore. `#[serde(default)]` everywhere keeps old and
//! partial config files loading cleanly. Pure logic (serialize / merge / paths);
//! the GTK app reads/writes via [`Preferences::load`] / [`save`](Preferences::save).

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::theme::{Accent, ThemeId};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
#[serde(default)]
pub struct Preferences {
    pub appearance: Appearance,
    pub toolchain: Toolchain,
    pub layout: LayoutPrefs,
    /// Recently opened folders/files (most-recent first), for the Welcome view.
    pub recent: Vec<String>,
    /// Files open in the editor at last exit, for session restore.
    pub open_tabs: Vec<String>,
    /// The last opened project folder.
    pub last_folder: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(default)]
pub struct Appearance {
    /// `ThemeId` key (`midnight` / `daylight` / `high-contrast`).
    pub theme: String,
    /// `Accent` key (`amber` / `blue` / …).
    pub accent: String,
    pub font_scale: f64,
    pub code_font_scale: f64,
    pub code_font: String,
}

impl Default for Appearance {
    fn default() -> Self {
        Appearance {
            theme: ThemeId::Midnight.key().to_string(),
            accent: Accent::Amber.key().to_string(),
            font_scale: 1.0,
            code_font_scale: 1.0,
            code_font: "JetBrains Mono".to_string(),
        }
    }
}

impl Appearance {
    pub fn theme_id(&self) -> ThemeId {
        ThemeId::from_key(&self.theme)
    }
    pub fn accent_enum(&self) -> Accent {
        Accent::from_key(&self.accent)
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
#[serde(default)]
pub struct Toolchain {
    pub matlabc_path: Option<String>,
    pub runtime_archive: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(default)]
pub struct LayoutPrefs {
    pub sidebar_visible: bool,
    pub workspace_visible: bool,
    pub plots_visible: bool,
    pub sidebar_width: i32,
    pub right_width: i32,
}

impl Default for LayoutPrefs {
    fn default() -> Self {
        LayoutPrefs {
            sidebar_visible: true,
            workspace_visible: true,
            plots_visible: true,
            sidebar_width: 220,
            right_width: 620,
        }
    }
}

impl Preferences {
    /// `~/.config/matforge/config.toml`, honoring `$XDG_CONFIG_HOME`.
    pub fn config_path() -> Option<PathBuf> {
        let base = std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))?;
        Some(base.join("matforge").join("config.toml"))
    }

    /// Load preferences from the default path, falling back to defaults when the
    /// file is missing or unparseable (never fails — preferences must not block
    /// startup).
    pub fn load() -> Preferences {
        match Self::config_path() {
            Some(path) => Self::load_from(&path),
            None => Preferences::default(),
        }
    }

    pub fn load_from(path: &std::path::Path) -> Preferences {
        std::fs::read_to_string(path)
            .ok()
            .and_then(|text| Self::parse(&text).ok())
            .unwrap_or_default()
    }

    /// Parse a TOML string; partial/old files fill missing fields with defaults.
    pub fn parse(text: &str) -> Result<Preferences, toml::de::Error> {
        toml::from_str(text)
    }

    pub fn to_toml(&self) -> String {
        toml::to_string_pretty(self).unwrap_or_default()
    }

    /// Write to the default path, creating the directory as needed.
    pub fn save(&self) -> std::io::Result<()> {
        let Some(path) = Self::config_path() else {
            return Ok(());
        };
        self.save_to(&path)
    }

    pub fn save_to(&self, path: &std::path::Path) -> std::io::Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        std::fs::write(path, self.to_toml())
    }

    /// Record `entry` as the most-recent item (deduped, capped at 12).
    pub fn push_recent(&mut self, entry: impl Into<String>) {
        let entry = entry.into();
        self.recent.retain(|e| e != &entry);
        self.recent.insert(0, entry);
        self.recent.truncate(12);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_midnight_amber() {
        let p = Preferences::default();
        assert_eq!(p.appearance.theme_id(), ThemeId::Midnight);
        assert_eq!(p.appearance.accent_enum(), Accent::Amber);
        assert_eq!(p.appearance.font_scale, 1.0);
        assert!(p.layout.sidebar_visible);
    }

    #[test]
    fn toml_round_trips() {
        let mut p = Preferences::default();
        p.appearance.theme = "daylight".into();
        p.appearance.accent = "blue".into();
        p.appearance.font_scale = 1.25;
        p.layout.sidebar_width = 260;
        p.push_recent("/proj/a");
        p.push_recent("/proj/b");
        let back = Preferences::parse(&p.to_toml()).unwrap();
        assert_eq!(back, p);
        assert_eq!(back.appearance.theme_id(), ThemeId::Daylight);
        assert_eq!(back.recent, vec!["/proj/b", "/proj/a"]);
    }

    #[test]
    fn partial_file_fills_defaults() {
        // Only the theme is set; everything else must default.
        let p = Preferences::parse("[appearance]\ntheme = \"high-contrast\"\n").unwrap();
        assert_eq!(p.appearance.theme_id(), ThemeId::HighContrast);
        assert_eq!(p.appearance.accent_enum(), Accent::Amber);
        assert_eq!(p.appearance.font_scale, 1.0);
        assert!(p.layout.plots_visible);
    }

    #[test]
    fn garbage_or_missing_falls_back_to_default() {
        assert_eq!(Preferences::load_from(std::path::Path::new("/no/such/file")), Preferences::default());
    }

    #[test]
    fn push_recent_dedupes_and_caps() {
        let mut p = Preferences::default();
        for i in 0..20 {
            p.push_recent(format!("/p/{i}"));
        }
        p.push_recent("/p/19"); // already most-recent
        assert_eq!(p.recent.len(), 12);
        assert_eq!(p.recent[0], "/p/19");
    }

    #[test]
    fn save_and_load_from_disk_round_trip() {
        let dir = std::env::temp_dir().join(format!("mf_prefs_test_{}", std::process::id()));
        let path = dir.join("config.toml");
        let mut p = Preferences::default();
        p.appearance.accent = "violet".into();
        p.save_to(&path).unwrap();
        let back = Preferences::load_from(&path);
        assert_eq!(back.appearance.accent_enum(), Accent::Violet);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
