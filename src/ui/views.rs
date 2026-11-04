use super::{
    Popup,
    commands::Command,
    popup::{Gallery, View},
    rows::file_icon,
    theme::{LABEL, META, Palette, RADIUS, S1, S2, S3, S4, TITLE, palette},
    widgets::{button, icon, icon_button},
};
use crate::files::{self, Kind};
use gpui::{prelude::FluentBuilder, *};
use gpui_component::{input::Textarea, menu::PopupMenu};
use gpui_kit_assets::IconName;
use std::path::PathBuf;

const TILE_MIN: f32 = 104.;

/// Gallery columns for the current popup width.
pub(super) fn columns(window: &Window) -> usize {
    let inner = f32::from(window.viewport_size().width) - 2. * 12. + 8.;
    ((inner / (TILE_MIN + 8.)).floor() as usize).clamp(2, 8)
}

fn caption(text: impl Into<SharedString>, p: Palette) -> Div {
    div()
        .text_size(META)
        .line_height(px(16.))
        .text_color(p.muted)
        .truncate()
        .child(text.into())
}

/// A recessed panel that fits its content, for image and file previews.
fn well(p: Palette) -> Div {
    div()
        .flex_1()
        .min_h_0()
        .rounded(RADIUS)
        .border_1()
        .border_color(p.hairline)
        .bg(p.well)
        .overflow_hidden()
        .p(S3)
        .flex()
        .items_center()
        .justify_center()
}

impl Popup {
    /// Right-click menu for one file of the gallery; selects that file first.
    fn file_menu(
        &mut self,
        ix: usize,
        position: Point<Pixels>,
        in_grid: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let View::Gallery(g) = &mut self.view else {
            return;
        };
        let Some(file) = g.files.get(ix) else {
            return;
        };
        g.selected = ix;
        let (id, path, missing) = (g.id, file.path.clone(), file.missing == Some(true));
        let view = cx.entity().downgrade();
        let menu = PopupMenu::build(window, cx, move |menu, _, _| {
            super::menu::file(menu, view.clone(), id, ix, path.clone(), missing, in_grid)
        });
        self.open_menu(menu, position, Anchor::TopLeft, window, cx);
    }
    fn bar(
        &self,
        title: SharedString,
        detail: Option<String>,
        actions: Vec<AnyElement>,
        cx: &mut Context<Self>,
    ) -> Div {
        let p = palette(cx);
        div()
            .flex_none()
            .h(px(48.))
            .px(S2)
            .flex()
            .items_center()
            .gap(S2)
            .border_b_1()
            .border_color(p.hairline)
            .child(
                icon_button("back", IconName::ArrowLeft, "Back", px(32.), p)
                    .on_click(cx.listener(|this, _, window, cx| this.back(window, cx))),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .items_baseline()
                    .gap(S2)
                    .child(
                        div()
                            .min_w_0()
                            .truncate()
                            .text_size(TITLE)
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(title),
                    )
                    .children(detail.map(|d| {
                        div()
                            .flex_none()
                            .text_size(META)
                            .text_color(p.muted)
                            .child(d)
                    })),
            )
            .child(div().flex().items_center().gap(S1).children(actions))
    }

    fn act(
        &self,
        id: i64,
        name: IconName,
        label: &'static str,
        cmd: Command,
        primary: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        button(label, Some(name), label, primary, palette(cx))
            .on_click(cx.listener(move |this, _, window, cx| this.command(id, cmd, window, cx)))
            .into_any_element()
    }

    fn clip_title(&self, id: i64) -> SharedString {
        self.clips
            .iter()
            .find(|c| c.id == id)
            .map(|c| c.title().0)
            .unwrap_or_else(|| "Clip".into())
            .into()
    }

    pub(super) fn subview(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let p = palette(cx);
        let column = div().flex_1().min_h_0().flex().flex_col();
        match &self.view {
            View::List => div().into_any_element(),
            View::Text { id, text } => {
                let id = *id;
                let clip = self.clips.iter().find(|c| c.id == id).cloned();
                let kind = clip.as_ref().map_or("text", |c| c.kind.as_str());
                let title = match kind {
                    "link" => "Link",
                    "email" => "Email address",
                    _ => "Text",
                };
                let actions = vec![
                    self.act(id, IconName::Copy, "Copy", Command::Copy, false, cx),
                    self.act(id, IconName::Pencil, "Edit", Command::Edit, false, cx),
                    self.act(
                        id,
                        IconName::ClipboardPaste,
                        "Paste",
                        Command::Paste(false),
                        true,
                        cx,
                    ),
                ];
                let info = text.as_ref().map(|t| {
                    let lines = t.lines().count().max(1);
                    format!(
                        "{} characters · {lines} {}{}",
                        t.chars().count(),
                        if lines == 1 { "line" } else { "lines" },
                        clip.as_ref()
                            .map(|c| format!(" · {} · {}", c.source, c.age()))
                            .unwrap_or_default()
                    )
                });
                let body = text.clone().unwrap_or_else(|| "Loading…".into());
                column
                    .child(self.bar(title.into(), None, actions, cx))
                    .child(
                        div()
                            .id("preview-text")
                            .flex_1()
                            .min_h_0()
                            .overflow_y_scroll()
                            .px(S4)
                            .py(S3)
                            .text_size(TITLE)
                            .line_height(px(20.))
                            .child(body),
                    )
                    .children(info.map(|i| {
                        div()
                            .flex_none()
                            .px(S4)
                            .py(S2)
                            .border_t_1()
                            .border_color(p.hairline)
                            .child(caption(i, p))
                    }))
                    .into_any_element()
            }
            View::Image {
                id,
                image,
                caption: info,
            } => {
                let id = *id;
                let actions = vec![
                    self.act(
                        id,
                        IconName::ExternalLink,
                        "Open",
                        Command::OpenExternal,
                        false,
                        cx,
                    ),
                    self.act(
                        id,
                        IconName::FolderOpen,
                        "Show in folder",
                        Command::OpenLocation,
                        false,
                        cx,
                    ),
                    self.act(
                        id,
                        IconName::ClipboardPaste,
                        "Paste",
                        Command::Paste(false),
                        true,
                        cx,
                    ),
                ];
                let content = match image {
                    Some(image) => img(image.clone())
                        .size_full()
                        .object_fit(ObjectFit::Contain)
                        .into_any_element(),
                    None => caption("Loading…", p).into_any_element(),
                };
                let info = info.clone();
                column
                    .child(self.bar(self.clip_title(id), None, actions, cx))
                    .child(
                        div()
                            .flex_1()
                            .min_h_0()
                            .p(S3)
                            .flex()
                            .flex_col()
                            .gap(S2)
                            .child(well(p).child(content))
                            .child(caption(info, p).text_center()),
                    )
                    .into_any_element()
            }
            View::Gallery(g) if g.open => self.file_preview(g, cx),
            View::Gallery(g) => self.gallery(g, window, cx),
            View::Edit { editor, .. } => {
                let editor = editor.clone();
                let actions = vec![
                    button("cancel", None, "Cancel", false, p)
                        .on_click(cx.listener(|this, _, window, cx| this.back(window, cx)))
                        .into_any_element(),
                    button("save", Some(IconName::Check), "Save", true, p)
                        .on_click(cx.listener(|this, _, window, cx| this.save_edit(window, cx)))
                        .into_any_element(),
                ];
                column
                    .child(self.bar("Edit clip".into(), None, actions, cx))
                    .child(
                        div()
                            .flex_1()
                            .min_h_0()
                            .p(S3)
                            .flex()
                            .flex_col()
                            .gap(S2)
                            .child(Textarea::new(&editor).flex_1().min_h_0().h_full())
                            .child(caption(
                                "Saved as plain text; rich formatting is dropped",
                                p,
                            )),
                    )
                    .into_any_element()
            }
            View::Details { id, rows } => {
                let id = *id;
                let rows = rows.clone();
                column
                    .child(self.bar("Details".into(), None, vec![], cx))
                    .child(
                        div()
                            .id("details")
                            .flex_1()
                            .min_h_0()
                            .overflow_y_scroll()
                            .px(S4)
                            .py(S3)
                            .flex()
                            .flex_col()
                            .gap(S2)
                            .child(
                                div()
                                    .pb(S1)
                                    .truncate()
                                    .text_size(TITLE)
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child(self.clip_title(id)),
                            )
                            .children(rows.into_iter().map(|(label, value)| {
                                div()
                                    .flex()
                                    .gap(S3)
                                    .child(
                                        div()
                                            .w(px(96.))
                                            .flex_none()
                                            .text_size(META)
                                            .line_height(px(20.))
                                            .text_color(p.muted)
                                            .child(label),
                                    )
                                    .child(
                                        div()
                                            .flex_1()
                                            .min_w_0()
                                            .text_size(TITLE)
                                            .line_height(px(20.))
                                            .child(value),
                                    )
                            })),
                    )
                    .into_any_element()
            }
        }
    }

    fn gallery(&self, g: &Gallery, window: &Window, cx: &mut Context<Self>) -> AnyElement {
        let p = palette(cx);
        let id = g.id;
        let cols = columns(window);
        let inner = f32::from(window.viewport_size().width) - 2. * 12.;
        let tile = ((inner - 8. * (cols as f32 - 1.)) / cols as f32).floor();
        let actions = vec![
            self.act(
                id,
                IconName::FolderOpen,
                "Show in folder",
                Command::OpenLocation,
                false,
                cx,
            ),
            self.act(
                id,
                IconName::ClipboardPaste,
                "Paste all",
                Command::Paste(false),
                true,
                cx,
            ),
        ];
        let missing = g.files.iter().filter(|f| f.missing == Some(true)).count();
        let detail = if missing > 0 {
            format!("{missing} missing")
        } else {
            files::breakdown(&g.files.iter().map(|f| f.kind).collect::<Vec<_>>())
        };
        let tiles = g.files.iter().enumerate().map(|(ix, f)| {
            let selected = ix == g.selected;
            let gone = f.missing == Some(true);
            let preview = match &f.thumb {
                Some(thumb) => img(thumb.clone())
                    .size_full()
                    .object_fit(ObjectFit::Contain)
                    .into_any_element(),
                None => icon(file_icon(f.kind), px(28.), p.muted).into_any_element(),
            };
            div()
                .id(("file", ix))
                .w(px(tile))
                .p(S1)
                .flex()
                .flex_col()
                .gap(S1)
                .rounded(RADIUS)
                .when(selected, |el| el.bg(p.selected))
                .when(!selected, |el| el.hover(|s| s.bg(p.hover)))
                .child(
                    div()
                        .w_full()
                        .h(px((tile * 0.7).floor()))
                        .rounded(px(6.))
                        .border_1()
                        .border_color(if selected { p.border } else { p.hairline })
                        .bg(p.well)
                        .overflow_hidden()
                        .p(S1)
                        .flex()
                        .items_center()
                        .justify_center()
                        .when(gone, |el| el.opacity(0.45))
                        .child(preview),
                )
                .child(
                    div()
                        .px(S1)
                        .text_size(META)
                        .line_height(px(16.))
                        .truncate()
                        .text_center()
                        .text_color(if gone { p.muted } else { p.text })
                        .child(files::name(&f.path).to_owned()),
                )
                .children(gone.then(|| {
                    div()
                        .text_size(LABEL)
                        .text_center()
                        .text_color(p.danger)
                        .child("Missing")
                }))
                .on_click(cx.listener(move |this, event: &ClickEvent, _, cx| {
                    if let View::Gallery(g) = &mut this.view {
                        g.selected = ix;
                        g.open = event.click_count() == 2;
                    }
                    if event.click_count() == 2 {
                        this.load_gallery_detail(ix, cx);
                    }
                    cx.notify();
                }))
                .on_mouse_down(
                    MouseButton::Right,
                    cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                        cx.stop_propagation();
                        this.file_menu(ix, event.position, true, window, cx);
                    }),
                )
        });
        let tiles: Vec<_> = tiles.collect();
        div()
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            .child(self.bar(
                format!("{} files", g.files.len()).into(),
                Some(detail),
                actions,
                cx,
            ))
            .child(
                div()
                    .id("gallery")
                    .track_scroll(&self.gallery_scroll)
                    .overflow_y_scroll()
                    .flex_1()
                    .min_h_0()
                    .p(S3)
                    .flex()
                    .flex_wrap()
                    .content_start()
                    .gap(S2)
                    .children(tiles),
            )
            .into_any_element()
    }

    fn file_preview(&self, g: &Gallery, cx: &mut Context<Self>) -> AnyElement {
        let p = palette(cx);
        let id = g.id;
        let ix = g.selected;
        let Some(f) = g.files.get(ix) else {
            return div().into_any_element();
        };
        let path = f.path.clone();
        let reveal = path.clone();
        let open = path.clone();
        let actions = vec![
            button("open", Some(IconName::ExternalLink), "Open", false, p)
                .on_click(cx.listener(move |this, _, _, cx| {
                    if let Err(error) = crate::platform::open_external(&open) {
                        this.status = Some(super::popup::Status::Error(error));
                        cx.notify();
                    }
                }))
                .into_any_element(),
            button(
                "show",
                Some(IconName::FolderOpen),
                "Show in folder",
                false,
                p,
            )
            .on_click(cx.listener(move |_, _, _, cx| {
                let path = PathBuf::from(&reveal);
                cx.background_spawn(async move {
                    if let Err(error) = crate::platform::preview::reveal(&[path]) {
                        eprintln!("Show in folder: {error}");
                    }
                })
                .detach();
            }))
            .into_any_element(),
            self.act(
                id,
                IconName::ClipboardPaste,
                "Paste",
                Command::PasteFile(ix),
                true,
                cx,
            ),
        ];
        let content = if f.missing == Some(true) {
            div()
                .flex()
                .flex_col()
                .items_center()
                .gap(S2)
                .child(icon(file_icon(f.kind), px(48.), p.muted))
                .child(caption("This file is no longer available", p))
                .into_any_element()
        } else if f.kind == Kind::Image {
            img(PathBuf::from(&path))
                .size_full()
                .object_fit(ObjectFit::Contain)
                .into_any_element()
        } else if let Some(large) = f.large.as_ref().or(f.thumb.as_ref()) {
            img(large.clone())
                .size_full()
                .max_w(px(256.))
                .max_h(px(256.))
                .object_fit(ObjectFit::Contain)
                .into_any_element()
        } else {
            icon(file_icon(f.kind), px(64.), p.muted).into_any_element()
        };
        let folder = std::path::Path::new(&path)
            .parent()
            .map(|d| d.display().to_string())
            .unwrap_or_default();
        let position = (!g.single).then(|| format!("{} of {}", ix + 1, g.files.len()));
        div()
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            .child(self.bar(files::name(&path).to_owned().into(), position, actions, cx))
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .p(S3)
                    .flex()
                    .flex_col()
                    .gap(S2)
                    .child(
                        well(p)
                            .id("file-well")
                            .on_mouse_down(
                                MouseButton::Right,
                                cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                                    cx.stop_propagation();
                                    this.file_menu(ix, event.position, false, window, cx);
                                }),
                            )
                            .child(content),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .items_center()
                            .child(
                                caption(f.caption.clone().unwrap_or_else(|| "…".into()), p)
                                    .text_color(p.secondary),
                            )
                            .child(caption(folder, p)),
                    ),
            )
            .into_any_element()
    }
}
