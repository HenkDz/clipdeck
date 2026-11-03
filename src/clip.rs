use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Clone, Debug, PartialEq)]
pub struct Format {
    pub name: String,
    pub data: Vec<u8>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Clip {
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
            "files" => format!("{} files · ", self.text.lines().count()),
            _ => String::new(),
        };
        format!("{prefix}{} · {age}", self.source)
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
