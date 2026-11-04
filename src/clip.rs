use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Clone, Debug, PartialEq)]
pub struct Format {
    pub name: String,
    pub data: Vec<u8>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Clip {
    pub copies: i64,
    pub pastes: i64,
    pub plain_pastes: i64,
    pub file_kinds: String,
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
            pastes: 0,
            plain_pastes: 0,
            file_kinds: String::new(),
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
    pub fn paths(&self) -> Vec<&str> {
        if self.kind == "files" {
            self.text.lines().collect()
        } else {
            vec![]
        }
    }
    pub fn file_kinds(&self) -> Vec<crate::files::Kind> {
        crate::files::kinds(&self.paths(), &self.file_kinds)
    }
    /// The row title and, for several files, their names as a quieter suffix.
    pub fn title(&self) -> (String, Option<String>) {
        match self.kind.as_str() {
            "image" if !self.screenshot_title.is_empty() => {
                (format!("Screenshot of {}", self.screenshot_title), None)
            }
            "image" => ("Image".into(), None),
            "files" => crate::files::title(&self.paths()),
            _ => (
                self.text
                    .lines()
                    .map(str::trim)
                    .find(|l| !l.is_empty())
                    .unwrap_or("")
                    .chars()
                    .take(180)
                    .collect(),
                None,
            ),
        }
    }
    pub fn age(&self) -> String {
        let seconds = now().saturating_sub(self.updated);
        if seconds < 60 {
            "just now".into()
        } else if seconds < 3600 {
            format!("{} min ago", seconds / 60)
        } else if seconds < 86400 {
            format!("{} h ago", seconds / 3600)
        } else {
            format!("{} d ago", seconds / 86400)
        }
    }
    /// The compact row's right-aligned time: "now", "2m", "3h", "4d".
    pub fn short_age(&self) -> String {
        let seconds = now().saturating_sub(self.updated);
        match seconds {
            ..60 => "now".into(),
            60..3600 => format!("{}m", seconds / 60),
            3600..86400 => format!("{}h", seconds / 3600),
            _ => format!("{}d", seconds / 86400),
        }
    }
    /// "1440×900 · Snipping Tool · 9 min ago", "3 images · 1 PDF · Orca · 2 min ago".
    pub fn meta(&self) -> String {
        let prefix = match self.kind.as_str() {
            "image" => format!("{} · ", self.text),
            "files" => format!("{} · ", crate::files::breakdown(&self.file_kinds())),
            _ => String::new(),
        };
        let copied = if self.copies > 1 {
            format!(" · copied {}×", self.copies)
        } else {
            String::new()
        };
        format!("{prefix}{} · {}{copied}", self.source, self.age())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn titles_and_meta_by_kind() {
        let mut c = Clip::text("\n  first line\nsecond".into(), "Notepad".into());
        assert_eq!(c.title(), ("first line".into(), None));
        assert_eq!(c.meta(), "Notepad · just now");
        c.copies = 3;
        assert_eq!(c.meta(), "Notepad · just now · copied 3×");
        c.kind = "image".into();
        c.text = "1440×900".into();
        c.copies = 1;
        assert_eq!(c.title().0, "Image");
        assert_eq!(c.meta(), "1440×900 · Notepad · just now");
        c.screenshot_title = "Ditto".into();
        assert_eq!(c.title().0, "Screenshot of Ditto");
        c.kind = "files".into();
        c.text = ["ui.png", "notes.pdf", "demo.mp4", "a.jpg", "b.png"]
            .map(|n| format!("C:/p/{n}"))
            .join("\n");
        c.source = "Orca".into();
        c.updated -= 150;
        assert_eq!(
            c.title(),
            (
                "5 files".into(),
                Some("ui.png, notes.pdf, demo.mp4 +2".into())
            )
        );
        assert_eq!(c.meta(), "3 images · 1 video · 1 PDF · Orca · 2 min ago");
        assert_eq!(c.short_age(), "2m");
        c.updated -= 7200;
        assert_eq!(c.short_age(), "2h");
    }
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
