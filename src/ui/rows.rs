use super::{
    Popup,
    commands::Command,
    popup::Menu,
    theme::{LABEL, META, Palette, RADIUS, S1, S2, S3, TITLE, palette},
    widgets::{ghost, icon},
};
use crate::{clip::Clip, files::Kind};
use gpui::{prelude::FluentBuilder, *};
use gpui_component::{Icon, menu::PopupMenu};
use gpui_kit_assets::IconName;
use std::sync::Arc;

pub(super) const COMFORTABLE: Pixels = px(52.);
pub(super) const COMPACT: Pixels = px(30.);

pub(super) fn file_icon(kind: Kind) -> IconName {
    match kind {
        Kind::Image => IconName::FileImage,
        Kind::Video => IconName::FilePlay,
        Kind::Audio => IconName::FileMusic,
        Kind::Pdf | Kind::Document => IconName::FileText,
        Kind::Spreadsheet => IconName::FileSpreadsheet,
        Kind::Slides => IconName::Presentation,
        Kind::Archive => IconName::FileArchive,
        Kind::Code => IconName::FileCode,
        Kind::Folder => IconName::Folder,
        Kind::Other => IconName::File,
    }
}

/// The type glyph and its tile colors (background, foreground).
fn kind_style(clip: &Clip, p: Palette) -> (IconName, (Hsla, Hsla)) {
    match clip.kind.as_str() {
        "link" => (IconName::Link, p.link),
        "email" => (IconName::AtSign, p.mail),
        "image" => (IconName::Image, p.neutral),
        "files" => match clip.file_kinds().as_slice() {
            [kind] => (file_icon(*kind), p.file),
            _ => (IconName::Files, p.file),
        },
        _ => (IconName::TextAlignStart, p.neutral),
    }
}

/// A paste glyph tinted green once used, with its count beside it.
pub(super) fn paste_glyph(name: IconName, count: i64, size: Pixels, p: Palette) -> Div {
    div()
        .flex()
        .items_center()
        .gap(px(2.))
        .child(if count > 0 {
            icon(name, size, p.success)
        } else {
            Icon::new(name).size(size)
        })
        .when(count >= 2, |el| {
            el.child(
                div()
                    .text_size(px(9.))
                    .line_height(px(9.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(p.success)
                    .child(format!("×{count}")),
            )
        })
}

impl Popup {
    fn section(&self, item: usize, p: Palette) -> AnyElement {
        let label = self
            .items
            .get(item + 1)
            .copied()
            .flatten()
            .map(|ix| self.clips[ix].section())
            .unwrap_or("");
        div()
            .w_full()
            .h(if self.compact() { px(24.) } else { px(28.) })
            .px(S2)
            .pb(S1)
            .flex()
            .items_end()
            .text_size(LABEL)
            .font_weight(FontWeight::MEDIUM)
            .text_color(p.muted)
            .child(label)
            .into_any_element()
    }

    /// The fixed-width leading column: type tile, thumbnail or file mosaic.
    fn leading(&self, clip: &Clip, ring: Hsla, p: Palette) -> Div {
        let compact = self.compact();
        let current = crate::platform::is_current(clip.id);
        let (glyph, (tile_bg, tile_fg)) = kind_style(clip, p);
        let mosaic = clip.kind == "files" && clip.file_count > 1;
        let thumb = (!clip.thumbnail.is_empty()).then(|| {
            img(Arc::new(Image::from_bytes(
                ImageFormat::Png,
                clip.thumbnail.clone(),
            )))
            .size_full()
            .object_fit(if mosaic {
                ObjectFit::Contain
            } else {
                ObjectFit::Cover
            })
        });
        let (slot_w, slot_h) = if compact {
            (px(22.), px(16.))
        } else {
            (px(44.), px(32.))
        };
        let inner = match thumb {
            Some(thumb) if mosaic => div().size_full().child(thumb),
            Some(thumb) => div()
                .size_full()
                .rounded(if compact { px(3.) } else { px(6.) })
                .overflow_hidden()
                .border_1()
                .border_color(p.hairline)
                .bg(p.well)
                .child(thumb),
            None if compact => div()
                .size_full()
                .flex()
                .items_center()
                .justify_center()
                .child(icon(glyph, px(16.), tile_fg)),
            None => div()
                .size(px(32.))
                .rounded(RADIUS)
                .flex()
                .items_center()
                .justify_center()
                .bg(tile_bg)
                .child(icon(glyph, px(16.), tile_fg)),
        };
        // More than four files: the mosaic's last cell carries the remaining count.
        let badge = (mosaic && clip.file_count > 4 && !compact).then(|| {
            div()
                .absolute()
                .right_0()
                .bottom_0()
                .w(px(21.))
                .h(px(15.))
                .rounded(px(3.))
                .bg(tile_bg)
                .flex()
                .items_center()
                .justify_center()
                .text_size(px(10.))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(tile_fg)
                .child(format!("+{}", (clip.file_count - 3).min(99)))
        });
        let dot = current.then(|| {
            div()
                .id("on-clipboard")
                .absolute()
                .top(px(-3.))
                .right(px(-3.))
                .size(px(8.))
                .rounded_full()
                .border_2()
                .border_color(ring)
                .group_hover("row", move |s| s.border_color(p.hover))
                .bg(p.accent)
                .tooltip(|window, cx| {
                    gpui_component::tooltip::Tooltip::new("On the clipboard now").build(window, cx)
                })
        });
        div()
            .w(slot_w)
            .h(slot_h)
            .flex_none()
            .flex()
            .items_center()
            .justify_center()
            .child(
                inner
                    .relative()
                    .children(badge)
                    .children(dot)
                    .when(compact, |el| el.size_full()),
            )
    }

    fn title_line(clip: &Clip, p: Palette) -> Div {
        let (title, names) = clip.title();
        div()
            .flex()
            .min_w_0()
            .overflow_hidden()
            .gap(S2)
            .text_size(TITLE)
            .line_height(px(18.))
            .text_color(p.text)
            .child(
                div()
                    .when(names.is_some(), |el| el.flex_none())
                    .when(names.is_none(), |el| el.flex_1().min_w_0())
                    .truncate()
                    .child(title),
            )
            .children(names.map(|names| {
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .text_color(p.secondary)
                    .child(names)
            }))
    }

    /// Quiet trailing indicators; the action overlay covers them when it shows.
    fn indicators(clip: &Clip, compact: bool, p: Palette) -> Div {
        let quiet = |el: Div| el.opacity(0.8);
        div()
            .flex_none()
            .flex()
            .items_center()
            .gap(S2)
            .when(clip.pastes > 0, |el| {
                el.child(quiet(paste_glyph(
                    IconName::ClipboardPaste,
                    clip.pastes,
                    px(14.),
                    p,
                )))
            })
            .when(clip.plain_pastes > 0, |el| {
                el.child(quiet(paste_glyph(
                    IconName::ClipboardType,
                    clip.plain_pastes,
                    px(14.),
                    p,
                )))
            })
            .when(clip.pinned, |el| {
                el.child(icon(IconName::Pin, px(14.), p.muted))
            })
            .when(compact, |el| {
                el.child(
                    div()
                        .w(px(28.))
                        .text_right()
                        .text_size(LABEL)
                        .text_color(p.muted)
                        .child(clip.short_age()),
                )
            })
    }

    /// Inline actions drawn over the row's right end, fading in from the row color.
    fn actions(&self, clip: &Clip, bg: Hsla, cx: &mut Context<Self>) -> Div {
        let p = palette(cx);
        let id = clip.id;
        let side = if self.compact() { px(24.) } else { px(28.) };
        let glyph = if self.compact() { px(14.) } else { px(16.) };
        let mut buttons: Vec<(IconName, &'static str, Command, i64)> = vec![(
            IconName::ClipboardPaste,
            "Paste",
            Command::Paste(false),
            clip.pastes,
        )];
        if clip.kind == "link" {
            buttons.insert(
                0,
                (
                    IconName::ExternalLink,
                    "Open in default app",
                    Command::Open,
                    0,
                ),
            );
        }
        if clip.is_text() {
            buttons.push((
                IconName::ClipboardType,
                "Paste as plain text",
                Command::Paste(true),
                clip.plain_pastes,
            ));
        }
        buttons.push(if clip.pinned {
            (IconName::PinOff, "Unpin", Command::Pin, 0)
        } else {
            (IconName::Pin, "Pin", Command::Pin, 0)
        });
        buttons.push((IconName::Trash, "Delete", Command::Delete, 0));
        div()
            .absolute()
            .top_0()
            .bottom_0()
            .right_0()
            .flex()
            .items_center()
            .child(div().w(px(24.)).h_full().bg(linear_gradient(
                90.,
                linear_color_stop(bg.opacity(0.), 0.),
                linear_color_stop(bg, 1.),
            )))
            .child(
                div()
                    .h_full()
                    .pr(S1)
                    .flex()
                    .items_center()
                    .bg(bg)
                    .rounded_r(if self.compact() { px(6.) } else { RADIUS })
                    .children(buttons.into_iter().map(|(name, label, cmd, count)| {
                        let danger = cmd == Command::Delete;
                        let colors = if danger {
                            Palette {
                                text: p.danger,
                                ..p
                            }
                        } else {
                            p
                        };
                        ghost((label, id as u64), label, side, colors)
                            .when(count >= 2, |el| el.w_auto().min_w(side).px(S1))
                            .child(paste_glyph(name, count, glyph, p))
                            .on_click(cx.listener(move |this, _, window, cx| {
                                cx.stop_propagation();
                                this.command(id, cmd, window, cx);
                            }))
                    })),
            )
    }

    pub(super) fn row(
        &mut self,
        item: usize,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let p = palette(cx);
        let Some(ix) = self.items[item] else {
            return self.section(item, p);
        };
        let compact = self.compact();
        let clip = &self.clips[ix];
        let id = clip.id;
        let selected = self.selection.contains(id);
        let row_bg = if selected { p.selected } else { p.surface };
        let radius = if compact { px(6.) } else { RADIUS };
        let inset = if compact { px(6.) } else { px(10.) };
        let text = if compact {
            Self::title_line(clip, p).flex_1()
        } else {
            div()
                .flex_1()
                .min_w_0()
                .flex()
                .flex_col()
                .child(Self::title_line(clip, p))
                .child(
                    div()
                        .text_size(META)
                        .line_height(px(16.))
                        .text_color(p.muted)
                        .truncate()
                        .child(clip.meta()),
                )
        };
        let actions = self
            .actions(clip, if selected { p.selected } else { p.hover }, cx)
            .when(!selected, |el| {
                el.invisible().group_hover("row", |s| s.visible())
            });
        div()
            .id(("clip", id as u64))
            .group("row")
            .aria_selected(selected)
            .w_full()
            .h(if compact { COMPACT } else { COMFORTABLE })
            .px(S2)
            .relative()
            .flex()
            .items_center()
            .gap(if compact { S2 } else { S3 })
            .rounded(radius)
            .bg(row_bg)
            .when(!selected, |el| el.hover(|s| s.bg(p.hover)))
            .children(
                (clip.copies > 1 || crate::platform::is_current(id)).then(|| {
                    div()
                        .absolute()
                        .left_0()
                        .top(inset)
                        .bottom(inset)
                        .w(px(3.))
                        .rounded_full()
                        .bg(p.accent)
                }),
            )
            .child(self.leading(clip, row_bg, p))
            .child(text)
            .child(Self::indicators(clip, compact, p))
            .child(actions)
            .on_click(cx.listener(move |this, event: &ClickEvent, window, cx| {
                let mods = event.modifiers();
                this.mode.search();
                crate::platform::mouse_mode(window, false, this.target);
                this.activate(window, cx);
                this.selection.click(
                    &this.clips.iter().map(|c| c.id).collect::<Vec<_>>(),
                    id,
                    mods.control,
                    mods.shift,
                );
                if event.click_count() == 2 && !mods.control && !mods.shift {
                    this.command(id, Command::Paste(false), window, cx);
                }
                cx.notify();
            }))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, event: &MouseDownEvent, _, _| {
                    this.press = (!event.modifiers.control && !event.modifiers.shift).then_some((
                        id,
                        None,
                        event.position,
                    ));
                }),
            )
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                    let view = cx.entity().downgrade();
                    let Some(clip) = this.clips.iter().find(|c| c.id == id).cloned() else {
                        return;
                    };
                    if !this.selection.contains(id) {
                        this.selection.click(
                            &this.clips.iter().map(|c| c.id).collect::<Vec<_>>(),
                            id,
                            false,
                            false,
                        );
                    } else {
                        this.selection.focus = Some(id);
                    }
                    let count = this.selection.len();
                    let unpin = this
                        .clips
                        .iter()
                        .filter(|c| this.selection.contains(c.id))
                        .all(|c| c.pinned);
                    let menu = PopupMenu::build(window, cx, move |menu, window, cx| {
                        super::menu::build(
                            menu,
                            view.clone(),
                            id,
                            &clip,
                            (count, unpin),
                            window,
                            cx,
                        )
                    });
                    // Start at the left inset so submenus have room to open rightwards.
                    let at = point(S2, event.position.y);
                    this.open_menu(menu, at, Anchor::TopLeft, window, cx);
                    cx.stop_propagation();
                }),
            )
            .into_any_element()
    }

    pub(super) fn open_menu(
        &mut self,
        menu: Entity<PopupMenu>,
        position: Point<Pixels>,
        corner: Anchor,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        cx.subscribe_in(&menu, window, |this, _, _: &DismissEvent, window, cx| {
            this.menu = None;
            if matches!(this.view, super::popup::View::List) {
                this.input.update(cx, |s, cx| s.focus(window, cx));
            } else {
                // Gallery keys need focus back on the popup after the menu closes.
                this.focus.focus(window, cx);
            }
            cx.notify();
        })
        .detach();
        menu.focus_handle(cx).focus(window, cx);
        self.menu = Some(Menu {
            entity: menu,
            position,
            corner,
        });
        cx.notify();
    }
}
