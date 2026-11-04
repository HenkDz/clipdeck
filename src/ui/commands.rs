use super::Popup;
use crate::platform;
use gpui::*;
use gpui_component::input::TextareaState;
#[derive(Clone, Copy)]
pub(super) enum Command {
    Preview,
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
        let result: Result<(), String> = (|| {
            match cmd {
                Command::Preview | Command::OpenLocation => {
                    let location = matches!(cmd, Command::OpenLocation);
                    self.preview = Some(
                        if location {
                            "Opening file location…"
                        } else {
                            "Opening preview…"
                        }
                        .into(),
                    );
                    self.preview_id = Some(id);
                    let task =
                        cx.background_spawn(async move { platform::preview::load(id, location) });
                    cx.spawn(async move |this, cx| {
                        let result = task.await;
                        let _ = this.update(cx, |this, cx| {
                            if this.preview_id == Some(id) {
                                match result {
                                    Ok(preview) => this.preview = preview,
                                    Err(error) => this.preview = Some(error),
                                }
                                this.reload(cx);
                            }
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
                    self.editor = Some((id, input));
                }
                Command::Details => {
                    self.store
                        .borrow()
                        .hydrate(&mut clip)
                        .map_err(|e| e.to_string())?;
                    self.details = Some(format!(
                        "Type: {}\nFormats: {}\nSize: {} bytes\nSource: {}\nCreated: {} (Unix seconds)",
                        clip.kind,
                        clip.formats
                            .iter()
                            .map(|f| f.name.as_str())
                            .collect::<Vec<_>>()
                            .join(", "),
                        clip.formats.iter().map(|f| f.data.len()).sum::<usize>(),
                        clip.source,
                        clip.created
                    ));
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
                Command::Paste(_) | Command::Transform(_) | Command::Copy => {
                    let plain = matches!(cmd, Command::Paste(true) | Command::Transform(_));
                    if plain && !clip.is_text() {
                        return Ok(());
                    }
                    self.store
                        .borrow()
                        .hydrate(&mut clip)
                        .map_err(|e| e.to_string())?;
                    if let Command::Transform(transform) = cmd {
                        clip.text = transform_text(&clip.text, transform);
                    }
                    let clipboard = self.clipboard;
                    let target = self.target;
                    let mouse = self.mode.is_mouse();
                    let delay = self.delay;
                    if !matches!(cmd, Command::Copy) {
                        platform::hide_popup(window);
                    }
                    std::thread::spawn(move || {
                        if let Err(error) = clipboard.write_clip(&clip, plain) {
                            eprintln!("Clipboard write: {error}");
                            return;
                        }
                        if !matches!(cmd, Command::Copy) {
                            std::thread::sleep(std::time::Duration::from_millis(delay));
                            match target.paste(mouse) {
                                Ok(()) => platform::paste_sent(
                                    clip.id,
                                    crate::store::content_hash(&clip.kind, &clip.payload),
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
            self.error = Some(error);
        }
        self.reload(cx);
    }
    pub(super) fn save_edit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some((id, input)) = &self.editor {
            let text = input.read(cx).value().to_string();
            let result = self.store.borrow_mut().edit(*id, &text);
            match result {
                Ok(()) => {
                    self.editor = None;
                    self.input.update(cx, |s, cx| s.focus(window, cx));
                    self.reload(cx);
                }
                Err(error) => self.error = Some(format!("Edit not saved: {error}")),
            }
        }
        cx.notify();
    }
}
fn transform_text(text: &str, transform: &str) -> String {
    match transform {
        "UPPERCASE" => text.to_uppercase(),
        "lowercase" => text.to_lowercase(),
        "Trim spaces" => text.trim().into(),
        "One line" => text.split_whitespace().collect::<Vec<_>>().join(" "),
        _ => text.into(),
    }
}
#[cfg(test)]
mod tests {
    use super::transform_text;
    #[test]
    fn transforms() {
        assert_eq!(transform_text(" éTé ", "UPPERCASE"), " ÉTÉ ");
        assert_eq!(transform_text(" A\n B\tC ", "One line"), "A B C");
        assert_eq!(transform_text(" x ", "Trim spaces"), "x");
        assert_eq!(transform_text("ÉTÉ", "lowercase"), "été");
    }
}
