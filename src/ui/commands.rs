use super::{
    Popup,
    popup::{Gallery, GalleryFile, Status, View},
};
use crate::{
    files::{self, Kind},
    platform,
};
use gpui::*;
use gpui_component::input::TextareaState;
use std::{path::Path, sync::Arc};

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) enum Command {
    Preview,
    OpenExternal,
    OpenLocation,
    Paste(bool),
    Pin,
    Delete,
    Edit,
    Copy,
    Details,
    Transform(&'static str),
    Open,
    Save,
    /// One file of a file clip, by index.
    PasteFile(usize),
    /// Only the image files of a mixed file clip.
    PasteImages,
    /// One file of a file clip onto the clipboard, without pasting.
    CopyFile(usize),
    /// One file's full path (false) or name (true), pasted as plain text.
    FileText(usize, bool),
}

pub(super) const TEXT_TRANSFORMS: [&str; 4] = ["UPPERCASE", "lowercase", "Trim spaces", "One line"];
pub(super) const FILE_TRANSFORMS: [&str; 2] = ["Paths as text", "File names only"];

fn png(bytes: Vec<u8>) -> Arc<Image> {
    Arc::new(Image::from_bytes(ImageFormat::Png, bytes))
}

pub(super) fn size_label(bytes: u64) -> String {
    if bytes < 1024 {
        format!("{bytes} bytes")
    } else if bytes < 1_048_576 {
        format!("{} KB", bytes.div_ceil(1024))
    } else {
        format!("{:.1} MB", bytes as f64 / 1_048_576.)
    }
}

impl Popup {
    pub(super) fn selected_command(
        &mut self,
        cmd: Command,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(clip) = self.clips.get(self.selected) {
            self.command(clip.id, cmd, window, cx);
        }
    }
    pub(super) fn command(
        &mut self,
        id: i64,
        cmd: Command,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(mut clip) = self.clips.iter().find(|c| c.id == id).cloned() else {
            return;
        };
        self.status = None;
        let result: Result<(), String> = (|| {
            match cmd {
                Command::Preview => {
                    self.focus.focus(window, cx);
                    match clip.kind.as_str() {
                        "files" => self.open_gallery(&clip, cx),
                        "image" => {
                            self.view = View::Image {
                                id,
                                image: None,
                                caption: format!("{} · {}", clip.text, clip.source),
                            };
                            let task = cx.background_spawn(async move {
                                platform::locations::load_clip(id).map(|c| c.payload)
                            });
                            cx.spawn(async move |this, cx| {
                                let result = task.await;
                                let _ = this.update(cx, |this, cx| {
                                    if let View::Image {
                                        id: shown,
                                        image,
                                        caption,
                                    } = &mut this.view
                                        && *shown == id
                                    {
                                        match result {
                                            Ok(bytes) => {
                                                *caption = format!(
                                                    "{} · PNG · {} · {} · {}",
                                                    clip.text,
                                                    size_label(bytes.len() as u64),
                                                    clip.source,
                                                    clip.age()
                                                );
                                                *image = Some(png(bytes));
                                            }
                                            Err(error) => this.status = Some(Status::Error(error)),
                                        }
                                        cx.notify();
                                    }
                                });
                            })
                            .detach();
                        }
                        _ => {
                            self.view = View::Text { id, text: None };
                            let task = cx.background_spawn(async move {
                                platform::locations::load_clip(id).map(|c| c.text)
                            });
                            cx.spawn(async move |this, cx| {
                                let result = task.await;
                                let _ = this.update(cx, |this, cx| {
                                    if let View::Text { id: shown, text } = &mut this.view
                                        && *shown == id
                                    {
                                        match result {
                                            Ok(full) => *text = Some(full),
                                            Err(error) => this.status = Some(Status::Error(error)),
                                        }
                                        cx.notify();
                                    }
                                });
                            })
                            .detach();
                        }
                    }
                }
                Command::OpenExternal | Command::OpenLocation => {
                    let location = cmd == Command::OpenLocation;
                    self.status = Some(Status::Info(
                        if location {
                            "Opening file location…"
                        } else {
                            "Opening in the default app…"
                        }
                        .into(),
                    ));
                    let task =
                        cx.background_spawn(async move { platform::preview::load(id, location) });
                    cx.spawn(async move |this, cx| {
                        let result = task.await;
                        let _ = this.update(cx, |this, cx| {
                            this.status = result.err().map(Status::Error);
                            this.reload(cx);
                        });
                    })
                    .detach();
                }
                Command::Pin => self.store.borrow_mut().pin(id).map_err(|e| e.to_string())?,
                Command::Delete => self
                    .store
                    .borrow_mut()
                    .delete(id)
                    .map_err(|e| e.to_string())?,
                Command::Edit => {
                    if !clip.is_text() {
                        return Ok(());
                    }
                    self.store
                        .borrow()
                        .hydrate(&mut clip)
                        .map_err(|e| e.to_string())?;
                    let input = cx.new(|cx| TextareaState::new(window, cx).rows(10));
                    input.update(cx, |s, cx| {
                        s.set_value(clip.text, window, cx);
                        s.focus(window, cx);
                    });
                    self.view = View::Edit { id, editor: input };
                }
                Command::Details => {
                    self.store
                        .borrow()
                        .hydrate(&mut clip)
                        .map_err(|e| e.to_string())?;
                    self.view = View::Details {
                        id,
                        rows: details(&clip),
                    };
                    self.focus.focus(window, cx);
                }
                Command::Open => {
                    if clip.kind == "link" {
                        self.store
                            .borrow()
                            .hydrate(&mut clip)
                            .map_err(|e| e.to_string())?;
                        platform::open_external(clip.text.trim())?;
                    }
                }
                Command::Save => {
                    self.store
                        .borrow()
                        .hydrate(&mut clip)
                        .map_err(|e| e.to_string())?;
                    let dir = crate::config::directory().map_err(|e| e.to_string())?;
                    let prompt = cx.prompt_for_new_path(&dir, Some("clip.png"));
                    let bytes = clip.payload;
                    cx.spawn(async move |_, _| {
                        if let Ok(Ok(Some(path))) = prompt.await
                            && let Err(error) = std::fs::write(path, bytes)
                        {
                            eprintln!("Save image: {error}");
                        }
                    })
                    .detach();
                }
                Command::Paste(_)
                | Command::Transform(_)
                | Command::Copy
                | Command::PasteFile(_)
                | Command::PasteImages
                | Command::CopyFile(_)
                | Command::FileText(..) => {
                    let plain = matches!(
                        cmd,
                        Command::Paste(true) | Command::Transform(_) | Command::FileText(..)
                    );
                    let file_text = matches!(cmd, Command::FileText(..))
                        || matches!(cmd, Command::Transform(t) if FILE_TRANSFORMS.contains(&t));
                    if plain && !clip.is_text() && !(file_text && clip.kind == "files") {
                        return Ok(());
                    }
                    self.store
                        .borrow()
                        .hydrate(&mut clip)
                        .map_err(|e| e.to_string())?;
                    let paths: Vec<String> = clip.text.lines().map(str::to_owned).collect();
                    let subset = match cmd {
                        Command::PasteFile(ix) | Command::CopyFile(ix) if clip.kind == "files" => {
                            Some(paths.get(ix).cloned().into_iter().collect::<Vec<_>>())
                        }
                        Command::PasteImages if clip.kind == "files" => Some(
                            paths
                                .iter()
                                .zip(clip.file_kinds())
                                .filter(|(_, k)| *k == Kind::Image)
                                .map(|(p, _)| p.clone())
                                .collect(),
                        ),
                        Command::PasteFile(_) | Command::PasteImages | Command::CopyFile(_) => {
                            return Ok(());
                        }
                        _ => None,
                    };
                    if subset.as_ref().is_some_and(|s| s.is_empty()) {
                        return Ok(());
                    }
                    if let Command::Transform(transform) = cmd {
                        clip.text = transform_text(&clip.text, transform);
                    }
                    if let Command::FileText(ix, name) = cmd {
                        let Some(path) = paths.get(ix) else {
                            return Ok(());
                        };
                        clip.text = if name { files::name(path) } else { path }.to_owned();
                    }
                    let clipboard = self.clipboard;
                    // Kept open, the popup stays put and pastes into the last real app.
                    let keep = platform::keep();
                    let target = if keep {
                        platform::last_app()
                    } else {
                        self.target
                    };
                    let mouse = self.mode.is_mouse() && !keep;
                    let delay = self.config.paste_delay_ms;
                    let copy = matches!(cmd, Command::Copy | Command::CopyFile(_));
                    if !copy && !keep {
                        platform::hide_popup(window);
                    }
                    std::thread::spawn(move || {
                        let written = match &subset {
                            Some(paths) => clipboard
                                .write_files(&paths.iter().map(String::as_str).collect::<Vec<_>>()),
                            None => clipboard.write_clip(&clip, plain),
                        };
                        if let Err(error) = written {
                            eprintln!("Clipboard write: {error}");
                            return;
                        }
                        if !copy {
                            std::thread::sleep(std::time::Duration::from_millis(delay));
                            match target.paste(mouse) {
                                Ok(()) => platform::paste_sent(
                                    clip.id,
                                    crate::store::content_hash(&clip.kind, &clip.payload),
                                    plain,
                                ),
                                Err(error) => eprintln!("Paste: {error}"),
                            }
                        }
                    });
                }
            }
            Ok(())
        })();
        if let Err(error) = result {
            self.status = Some(Status::Error(error));
        }
        self.reload(cx);
    }
    fn open_gallery(&mut self, clip: &crate::clip::Clip, cx: &mut Context<Self>) {
        let kinds = clip.file_kinds();
        let files: Vec<GalleryFile> = clip
            .paths()
            .iter()
            .zip(kinds)
            .map(|(path, kind)| GalleryFile {
                path: (*path).to_owned(),
                kind,
                missing: None,
                thumb: None,
                large: None,
                caption: None,
            })
            .collect();
        let single = files.len() == 1;
        let id = clip.id;
        let jobs: Vec<(String, Kind)> = files
            .iter()
            .take(400)
            .map(|f| (f.path.clone(), f.kind))
            .collect();
        self.view = View::Gallery(Gallery {
            id,
            files,
            selected: 0,
            open: single,
            single,
        });
        self.gallery_scroll = ScrollHandle::new();
        if single {
            self.load_gallery_detail(0, cx);
        }
        // Existence checks and Shell thumbnails, a few files at a time, off the UI thread.
        cx.spawn(async move |this, cx| {
            for start in (0..jobs.len()).step_by(8) {
                let chunk = jobs[start..(start + 8).min(jobs.len())].to_vec();
                let results = cx
                    .background_spawn(async move {
                        chunk
                            .into_iter()
                            .map(|(path, kind)| {
                                let missing = !Path::new(&path).exists();
                                let thumb = (!missing)
                                    .then(|| {
                                        platform::preview::shell_image(&path, 160, !kind.is_media())
                                            .and_then(|i| platform::preview::png(&i))
                                            .ok()
                                    })
                                    .flatten();
                                (missing, thumb)
                            })
                            .collect::<Vec<_>>()
                    })
                    .await;
                let more = this
                    .update(cx, |this, cx| {
                        let View::Gallery(g) = &mut this.view else {
                            return false;
                        };
                        if g.id != id {
                            return false;
                        }
                        for (k, (missing, thumb)) in results.into_iter().enumerate() {
                            let file = &mut g.files[start + k];
                            file.missing = Some(missing);
                            file.thumb = thumb.map(png);
                        }
                        cx.notify();
                        true
                    })
                    .unwrap_or(false);
                if !more {
                    break;
                }
            }
        })
        .detach();
    }
    /// Opens one file in its default app, or selects it in Explorer, off the UI thread.
    pub(super) fn open_file(&mut self, path: String, reveal: bool, cx: &mut Context<Self>) {
        let task = cx.background_spawn(async move {
            if reveal {
                platform::preview::reveal(&[std::path::PathBuf::from(&path)])
            } else {
                platform::open_external(&path)
            }
        });
        cx.spawn(async move |this, cx| {
            if let Err(error) = task.await {
                let _ = this.update(cx, |this, cx| {
                    this.status = Some(Status::Error(error));
                    cx.notify();
                });
            }
        })
        .detach();
    }
    pub(super) fn open_gallery_file(&mut self, ix: usize, cx: &mut Context<Self>) {
        if let View::Gallery(g) = &mut self.view {
            g.selected = ix;
            g.open = true;
        }
        self.load_gallery_detail(ix, cx);
        cx.notify();
    }
    /// Size, dimensions and a large Shell image for the gallery's opened file.
    pub(super) fn load_gallery_detail(&mut self, ix: usize, cx: &mut Context<Self>) {
        let View::Gallery(g) = &self.view else {
            return;
        };
        let Some(file) = g.files.get(ix).filter(|f| f.caption.is_none()) else {
            return;
        };
        let (id, path, kind) = (g.id, file.path.clone(), file.kind);
        let task = cx.background_spawn(async move {
            let Ok(meta) = std::fs::metadata(&path) else {
                return (
                    Some("Missing · this file is no longer available".into()),
                    None,
                );
            };
            let mut parts = vec![files::breakdown(&[kind])];
            if kind == Kind::Image
                && let Ok(size) = image::ImageReader::open(&path)
                    .and_then(|r| r.with_guessed_format())
                    .map_err(|e| e.to_string())
                    .and_then(|r| r.into_dimensions().map_err(|e| e.to_string()))
            {
                parts.push(format!("{}×{}", size.0, size.1));
            }
            if meta.is_file() {
                parts.push(size_label(meta.len()));
            }
            let large = (kind != Kind::Image)
                .then(|| {
                    platform::preview::shell_image(&path, 256, !kind.is_media())
                        .and_then(|i| platform::preview::png(&i))
                        .ok()
                })
                .flatten();
            (Some(parts.join(" · ")), large)
        });
        cx.spawn(async move |this, cx| {
            let (caption, large) = task.await;
            let _ = this.update(cx, |this, cx| {
                if let View::Gallery(g) = &mut this.view
                    && g.id == id
                    && let Some(file) = g.files.get_mut(ix)
                {
                    file.caption = caption;
                    file.large = large.map(png);
                    cx.notify();
                }
            });
        })
        .detach();
    }
    pub(super) fn save_edit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let View::Edit { id, editor } = &self.view {
            let text = editor.read(cx).value().to_string();
            let result = self.store.borrow_mut().edit(*id, &text);
            match result {
                Ok(()) => self.back(window, cx),
                Err(error) => self.status = Some(Status::Error(format!("Edit not saved: {error}"))),
            }
        }
        cx.notify();
    }
}

fn details(clip: &crate::clip::Clip) -> Vec<(&'static str, String)> {
    let mut rows = vec![
        (
            "Type",
            match clip.kind.as_str() {
                "link" => "Link",
                "email" => "Email address",
                "image" => "Image",
                "files" => "Files",
                _ => "Text",
            }
            .to_owned(),
        ),
        ("Source", clip.source.clone()),
        ("First copied", platform::when(clip.created)),
        ("Last copied", platform::when(clip.updated)),
        ("Times copied", clip.copies.to_string()),
        (
            "Pasted",
            if clip.pastes + clip.plain_pastes == 0 {
                "Not yet".into()
            } else {
                format!("{}× · {}× as plain text", clip.pastes, clip.plain_pastes)
            },
        ),
    ];
    match clip.kind.as_str() {
        "image" => {
            rows.push(("Dimensions", clip.text.clone()));
            rows.push(("Size", size_label(clip.payload.len() as u64)));
            if !clip.file_path.is_empty() {
                rows.push(("File", clip.file_path.clone()));
            }
        }
        "files" => {
            rows.push((
                "Files",
                format!(
                    "{} · {}",
                    clip.file_count,
                    files::breakdown(&clip.file_kinds())
                ),
            ));
            if let Some(first) = clip.paths().first() {
                rows.push((
                    "Folder",
                    Path::new(first)
                        .parent()
                        .map(|p| p.display().to_string())
                        .unwrap_or_default(),
                ));
            }
        }
        _ => {
            rows.push(("Characters", clip.text.chars().count().to_string()));
            rows.push(("Lines", clip.text.lines().count().max(1).to_string()));
        }
    }
    if !clip.formats.is_empty() {
        rows.push((
            "Formats",
            clip.formats
                .iter()
                .map(|f| f.name.as_str())
                .collect::<Vec<_>>()
                .join(", "),
        ));
        rows.push((
            "Stored size",
            size_label(clip.formats.iter().map(|f| f.data.len() as u64).sum()),
        ));
    }
    rows
}

fn transform_text(text: &str, transform: &str) -> String {
    match transform {
        "UPPERCASE" => text.to_uppercase(),
        "lowercase" => text.to_lowercase(),
        "Trim spaces" => text.trim().into(),
        "One line" => text.split_whitespace().collect::<Vec<_>>().join(" "),
        "Paths as text" => text.lines().collect::<Vec<_>>().join("\r\n"),
        "File names only" => text
            .lines()
            .map(files::name)
            .collect::<Vec<_>>()
            .join("\r\n"),
        _ => text.into(),
    }
}
#[cfg(test)]
mod tests {
    use super::{size_label, transform_text};
    #[test]
    fn transforms() {
        assert_eq!(transform_text(" éTé ", "UPPERCASE"), " ÉTÉ ");
        assert_eq!(transform_text(" A\n B\tC ", "One line"), "A B C");
        assert_eq!(transform_text(" x ", "Trim spaces"), "x");
        assert_eq!(transform_text("ÉTÉ", "lowercase"), "été");
        let files = "C:/p/ui.png\nC:/p/notes.pdf\nD:/assets/";
        assert_eq!(
            transform_text(files, "Paths as text"),
            "C:/p/ui.png\r\nC:/p/notes.pdf\r\nD:/assets/"
        );
        assert_eq!(
            transform_text(files, "File names only"),
            "ui.png\r\nnotes.pdf\r\nassets"
        );
    }
    #[test]
    fn sizes() {
        assert_eq!(size_label(512), "512 bytes");
        assert_eq!(size_label(250_000), "245 KB");
        assert_eq!(size_label(5_500_000), "5.2 MB");
    }
}
