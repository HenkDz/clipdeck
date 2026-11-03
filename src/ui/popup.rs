use super::commands::Command;
use crate::{
    clip::Clip,
    platform::{self, Clipboard, Target},
    store::Store,
};
use gpui::*;
use gpui_component::{
    ActiveTheme,
    input::{InputEvent, InputState, TextareaState},
    kbd::Kbd,
};
use std::{
    cell::RefCell,
    rc::Rc,
    sync::{Arc, atomic::AtomicBool},
};
pub struct Popup {
    pub(super) store: Rc<RefCell<Store>>,
    pub(super) input: Entity<InputState>,
    pub(super) clips: Vec<Clip>,
    pub(super) items: Vec<Option<usize>>,
    pub(super) selected: usize,
    pub(super) filter: usize,
    pub(super) list: ListState,
    pub(super) clipboard: Clipboard,
    pub(super) target: Target,
    pub(super) delay: u64,
    pub(super) error: Option<String>,
    pub(super) editor: Option<(i64, Entity<TextareaState>)>,
    pub(super) details: Option<String>,
    pub(super) paused: Arc<AtomicBool>,
    _subscriptions: Vec<Subscription>,
}
impl Popup {
    pub fn new(
        store: Rc<RefCell<Store>>,
        clipboard: Clipboard,
        target: Target,
        paused: Arc<AtomicBool>,
        delay: u64,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let input = cx.new(|cx| InputState::new(window, cx).placeholder("Search your clipboard"));
        input.update(cx, |s, cx| s.focus(window, cx));
        let change = cx.subscribe_in(&input, window, |this, _, event, _, cx| {
            if matches!(event, InputEvent::Change) {
                this.selected = 0;
                this.reload(cx);
            }
        });
        let activation = cx.observe_window_activation(window, |_, window, _| {
            if !window.is_window_active() {
                platform::hide_popup(window);
            }
        });
        let theme = cx.observe_window_appearance(window, |_, window, cx| {
            super::sync_theme(Some(window), cx);
        });
        let mut this = Self {
            store,
            input,
            clips: vec![],
            items: vec![],
            selected: 0,
            filter: 0,
            list: ListState::new(0, ListAlignment::Top, px(120.)),
            clipboard,
            target,
            delay,
            error: None,
            editor: None,
            details: None,
            paused,
            _subscriptions: vec![change, activation, theme],
        };
        this.reload(cx);
        this
    }
    pub fn reopen(&mut self, target: Target, window: &mut Window, cx: &mut Context<Self>) {
        if !target.is_app() {
            self.target = target;
        }
        self.selected = 0;
        self.filter = 0;
        self.error = None;
        self.editor = None;
        self.details = None;
        self.input.update(cx, |s, cx| {
            s.set_value("", window, cx);
            s.focus(window, cx);
        });
        self.reload(cx);
        platform::configure_popup(window);
        window.activate_window();
    }
    pub fn reload(&mut self, cx: &mut Context<Self>) {
        let result = self.store.borrow().list(&self.input.read(cx).value());
        match result {
            Ok(clips) => {
                self.clips = clips
                    .into_iter()
                    .filter(|c| match self.filter {
                        1 => c.kind == "link",
                        2 => c.kind == "image",
                        3 => c.is_text(),
                        4 => c.pinned,
                        _ => true,
                    })
                    .collect()
            }
            Err(error) => {
                self.error = Some(error.to_string());
                return;
            }
        }
        self.selected = self.selected.min(self.clips.len().saturating_sub(1));
        self.items.clear();
        let mut section = "";
        for (ix, clip) in self.clips.iter().enumerate() {
            if clip.section() != section {
                section = clip.section();
                self.items.push(None);
            }
            self.items.push(Some(ix));
        }
        self.list.reset(self.items.len());
        cx.notify();
    }
    pub(super) fn cycle_filter(
        &mut self,
        backwards: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.editor.is_some()
            || self.details.is_some()
            || !self.input.read(cx).focus_handle(cx).is_focused(window)
        {
            return;
        }
        self.filter = (self.filter + if backwards { 4 } else { 1 }) % 5;
        self.selected = 0;
        self.reload(cx);
        cx.stop_propagation();
    }
    pub(super) fn key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let key = event.keystroke.key.as_str();
        let mods = &event.keystroke.modifiers;
        if self.editor.is_none()
            && self.details.is_none()
            && !self.input.read(cx).focus_handle(cx).is_focused(window)
        {
            return;
        }
        if self.editor.is_some() {
            if key == "escape" {
                self.editor = None;
                self.input.update(cx, |s, cx| s.focus(window, cx));
            } else if mods.control && key == "s" {
                self.save_edit(window, cx);
            } else {
                return;
            }
        } else if self.details.is_some() {
            if key == "escape" {
                self.details = None;
            } else {
                return;
            }
        } else {
            match key {
                "escape" => platform::hide_popup(window),
                "tab" => {
                    self.filter = (self.filter + if mods.shift { 4 } else { 1 }) % 5;
                    self.selected = 0;
                    self.reload(cx);
                }
                "1" | "2" | "3" | "4" | "5" if mods.control => {
                    self.filter = key.parse::<usize>().unwrap() - 1;
                    self.selected = 0;
                    self.reload(cx);
                }
                "up" => self.selected = self.selected.saturating_sub(1),
                "down" => {
                    self.selected = (self.selected + 1).min(self.clips.len().saturating_sub(1))
                }
                "enter" => self.selected_command(Command::Paste(mods.shift), window, cx),
                "p" if mods.control => self.selected_command(Command::Pin, window, cx),
                "delete" => self.selected_command(Command::Delete, window, cx),
                "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9"
                    if !mods.control && !mods.alt && self.input.read(cx).value().is_empty() =>
                {
                    let ix = key.parse::<usize>().unwrap() - 1;
                    if ix < self.clips.len() {
                        self.selected = ix;
                        self.selected_command(Command::Paste(false), window, cx);
                    }
                }
                _ => return,
            }
            if let Some(ix) = self.items.iter().position(|i| *i == Some(self.selected)) {
                self.list.scroll_to_reveal_item(ix);
            }
        }
        cx.stop_propagation();
        cx.notify();
    }
    pub(super) fn footer(cx: &App) -> Div {
        div()
            .flex()
            .gap_3()
            .px_3()
            .py_2()
            .border_t_1()
            .border_color(cx.theme().border)
            .text_color(cx.theme().muted_foreground)
            .text_xs()
            .children(
                [
                    ("enter", "paste"),
                    ("shift-enter", "plain text"),
                    ("ctrl-p", "pin"),
                    ("delete", "delete"),
                ]
                .into_iter()
                .map(|(key, label)| {
                    div()
                        .flex()
                        .gap_1()
                        .child(Kbd::new(Keystroke::parse(key).unwrap()))
                        .child(label)
                }),
            )
    }
}
