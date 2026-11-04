//! File-list clips: per-file kinds, the type breakdown, titles and clipboard routing.
//! Pure logic only; Shell thumbnails and filesystem checks live in `platform`.
use std::path::Path;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Kind {
    Image,
    Video,
    Audio,
    Pdf,
    Document,
    Spreadsheet,
    Slides,
    Archive,
    Code,
    Folder,
    Other,
}

impl Kind {
    const ALL: [(Kind, char); 11] = [
        (Kind::Image, 'i'),
        (Kind::Video, 'v'),
        (Kind::Audio, 'a'),
        (Kind::Pdf, 'p'),
        (Kind::Document, 'd'),
        (Kind::Spreadsheet, 's'),
        (Kind::Slides, 'k'),
        (Kind::Archive, 'z'),
        (Kind::Code, 'c'),
        (Kind::Folder, 'f'),
        (Kind::Other, 'o'),
    ];
    pub fn code(self) -> char {
        Self::ALL.iter().find(|(k, _)| *k == self).unwrap().1
    }
    fn from_code(code: char) -> Option<Self> {
        Self::ALL.iter().find(|(_, c)| *c == code).map(|(k, _)| *k)
    }
    /// Classifies by extension only; folders need `is_dir` from the caller.
    pub fn of(path: &str, is_dir: bool) -> Self {
        if is_dir {
            return Self::Folder;
        }
        let ext = Path::new(path)
            .extension()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        match ext.as_str() {
            "png" | "jpg" | "jpeg" | "bmp" | "gif" | "webp" | "tif" | "tiff" | "heic" | "avif"
            | "ico" | "svg" => Self::Image,
            "mp4" | "mov" | "mkv" | "webm" | "avi" | "wmv" | "m4v" => Self::Video,
            "mp3" | "wav" | "flac" | "m4a" | "aac" | "ogg" | "opus" | "wma" => Self::Audio,
            "pdf" => Self::Pdf,
            "doc" | "docx" | "odt" | "rtf" | "txt" | "md" | "pages" => Self::Document,
            "xls" | "xlsx" | "ods" | "csv" | "tsv" | "numbers" => Self::Spreadsheet,
            "ppt" | "pptx" | "odp" | "key" => Self::Slides,
            "zip" | "7z" | "rar" | "tar" | "gz" | "tgz" | "bz2" | "xz" | "cab" | "iso" => {
                Self::Archive
            }
            "rs" | "py" | "js" | "ts" | "tsx" | "jsx" | "c" | "h" | "cpp" | "hpp" | "cs" | "go"
            | "java" | "kt" | "swift" | "rb" | "php" | "sh" | "ps1" | "bat" | "cmd" | "json"
            | "toml" | "yaml" | "yml" | "xml" | "html" | "css" | "sql" | "lua" => Self::Code,
            _ => Self::Other,
        }
    }
    /// Thumbnails are worth asking the Shell for; everything else shows a type icon.
    pub fn is_media(self) -> bool {
        matches!(self, Self::Image | Self::Video)
    }
    fn noun(self, count: usize) -> &'static str {
        let (one, many) = match self {
            Self::Image => ("image", "images"),
            Self::Video => ("video", "videos"),
            Self::Audio => ("audio file", "audio files"),
            Self::Pdf => ("PDF", "PDFs"),
            Self::Document => ("document", "documents"),
            Self::Spreadsheet => ("spreadsheet", "spreadsheets"),
            Self::Slides => ("presentation", "presentations"),
            Self::Archive => ("archive", "archives"),
            Self::Code => ("code file", "code files"),
            Self::Folder => ("folder", "folders"),
            Self::Other => ("file", "files"),
        };
        if count == 1 { one } else { many }
    }
}

/// Stored per-file kind codes, or extension guesses for rows captured before they were recorded.
pub fn kinds(paths: &[&str], stored: &str) -> Vec<Kind> {
    if stored.chars().count() == paths.len() {
        stored
            .chars()
            .map(|c| Kind::from_code(c).unwrap_or(Kind::Other))
            .collect()
    } else {
        paths.iter().map(|p| Kind::of(p, false)).collect()
    }
}

pub fn codes(kinds: &[Kind]) -> String {
    kinds.iter().map(|k| k.code()).collect()
}

/// "3 images · 1 video · 1 PDF"; at most three groups, the rest folded into "N more".
pub fn breakdown(kinds: &[Kind]) -> String {
    if let [kind] = kinds {
        let noun = kind.noun(1);
        let mut chars = noun.chars();
        return chars
            .next()
            .map(|c| c.to_uppercase().chain(chars).collect())
            .unwrap_or_default();
    }
    let mut groups: Vec<(Kind, usize)> = vec![];
    for kind in kinds {
        match groups.iter_mut().find(|(k, _)| k == kind) {
            Some((_, n)) => *n += 1,
            None => groups.push((*kind, 1)),
        }
    }
    groups.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    let shown = if groups.len() > 3 { 2 } else { groups.len() };
    let mut parts: Vec<String> = groups[..shown]
        .iter()
        .map(|(k, n)| format!("{n} {}", k.noun(*n)))
        .collect();
    let rest: usize = groups[shown..].iter().map(|(_, n)| n).sum();
    if rest > 0 {
        parts.push(format!("{rest} more"));
    }
    parts.join(" · ")
}

pub fn name(path: &str) -> &str {
    let trimmed = path.trim_end_matches(['\\', '/']);
    match trimmed.rfind(['\\', '/']) {
        Some(ix) if ix + 1 < trimmed.len() => &trimmed[ix + 1..],
        _ => path,
    }
}

/// One file: its name. Several: ("5 files", "ui.png, notes.pdf, demo.mp4 +2").
pub fn title(paths: &[&str]) -> (String, Option<String>) {
    match paths {
        [] => ("No files".into(), None),
        [one] => (name(one).into(), None),
        _ => {
            let mut names = paths
                .iter()
                .take(3)
                .map(|p| name(p))
                .collect::<Vec<_>>()
                .join(", ");
            if paths.len() > 3 {
                names.push_str(&format!(" +{}", paths.len() - 3));
            }
            (format!("{} files", paths.len()), Some(names))
        }
    }
}

#[derive(Debug, PartialEq)]
pub enum Route {
    Files,
    Image,
    Text,
    Nothing,
}

/// CF_HDROP wins unless it names exactly one image file and a bitmap came with it.
/// Explorer-style sources add a bitmap of one file to multi-file copies.
pub fn route(files: Option<&[String]>, has_image: bool, has_text: bool) -> Route {
    match files {
        Some([one]) if has_image && Kind::of(one, false) == Kind::Image => Route::Image,
        Some(files) if !files.is_empty() => Route::Files,
        _ if has_image => Route::Image,
        _ if has_text => Route::Text,
        _ => Route::Nothing,
    }
}

/// A CF_HDROP payload: DROPFILES header, wide paths, double NUL terminator.
pub fn hdrop(paths: &[&str]) -> Vec<u8> {
    let mut data = Vec::with_capacity(22 + paths.iter().map(|p| p.len() * 2 + 2).sum::<usize>());
    data.extend(20u32.to_le_bytes()); // pFiles
    data.extend([0u8; 8]); // pt
    data.extend(0u32.to_le_bytes()); // fNC
    data.extend(1u32.to_le_bytes()); // fWide
    for path in paths {
        data.extend(
            path.encode_utf16()
                .chain(Some(0))
                .flat_map(u16::to_le_bytes),
        );
    }
    data.extend([0, 0]);
    data
}

#[cfg(test)]
mod tests {
    use super::*;
    fn s(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }
    #[test]
    fn routes_drop_lists_ahead_of_bitmaps() {
        let five = s(&["a\\ui.png", "b.pdf", "c.mp4", "d.jpg", "e.rs"]);
        assert_eq!(route(Some(&five), true, false), Route::Files);
        assert_eq!(route(Some(&five), false, false), Route::Files);
        let pdf = s(&["C:\\x\\notes.pdf"]);
        assert_eq!(route(Some(&pdf), true, false), Route::Files);
        let png = s(&["C:\\x\\shot.PNG"]);
        assert_eq!(route(Some(&png), true, false), Route::Image);
        assert_eq!(route(Some(&png), false, true), Route::Files);
        let images = s(&["a.png", "b.png"]);
        assert_eq!(route(Some(&images), true, false), Route::Files);
        assert_eq!(route(Some(&[]), true, true), Route::Image);
        assert_eq!(route(None, true, true), Route::Image);
        assert_eq!(route(None, false, true), Route::Text);
        assert_eq!(route(None, false, false), Route::Nothing);
    }
    #[test]
    fn breakdown_groups_by_count_then_kind() {
        let k = |p: &[&str]| p.iter().map(|p| Kind::of(p, false)).collect::<Vec<_>>();
        assert_eq!(
            breakdown(&k(&["a.png", "b.mp4", "c.jpg", "d.pdf", "e.webp"])),
            "3 images · 1 video · 1 PDF"
        );
        assert_eq!(breakdown(&k(&["notes.pdf"])), "PDF");
        assert_eq!(breakdown(&[Kind::Folder]), "Folder");
        assert_eq!(breakdown(&k(&["a.rs", "b.zip"])), "1 archive · 1 code file");
        assert_eq!(
            breakdown(&k(&["a.png", "b.png", "c.mp4", "d.pdf", "e.zip", "f.bin"])),
            "2 images · 1 video · 3 more"
        );
        assert_eq!(breakdown(&k(&["x", "y"])), "2 files");
    }
    #[test]
    fn stored_codes_win_and_fall_back_to_extensions() {
        let paths = ["C:\\a", "C:\\b.png"];
        assert_eq!(kinds(&paths, "fi"), [Kind::Folder, Kind::Image]);
        assert_eq!(kinds(&paths, ""), [Kind::Other, Kind::Image]);
        assert_eq!(
            kinds(&paths, &codes(&[Kind::Archive, Kind::Code])),
            [Kind::Archive, Kind::Code]
        );
        for (kind, _) in Kind::ALL {
            assert_eq!(Kind::from_code(kind.code()), Some(kind));
        }
    }
    #[test]
    fn titles_name_the_first_files() {
        let five = [
            "C:\\p\\ui.png",
            "C:\\p\\notes.pdf",
            "C:\\p\\demo.mp4",
            "C:\\p\\d.jpg",
            "C:\\p\\e.rs",
        ];
        assert_eq!(
            title(&five),
            (
                "5 files".into(),
                Some("ui.png, notes.pdf, demo.mp4 +2".into())
            )
        );
        assert_eq!(title(&five[..1]), ("ui.png".into(), None));
        assert_eq!(
            title(&five[..2]),
            ("2 files".into(), Some("ui.png, notes.pdf".into()))
        );
        assert_eq!(name("C:\\dir\\assets\\"), "assets");
        assert_eq!(name("D:\\"), "D:\\");
    }
    #[test]
    fn hdrop_layout() {
        let data = hdrop(&["C:\\a", "é"]);
        assert_eq!(&data[..4], &20u32.to_le_bytes());
        assert_eq!(&data[16..20], &1u32.to_le_bytes());
        let wide: Vec<u16> = data[20..]
            .chunks(2)
            .map(|c| u16::from_le_bytes([c[0], c[1]]))
            .collect();
        assert_eq!(wide, [67, 58, 92, 97, 0, 0xe9, 0, 0]);
    }
}
