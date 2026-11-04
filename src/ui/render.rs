use super::Popup;
use gpui::prelude::FluentBuilder;
use gpui::*;
use gpui_component::{
    ActiveTheme,
    button::{Button, ButtonVariants},
    input::{Input, Textarea},
};
use std::sync::atomic::Ordering;
impl Render for Popup {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let border = if cx.theme().is_dark() {
            0x454542
        } else {
            0xe5e4e0
        };
        let native = crate::platform::native_border(window, border);
        let entity = cx.entity();
        let content = if let Some(preview) = &self.preview {
            div()
                .id("preview")
                .flex_1()
                .min_h_0()
                .p_3()
                .flex()
                .flex_col()
                .gap_2()
                .child(
                    div()
                        .id("preview-text")
                        .overflow_y_scroll()
                        .flex_1()
                        .min_h_0()
                        .text_sm()
                        .child(preview.clone()),
                )
                .child(
                    div()
                        .id("close-preview")
                        .cursor_pointer()
                        .child("Close preview · Space / Esc")
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.preview = None;
                            this.preview_id = None;
                            cx.notify();
                        })),
                )
                .into_any_element()
        } else if let Some((_, editor)) = &self.editor {
            div()
                .flex_1()
                .min_h_0()
                .p_3()
                .flex()
                .flex_col()
                .gap_2()
                .child("Edit clip")
                .child(Textarea::new(editor).h_full())
                .child(
                    div()
                        .id("save")
                        .p_2()
                        .cursor_pointer()
                        .child("Save · Ctrl+S")
                        .on_click(cx.listener(|this, _, window, cx| this.save_edit(window, cx))),
                )
                .into_any_element()
        } else if let Some(details) = &self.details {
            div()
                .flex_1()
                .p_3()
                .text_sm()
                .child(details.clone())
                .child(div().child("Esc to return"))
                .into_any_element()
        } else if self.clips.is_empty() {
            div()
                .flex_1()
                .p_4()
                .text_color(cx.theme().muted_foreground)
                .child("No clips found")
                .into_any_element()
        } else {
            list(self.list.clone(), move |ix, window, cx| {
                entity.update(cx, |this, cx| this.row(ix, window, cx))
            })
            .flex_1()
            .w_full()
            .min_h_0()
            .into_any_element()
        };
        div()
            .size_full()
            .flex()
            .flex_col()
            .overflow_hidden()
            .when(!native, |el| el.border_1())
            .border_color(cx.theme().border)
            .bg(cx.theme().background)
            .text_color(cx.theme().foreground)
            .capture_key_down(cx.listener(Self::key))
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
            .child(
                div()
                    .flex()
                    .px_3()
                    .pt_2()
                    .pb_2()
                    .child(
                        div()
                            .flex_1()
                            .capture_any_mouse_down(cx.listener(
                                |this, event: &MouseDownEvent, window, cx| {
                                    if event.button != MouseButton::Left {
                                        return;
                                    }
                                    this.mode.search();
                                    crate::platform::mouse_mode(window, false, this.target);
                                    window.activate_window();
                                    this.input.update(cx, |s, cx| s.focus(window, cx));
                                },
                            ))
                            .child(Input::new(&self.input)),
                    )
                    .child(
                        Button::new("close")
                            .ghost()
                            .compact()
                            .label("×")
                            .accessibility_label("Close popup")
                            .on_click(
                                cx.listener(|_, _, window, _| crate::platform::hide_popup(window)),
                            ),
                    ),
            )
            .child(
                div().flex().gap_1().px_3().pb_2().children(
                    ["All", "Links", "Images", "Text", "Pinned"]
                        .into_iter()
                        .enumerate()
                        .map(|(ix, label)| {
                            div()
                                .id(("filter", ix))
                                .px_2()
                                .py_1()
                                .rounded_full()
                                .text_xs()
                                .border_1()
                                .border_color(cx.theme().border)
                                .bg(if ix == self.filter {
                                    cx.theme().foreground
                                } else {
                                    cx.theme().background
                                })
                                .text_color(if ix == self.filter {
                                    cx.theme().background
                                } else {
                                    cx.theme().muted_foreground
                                })
                                .cursor_pointer()
                                .child(label)
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.filter = ix;
                                    this.selected = 0;
                                    this.reload(cx);
                                }))
                        }),
                ),
            )
            .children(self.paused.load(Ordering::SeqCst).then(|| {
                div()
                    .px_3()
                    .py_1()
                    .bg(cx.theme().accent)
                    .text_sm()
                    .child("Recording paused · resume from the tray")
            }))
            .child(content)
            .children(self.error.clone().map(|e| {
                div()
                    .px_3()
                    .text_xs()
                    .text_color(cx.theme().danger)
                    .child(e)
            }))
            .child(Self::footer(cx))
            .children((self.store.borrow().skipped > 0).then(|| {
                div().px_3().pb_2().text_xs().child(format!(
                    "Skipped {} large clips",
                    self.store.borrow().skipped
                ))
            }))
            // Reserve horizontal room for the second menu inside the 480px popup.
            .children(self.context_menu.as_ref().map(|(menu, y)| {
                deferred(
                    anchored()
                        .position(point(px(8.), *y))
                        .snap_to_window_with_margin(px(8.))
                        .child(menu.clone()),
                )
            }))
    }
}
