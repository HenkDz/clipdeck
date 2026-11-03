use super::Popup;
use gpui::*;
use gpui_component::{
    ActiveTheme,
    input::{Input, Textarea},
};
use std::sync::atomic::Ordering;
impl Render for Popup {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let entity = cx.entity();
        let content = if let Some((_, editor)) = &self.editor {
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
            .rounded_xl()
            .border_1()
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
            .child(div().px_3().pt_2().pb_2().child(Input::new(&self.input)))
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
    }
}
