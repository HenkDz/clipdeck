use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Clone, Debug, PartialEq)]
pub struct Format {
    pub name: String,
    pub data: Vec<u8>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Clip {
    pub copies: i64,
    pub file_path: String,
    pub screenshot_title: String,
    pub source_exe: String,
    pub sequence: u32,
    pub id: i64,
    pub kind: String,
    pub text: String,
    pub payload: Vec<u8>,
    pub thumbnail: Vec<u8>,
    pub formats: Vec<Format>,
    pub source: String,
    pub created: i64,
    pub updated: i64,
    pub pinned: bool,
    pub file_count: usize,
}

pub fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}

pub fn detect(text: &str) -> &'static str {
    let trimmed = text.trim();
    if !trimmed.chars().any(char::is_whitespace)
        && ["http://", "https://"]
            .iter()
            .any(|s| trimmed.to_ascii_lowercase().starts_with(s) && trimmed.len() > s.len())
    {
        "link"
    } else if !trimmed.chars().any(char::is_whitespace)
        && trimmed.split_once('@').is_some_and(|(local, host)| {
            !local.is_empty()
                && host.contains('.')
                && !host.contains('@')
                && !host.starts_with('.')
                && !host.ends_with('.')
        })
    {
        "email"
    } else {
        "text"
    }
}

impl Clip {
    pub fn text(text: String, source: String) -> Self {
        Self {
            copies: 1,
            file_path: String::new(),
            screenshot_title: String::new(),
            source_exe: String::new(),
            sequence: 0,
            id: 0,
            kind: detect(&text).into(),
            payload: text.as_bytes().to_vec(),
            text,
            thumbnail: vec![],
            formats: vec![],
            source,
            created: now(),
            updated: now(),
            pinned: false,
            file_count: 0,
        }
    }
    pub fn is_text(&self) -> bool {
        matches!(self.kind.as_str(), "text" | "link" | "email")
    }
    pub fn section(&self) -> &'static str {
        if self.pinned {
            "Pinned"
        } else if crate::platform::is_today(self.updated) {
            "Today"
        } else {
            "Earlier"
        }
    }
    pub fn meta(&self) -> String {
        let seconds = now().saturating_sub(self.updated);
        let age = if seconds < 60 {
            "just now".into()
        } else if seconds < 3600 {
            format!("{} min ago", seconds / 60)
        } else if seconds < 86400 {
            format!("{} h ago", seconds / 3600)
        } else {
            format!("{} d ago", seconds / 86400)
        };
        let prefix = match self.kind.as_str() {
            "image" => format!("{} · ", self.text),
            "files" => format!(
                "{} {} · ",
                self.file_count,
                if self.file_count == 1 {
                    "file"
                } else {
                    "files"
                }
            ),
            _ => String::new(),
        };
        let copied = if self.copies > 1 {
            format!(" · copied {}×", self.copies)
        } else {
            String::new()
        };
        let title = if self.screenshot_title.is_empty() {
            String::new()
        } else {
            format!("Screenshot of {} · ", self.screenshot_title)
        };
        format!("{title}{prefix}{} · {age}{copied}", self.source)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn detects_types() {
        for (s, t) in [
            ("https://example.com/a", "link"),
            ("HTTP://example.com", "link"),
            ("x@y.org", "email"),
            ("hello x@y.org", "text"),
            ("https://", "text"),
            ("x@@y.org", "text"),
            ("", "text"),
        ] {
            assert_eq!(detect(s), t);
        }
    }
}
