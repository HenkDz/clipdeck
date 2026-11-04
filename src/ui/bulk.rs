use super::{Popup, commands::Command, popup::Status};
use crate::{platform, store};
use gpui::*;
impl Popup {
    pub(super) fn action_ids(&self, id: i64) -> Vec<i64> {
        if self.selection.contains(id) {
            self.clips
                .iter()
                .filter(|c| self.selection.contains(c.id))
                .map(|c| c.id)
                .collect()
        } else {
            vec![id]
        }
    }
    pub(super) fn batch_command(
        &mut self,
        ids: Vec<i64>,
        cmd: Command,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let result = match cmd {
            Command::Pin => {
                let pin = !self
                    .clips
                    .iter()
                    .filter(|c| ids.contains(&c.id))
                    .all(|c| c.pinned);
                self.store
                    .borrow_mut()
                    .pin_many(&ids, pin)
                    .map_err(|e| e.to_string())
            }
            Command::Delete => self
                .store
                .borrow_mut()
                .delete_many(&ids)
                .map_err(|e| e.to_string()),
            Command::Copy | Command::Paste(_) => {
                self.clipboard_command(ids, cmd, window, cx);
                return;
            }
            _ => Err("Select one clip for this action".into()),
        };
        if let Err(error) = result {
            self.status = Some(Status::Error(error));
        }
        self.reload(cx);
    }
    pub(super) fn clipboard_command(
        &mut self,
        ids: Vec<i64>,
        cmd: Command,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let selected = self
            .clips
            .iter()
            .filter(|c| ids.contains(&c.id))
            .collect::<Vec<_>>();
        let text = selected.iter().all(|c| c.is_text());
        let plain = matches!(cmd, Command::Paste(true));
        if (plain && !text)
            || (ids.len() > 1
                && !text
                && !selected
                    .iter()
                    .all(|c| matches!(c.kind.as_str(), "image" | "files")))
        {
            self.status = Some(Status::Error(
                "Select only text clips, or only images and files, for bulk copy/paste".into(),
            ));
            cx.notify();
            return;
        }
        let paste = !matches!(cmd, Command::Copy);
        let clipboard = self.clipboard;
        let keep = platform::keep();
        let target = if keep {
            platform::last_app()
        } else {
            self.target
        };
        let mouse = self.mode.is_mouse() && !keep;
        let delay = self.config.paste_delay_ms;
        let task = cx.background_spawn(async move {
            let mut clips = vec![];
            let mut size = 0usize;
            for id in ids {
                let clip = platform::locations::load_clip(id)?;
                size = size
                    .saturating_add(clip.payload.len())
                    .saturating_add(clip.formats.iter().map(|f| f.data.len()).sum::<usize>());
                if size > 128 * 1024 * 1024 {
                    return Err("Selection exceeds 128 MiB".to_string());
                }
                if plain && !clip.is_text() {
                    return Err("Plain-text paste requires text clips".into());
                }
                clips.push(clip);
            }
            let written = clipboard
                .write_clips(&clips, plain)
                .map_err(|e| e.to_string())?;
            if paste {
                std::thread::sleep(std::time::Duration::from_millis(delay));
                target.paste(mouse).map_err(|e| e.to_string())?;
                platform::clipboard_batch_sent(
                    clips
                        .iter()
                        .map(|clip| (clip.id, store::content_hash(&clip.kind, &clip.payload)))
                        .collect(),
                    plain,
                );
            }
            Ok::<_, String>((!paste).then(|| {
                (
                    written,
                    clips
                        .iter()
                        .map(|clip| (clip.id, store::content_hash(&clip.kind, &clip.payload)))
                        .collect::<Vec<_>>(),
                )
            }))
        });
        if paste && !keep {
            platform::hide_popup(window);
        }
        cx.spawn(async move |this, cx| {
            let result = task.await;
            let _ = this.update(cx, |this, cx| {
                match result {
                    Ok(Some((written, members))) => {
                        let saved = this.store.borrow_mut().copied(&written, &members);
                        match saved {
                            Ok(current) => {
                                platform::set_current_ids(current, written.sequence);
                                this.status = Some(Status::Info(if members.len() > 1 {
                                    format!("Copied {} items as one batch", members.len())
                                } else {
                                    "Copied to clipboard".into()
                                }));
                            }
                            Err(error) => {
                                this.status = Some(Status::Error(format!(
                                    "Copied to clipboard, but history was not updated: {error}"
                                )))
                            }
                        }
                        this.reload(cx);
                        this.list.scroll_to_reveal_item(0);
                    }
                    Ok(None) => {}
                    Err(error) => this.status = Some(Status::Error(error)),
                }
                cx.notify();
            });
        })
        .detach();
    }
}
