use global_hotkey::hotkey::HotKey;
use std::path::PathBuf;

#[derive(Clone, Debug)]
pub struct Config {
    pub hotkey: String,
    pub history_size: usize,
    pub max_clip_mb: usize,
    pub paste_delay_ms: u64,
}
impl Default for Config {
    fn default() -> Self {
        Self {
            hotkey: "Ctrl+Shift+V".into(),
            history_size: 1000,
            max_clip_mb: 16,
            paste_delay_ms: 80,
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
        config
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
#[cfg(test)]
mod tests {
    use super::*;
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
