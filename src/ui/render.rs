use super::{
    Popup,
    popup::{FILTERS, Status, View},
    theme::{LABEL, META, Palette, RADIUS, S1, S2, S3, palette},
    widgets::{icon, icon_button, keycap},
};
use gpui::{prelude::FluentBuilder, *};
use gpui_component::{ActiveTheme, input::Input, menu::PopupMenu};
use gpui_kit_assets::IconName;
use std::sync::atomic::Ordering;

/// Rough text widths for fitting footer hints on one line without measuring.
fn hint_width(key: &str, label: &str) -> f32 {
    key.chars().count() as f32 * 6.5 + 10. + 4. + label.chars().count() as f32 * 6.4
}

fn status_line(name: IconName, text: String, color: Hsla, p: Palette) -> Div {
    div()
        .flex_none()
        .h(px(28.))
        .px(S3)
        .flex()
        .items_center()
        .gap(S2)
        .border_t_1()
        .border_color(p.hairline)
        .bg(p.well)
        .text_size(META)
        .text_color(color)
        .child(icon(name, px(14.), color))
        .child(div().flex_1().min_w_0().truncate().child(text))
}

impl Popup {
    fn header(&self, window: &mut Window, cx: &mut Context<Self>) -> Div {
        let p = palette(cx);
        let keep = self.config.keep_open;
        let focused = self.input.read(cx).focus_handle(cx).is_focused(window);
        let search = div()
            .flex_1()
            .min_w_0()
            .h(px(32.))
            .pl(S2)
            .flex()
            .items_center()
            .rounded(RADIUS)
            .bg(p.well)
            .border_1()
            .border_color(if focused { p.border } else { p.well })
            .capture_any_mouse_down(cx.listener(|this, event: &MouseDownEvent, window, cx| {
                if event.button != MouseButton::Left {
                    return;
                }
                this.mode.search();
                crate::platform::mouse_mode(window, false, this.target);
                this.activate(window, cx);
                this.input.update(cx, |s, cx| s.focus(window, cx));
            }))
            .child(icon(IconName::Search, px(16.), p.muted))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .child(Input::new(&self.input).appearance(false)),
            );
        let keep_button = icon_button(
            "keep-open",
            IconName::PictureInPicture2,
            if keep {
                "Keep open on top: on"
            } else {
                "Keep open on top"
            },
            px(32.),
            p,
        )
        .when(keep, |el| el.bg(p.selected).text_color(p.text))
        .on_click(cx.listener(|this, _, window, cx| this.toggle_keep(window, cx)));
        let menu_button = icon_button("menu", IconName::Ellipsis, "View options", px(32.), p)
            .on_click(cx.listener(|this, _, window, cx| {
                let view = cx.entity().downgrade();
                let (compact, keep) = (this.compact(), this.config.keep_open);
                let menu = PopupMenu::build(window, cx, move |menu, _, _| {
                    super::menu::header(menu, view.clone(), compact, keep)
                });
                let width = window.viewport_size().width;
                this.open_menu(
                    menu,
                    point(width - S3, px(48.)),
                    Anchor::TopRight,
                    window,
                    cx,
                );
            }));
        let close = icon_button("close", IconName::X, "Close", px(32.), p)
            .on_click(cx.listener(|_, _, window, _| crate::platform::hide_popup(window)));
        let chips = FILTERS.into_iter().enumerate().map(|(ix, label)| {
            let on = ix == self.filter;
            div()
                .id(("filter", ix))
                .role(Role::Tab)
                .aria_label(label)
                .h(px(24.))
                .px(S2)
                .flex()
                .items_center()
                .gap(S1)
                .rounded_full()
                .border_1()
                .text_size(META)
                .cursor_pointer()
                .when(on, |el| {
                    el.bg(p.chip)
                        .border_color(p.chip)
                        .text_color(p.on_chip)
                        .font_weight(FontWeight::MEDIUM)
                })
                .when(!on, |el| {
                    el.border_color(p.border)
                        .text_color(p.secondary)
                        .hover(|s| s.bg(p.hover).text_color(p.text))
                })
                .when(label == "Pinned", |el| {
                    el.child(icon(
                        IconName::Pin,
                        px(12.),
                        if on { p.on_chip } else { p.secondary },
                    ))
                })
                .child(label)
                .on_click(cx.listener(move |this, _, _, cx| this.set_filter(ix, cx)))
        });
        // Kept open, the empty end of the chip row moves the window.
        let drag = div()
            .id("drag")
            .flex_1()
            .h(px(24.))
            .flex()
            .items_center()
            .justify_end()
            .when(keep, |el| {
                el.window_control_area(WindowControlArea::Drag).child(icon(
                    IconName::GripHorizontal,
                    px(16.),
                    p.muted,
                ))
            });
        div()
            .flex_none()
            .flex()
            .flex_col()
            .gap(S2)
            .px(S3)
            .pt(S3)
            .pb(S2)
            .border_b_1()
            .border_color(p.hairline)
            .child(
                div().flex().items_center().gap(S2).child(search).child(
                    div()
                        .flex()
                        .items_center()
                        .child(keep_button)
                        .child(menu_button)
                        .child(close),
                ),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(S1)
                    .children(chips)
                    .child(drag),
            )
    }

    fn list_body(&self, cx: &mut Context<Self>) -> AnyElement {
        let p = palette(cx);
        if self.clips.is_empty() {
            let searching = !self.input.read(cx).value().is_empty();
            return div()
                .flex_1()
                .flex()
                .flex_col()
                .items_center()
                .justify_center()
                .gap(S2)
                .child(icon(
                    if searching {
                        IconName::SearchX
                    } else {
                        IconName::ClipboardList
                    },
                    px(24.),
                    p.muted,
                ))
                .child(
                    div()
                        .text_size(META)
                        .text_color(p.secondary)
                        .child(if searching {
                            "No clips match your search"
                        } else if self.filter > 0 {
                            "Nothing here yet"
                        } else {
                            "Copy something to start your history"
                        }),
                )
                .into_any_element();
        }
        let entity = cx.entity();
        list(self.list.clone(), move |ix, window, cx| {
            // List items lay out at their content width; give every row the full width.
            div()
                .w_full()
                .px(S1)
                .child(entity.update(cx, |this, cx| this.row(ix, window, cx)))
                .into_any_element()
        })
        .flex_1()
        .w_full()
        .min_h_0()
        .pb(S1)
        .into_any_element()
    }

    fn footer(&self, window: &Window, p: Palette) -> Div {
        // Show as many hints as fit, most important first, spread across the line.
        let mut room = f32::from(window.viewport_size().width) - 24.;
        let mut hints = vec![];
        for (key, label) in self.hints() {
            let width = hint_width(key, label) + if hints.is_empty() { 0. } else { 16. };
            if width > room {
                break;
            }
            room -= width;
            hints.push((*key, *label));
        }
        let spread = hints.len() > 2;
        div()
            .flex_none()
            .h(px(32.))
            .px(S3)
            .flex()
            .items_center()
            .when(spread, |el| el.justify_between())
            .when(!spread, |el| el.gap(S3))
            .overflow_hidden()
            .border_t_1()
            .border_color(p.hairline)
            .text_size(LABEL)
            .text_color(p.muted)
            .children(hints.into_iter().map(|(key, label)| {
                div()
                    .flex_none()
                    .flex()
                    .items_center()
                    .gap(S1)
                    .child(keycap(key, p))
                    .child(label)
            }))
    }
}

impl Render for Popup {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = palette(cx);
        let border = if cx.theme().is_dark() {
            0x4a4a47
        } else {
            0xd6d5d0
        };
        let native = crate::platform::native_border(window, border);
        let content = if matches!(self.view, View::List) {
            div()
                .flex_1()
                .min_h_0()
                .flex()
                .flex_col()
                .child(self.header(window, cx))
                .child(self.list_body(cx))
                .into_any_element()
        } else {
            self.subview(window, cx)
        };
        let skipped = self.store.borrow().skipped;
        let paused = self.paused.load(Ordering::SeqCst);
        div()
            .size_full()
            .flex()
            .flex_col()
            .overflow_hidden()
            .track_focus(&self.focus)
            .when(!native, |el| el.border_1())
            .border_color(p.border)
            .bg(p.surface)
            .text_color(p.text)
            .text_size(super::theme::TITLE)
            .capture_key_down(cx.listener(Self::key))
            // A press on a clip that moves a few pixels becomes a drag into another app.
            .on_mouse_move(cx.listener(|this, event: &MouseMoveEvent, window, cx| {
                let Some((id, file, at)) = this.press else {
                    return;
                };
                if event.pressed_button != Some(MouseButton::Left) {
                    this.press = None;
                } else if (event.position - at).magnitude() > 6. {
                    this.press = None;
                    this.begin_drag(id, file, window, cx);
                }
            }))
            // Clicks focus the root; keep typing and arrow keys going to the search field.
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|this, _, window, cx| {
                    this.press = None;
                    if matches!(this.view, View::List) && this.menu.is_none() {
                        this.input.update(cx, |s, cx| s.focus(window, cx));
                    }
                }),
            )
            .capture_action(
                cx.listener(|this, _: &gpui_component::input::Copy, window, cx| {
                    this.list_action(super::commands::Command::Copy, window, cx);
                }),
            )
            // Bound input actions dispatch before raw key handlers, including Delete.
            .capture_action(
                cx.listener(|this, _: &gpui_component::input::Delete, window, cx| {
                    this.list_action(super::commands::Command::Delete, window, cx);
                }),
            )
            .capture_action(cx.listener(
                |this, _: &gpui_component::input::IndentInline, window, cx| {
                    this.cycle_filter(false, window, cx)
                },
            ))
            .capture_action(cx.listener(
                |this, _: &gpui_component::input::OutdentInline, window, cx| {
                    this.cycle_filter(true, window, cx)
                },
            ))
            .child(content)
            .children(paused.then(|| {
                status_line(
                    IconName::Pause,
                    "Recording paused · resume from the tray".into(),
                    p.secondary,
                    p,
                )
            }))
            .children((skipped > 0).then(|| {
                status_line(
                    IconName::Info,
                    format!(
                        "Skipped {skipped} large {} · raise max_clip_mb in settings",
                        if skipped == 1 { "clip" } else { "clips" }
                    ),
                    p.secondary,
                    p,
                )
            }))
            .children(self.status.as_ref().map(|status| {
                let (name, text, color) = match status {
                    Status::Info(text) => (IconName::Info, text.clone(), p.secondary),
                    Status::Error(text) => (IconName::CircleAlert, text.clone(), p.danger),
                };
                status_line(name, text, color, p)
                    .id("status")
                    .cursor_pointer()
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.status = None;
                        cx.notify();
                    }))
            }))
            .children(
                (matches!(self.view, View::List) && self.selection.len() > 1).then(|| {
                    status_line(
                        IconName::Info,
                        format!(
                            "{} selected · Ctrl-click toggle · Shift-click range",
                            self.selection.len()
                        ),
                        p.secondary,
                        p,
                    )
                }),
            )
            .children(self.updates.borrow().offered().then(|| {
                status_line(
                    IconName::Info,
                    "Update available · view update".into(),
                    p.secondary,
                    p,
                )
                .id("update-available")
                .role(Role::Button)
                .aria_label("Open updates")
                .cursor_pointer()
                .on_click(cx.listener(|this, _, window, cx| this.open_updates(window, cx)))
            }))
            .child(self.footer(window, p))
            .children(self.menu.as_ref().map(|menu| {
                deferred(
                    anchored()
                        .anchor(menu.corner)
                        .position(menu.position)
                        .snap_to_window_with_margin(S2)
                        .child(menu.entity.clone()),
                )
            }))
    }
}
