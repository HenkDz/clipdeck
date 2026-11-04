use global_hotkey::hotkey::HotKey;
use std::path::PathBuf;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Density {
    Comfortable,
    Compact,
}
impl Density {
    pub fn name(self) -> &'static str {
        match self {
            Self::Comfortable => "comfortable",
            Self::Compact => "compact",
        }
    }
    pub fn toggled(self) -> Self {
        match self {
            Self::Comfortable => Self::Compact,
            Self::Compact => Self::Comfortable,
        }
    }
}

pub const MIN_SIZE: (u32, u32) = (360, 320);

#[derive(Clone, Debug)]
pub struct Config {
    pub hotkey: String,
    pub history_size: usize,
    pub max_clip_mb: usize,
    pub paste_delay_ms: u64,
    pub density: Density,
    /// Popup size in logical pixels; clamped to the monitor's work area when shown.
    pub width: u32,
    pub height: u32,
    pub keep_open: bool,
    pub check_updates: bool,
    /// Last top-left corner in screen pixels, used while the popup is kept open.
    pub position: Option<(i32, i32)>,
}
impl Default for Config {
    fn default() -> Self {
        Self {
            hotkey: "Ctrl+Shift+V".into(),
            history_size: 1000,
            max_clip_mb: 16,
            paste_delay_ms: 80,
            density: Density::Comfortable,
            width: 480,
            height: 520,
            keep_open: false,
            check_updates: false,
            position: None,
        }
    }
}
pub fn directory() -> std::io::Result<PathBuf> {
    std::env::var_os("APPDATA")
        .filter(|s| !s.is_empty())
        .map(PathBuf::from)
        .or_else(dirs::config_dir)
        .map(|p| p.join("Clipdeck"))
        .ok_or_else(|| std::io::Error::other("APPDATA is unavailable"))
}
impl Config {
    pub fn parse(text: &str) -> Self {
        let mut config = Self::default();
        let Ok(value) = text.parse::<toml::Table>() else {
            eprintln!("Invalid config.toml; using defaults");
            return config;
        };
        if let Some(v) = value.get("hotkey") {
            if let Some(s) = v.as_str().filter(|s| s.parse::<HotKey>().is_ok()) {
                config.hotkey = s.into();
            } else {
                eprintln!("Invalid hotkey; using default");
            }
        }
        if let Some(v) = value.get("history_size") {
            if let Some(n) = v.as_integer().filter(|n| (1..=100_000).contains(n)) {
                config.history_size = n as usize;
            } else {
                eprintln!("Invalid history_size; using default");
            }
        }
        if let Some(v) = value.get("max_clip_mb") {
            if let Some(n) = v.as_integer().filter(|n| (1..=128).contains(n)) {
                config.max_clip_mb = n as usize;
            } else {
                eprintln!("Invalid max_clip_mb; using default");
            }
        }
        if let Some(v) = value.get("paste_delay_ms") {
            if let Some(n) = v.as_integer().filter(|n| (0..=5000).contains(n)) {
                config.paste_delay_ms = n as u64;
            } else {
                eprintln!("Invalid paste_delay_ms; using default");
            }
        }
        if let Some(v) = value.get("density") {
            match v.as_str() {
                Some("comfortable") => config.density = Density::Comfortable,
                Some("compact") => config.density = Density::Compact,
                _ => eprintln!("Invalid density; using default"),
            }
        }
        for (key, min, field) in [
            ("width", MIN_SIZE.0, &mut config.width),
            ("height", MIN_SIZE.1, &mut config.height),
        ] {
            if let Some(v) = value.get(key) {
                if let Some(n) = v.as_integer().filter(|n| (min as i64..=8000).contains(n)) {
                    *field = n as u32;
                } else {
                    eprintln!("Invalid {key}; using default");
                }
            }
        }
        if let Some(v) = value.get("keep_open") {
            if let Some(b) = v.as_bool() {
                config.keep_open = b;
            } else {
                eprintln!("Invalid keep_open; using default");
            }
        }
        let coordinate = |key| {
            value
                .get(key)
                .and_then(|v| v.as_integer())
                .filter(|n| (-100_000..=100_000).contains(n))
                .map(|n| n as i32)
        };
        if let Some(value) = value.get("check_updates").and_then(toml::Value::as_bool) {
            config.check_updates = value;
        }
        config.position = coordinate("x").zip(coordinate("y"));
        config
    }
    /// Rewrites the given top-level keys in config.toml, keeping comments and other settings.
    pub fn save(keys: &[(&str, String)]) -> std::io::Result<()> {
        let path = directory()?.join("config.toml");
        let mut text = std::fs::read_to_string(&path).unwrap_or_default();
        for (key, value) in keys {
            text = set_key(&text, key, value);
        }
        let temp = path.with_extension("toml.tmp");
        std::fs::write(&temp, text)?;
        std::fs::rename(temp, path)
    }
    pub fn load() -> std::io::Result<Self> {
        let dir = directory()?;
        std::fs::create_dir_all(&dir)?;
        let path = dir.join("config.toml");
        if !path.exists() {
            std::fs::write(
                &path,
                "hotkey = \"Ctrl+Shift+V\"\nhistory_size = 1000\nmax_clip_mb = 16\npaste_delay_ms = 80\n",
            )?;
        }
        match std::fs::read_to_string(path) {
            Ok(text) => Ok(Self::parse(&text)),
            Err(error) => {
                eprintln!("Read config: {error}; using defaults");
                Ok(Self::default())
            }
        }
    }
}
/// Replaces `key = …` among the top-level keys, or adds it before the first table.
pub fn set_key(text: &str, key: &str, value: &str) -> String {
    let line = format!("{key} = {value}");
    let mut lines: Vec<String> = text.lines().map(str::to_owned).collect();
    let table = lines
        .iter()
        .position(|l| l.trim_start().starts_with('['))
        .unwrap_or(lines.len());
    let existing = lines[..table].iter().position(|l| {
        l.trim_start()
            .strip_prefix(key)
            .is_some_and(|rest| rest.trim_start().starts_with('='))
    });
    match existing {
        Some(ix) => lines[ix] = line,
        None => {
            // Keep a blank line before a following table header.
            let at = if table < lines.len() && table > 0 && lines[table - 1].trim().is_empty() {
                table - 1
            } else {
                table
            };
            lines.insert(at, line);
        }
    }
    let mut out = lines.join("\n");
    out.push('\n');
    out
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn view_settings_round_trip_through_set_key() {
        let original = "# my settings\nhotkey = \"Ctrl+Alt+V\" # keep\ndensity_note = 1\nhistory_size = 50\n\n[extra]\nwidth = 9";
        let mut text = original.to_owned();
        for (key, value) in [
            ("density", "\"compact\"".to_owned()),
            ("width", "640".into()),
            ("height", "700".into()),
            ("keep_open", "true".into()),
            ("x", "-1200".into()),
            ("y", "40".into()),
        ] {
            text = set_key(&text, key, &value);
        }
        text = set_key(&text, "width", "600");
        let c = Config::parse(&text);
        assert_eq!(c.density, Density::Compact);
        assert_eq!((c.width, c.height, c.keep_open), (600, 700, true));
        assert_eq!(c.position, Some((-1200, 40)));
        assert_eq!((c.hotkey.as_str(), c.history_size), ("Ctrl+Alt+V", 50));
        assert!(
            text.starts_with("# my settings\nhotkey = \"Ctrl+Alt+V\" # keep\ndensity_note = 1\n")
        );
        assert!(text.ends_with("\n\n[extra]\nwidth = 9\n"));
        assert_eq!(text.matches("width = 600").count(), 1);
        let c = Config::parse(&set_key(&text, "density", "\"comfortable\""));
        assert_eq!(c.density, Density::Comfortable);
        assert_eq!(set_key("", "keep_open", "false"), "keep_open = false\n");
    }
    #[test]
    fn invalid_view_settings_fall_back() {
        let c =
            Config::parse("density = 'tiny'\nwidth = 100\nheight = 'tall'\nkeep_open = 1\nx = 5");
        let d = Config::default();
        assert_eq!(c.density, d.density);
        assert_eq!((c.width, c.height, c.keep_open), (d.width, d.height, false));
        assert_eq!(c.position, None);
        assert_eq!(Config::parse("width = 360\nheight = 320").height, 320);
    }
    #[test]
    fn fallback_per_field() {
        assert_eq!(Config::parse("max_clip_mb=2").max_clip_mb, 2);
        for value in ["0", "-1", "129", "'oops'"] {
            assert_eq!(
                Config::parse(&format!("max_clip_mb={value}")).max_clip_mb,
                16
            );
        }
        let c = Config::parse("hotkey = 'invalid'\nhistory_size = -4\npaste_delay_ms = 125");
        assert_eq!(c.hotkey, Config::default().hotkey);
        assert_eq!(c.history_size, 1000);
        assert_eq!(c.paste_delay_ms, 125);
        assert_eq!(Config::parse("[").history_size, 1000);
        assert_eq!(
            Config::parse("history_size = 'oops'\npaste_delay_ms = 99999").paste_delay_ms,
            80
        );
        assert_eq!(Config::parse("hotkey = 'Ctrl+Alt+C'").hotkey, "Ctrl+Alt+C");
    }
}
