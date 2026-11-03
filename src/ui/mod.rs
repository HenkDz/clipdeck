use crate::{
    history::History,
    platform::{self, Clipboard, Target},
};
use gpui::*;
use gpui_component::{
    ActiveTheme,
    input::{Input, InputEvent, InputState},
};
use std::{cell::RefCell, rc::Rc};

pub struct Popup {
    history: Rc<RefCell<History>>,
    input: Entity<InputState>,
    selected: usize,
    scroll: ScrollHandle,
    clipboard: Clipboard,
    target: Target,
    error: Option<String>,
    _subscriptions: Vec<Subscription>,
}

impl Popup {
    pub fn reopen(&mut self, target: Target, window: &mut Window, cx: &mut Context<Self>) {
        self.target = target;
        self.selected = 0;
        self.error = None;
        self.input.update(cx, |input, cx| {
            input.set_value("", window, cx);
            input.focus(window, cx);
        });
        platform::configure_popup(window);
        window.activate_window();
        cx.notify();
    }
    pub fn new(
        history: Rc<RefCell<History>>,
        clipboard: Clipboard,
        target: Target,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let input = cx.new(|cx| InputState::new(window, cx).placeholder("Search clipboard…"));
        input.update(cx, |input, cx| input.focus(window, cx));
        let change = cx.subscribe_in(&input, window, |this, _, event, _, cx| {
            if matches!(event, InputEvent::Change) {
                this.selected = 0;
                cx.notify();
            }
        });
        let activation = cx.observe_window_activation(window, |_, window, _| {
            eprintln!("Popup active: {}", window.is_window_active());
            if !window.is_window_active() {
                platform::hide_popup(window);
            }
        });
        Self {
            history,
            input,
            selected: 0,
            scroll: ScrollHandle::new(),
            clipboard,
            target,
            error: None,
            _subscriptions: vec![change, activation],
        }
    }

    fn filtered(&self, cx: &App) -> Vec<String> {
        self.history.borrow().filter(&self.input.read(cx).value())
    }

    fn paste(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let clips = self.filtered(cx);
        let Some(text) = clips.get(self.selected.min(clips.len().saturating_sub(1))) else {
            return;
        };
        if let Err(error) = self.clipboard.write(text) {
            self.error = Some(error.to_string());
            cx.notify();
            return;
        }
        platform::hide_popup(window);
        let target = self.target;
        cx.spawn(async move |_, cx| {
            cx.background_executor()
                .timer(std::time::Duration::from_millis(80))
                .await;
            if let Err(error) = target.paste() {
                eprintln!("Paste: {error}");
            }
        })
        .detach();
    }
}

impl Render for Popup {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // shortcut: eager text rows suffice for the spike; virtualize with milestone 2 retention.
        let clips = self.filtered(cx);
        self.selected = self.selected.min(clips.len().saturating_sub(1));
        let selected = self.selected;
        div()
            .size_full()
            .flex()
            .flex_col()
            .p_3()
            .gap_2()
            .bg(cx.theme().background)
            .text_color(cx.theme().foreground)
            .capture_key_down(cx.listener(|this, event: &KeyDownEvent, window, cx| {
                let count = this.filtered(cx).len();
                match event.keystroke.key.as_str() {
                    "escape" => platform::hide_popup(window),
                    "up" => this.selected = this.selected.saturating_sub(1),
                    "down" => this.selected = (this.selected + 1).min(count.saturating_sub(1)),
                    "enter" => this.paste(window, cx),
                    _ => return,
                }
                cx.stop_propagation();
                this.scroll.scroll_to_item(this.selected);
                cx.notify();
            }))
            .child(div().text_sm().child("CLIPDECK · clipboard history"))
            .child(Input::new(&self.input))
            .child(
                div()
                    .id("clips")
                    .flex_1()
                    .overflow_y_scroll()
                    .track_scroll(&self.scroll)
                    .children(clips.iter().enumerate().map(|(ix, text)| {
                        div()
                            .id(ix)
                            .p_2()
                            .rounded_md()
                            .bg(if ix == selected {
                                cx.theme().accent
                            } else {
                                cx.theme().background
                            })
                            .child(
                                text.lines()
                                    .next()
                                    .unwrap_or("")
                                    .chars()
                                    .take(100)
                                    .collect::<String>(),
                            )
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.selected = ix;
                                this.paste(window, cx);
                            }))
                    })),
            )
            .children(self.error.clone().map(|error| div().text_sm().child(error)))
            .child(div().text_sm().child(format!(
                "{} clips · ↑↓ select · Enter paste · Esc close",
                clips.len()
            )))
    }
}
