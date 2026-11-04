use super::commands::Command;
use crate::{
    clip::Clip,
    config::{Config, Density},
    files::Kind,
    platform::{self, Clipboard, Target},
    store::Store,
};
use gpui::*;
use gpui_component::{
    input::{InputEvent, InputState, TextareaState},
    menu::PopupMenu,
};
use std::{
    cell::RefCell,
    rc::Rc,
    sync::{Arc, atomic::AtomicBool},
};

/// Re-registers the global shortcut; errors when another app owns it.
pub type SetHotkey = Rc<dyn Fn(&str) -> Result<(), String>>;

/// App-wide state the popup reads or changes.
pub struct Hooks {
    pub paused: Arc<AtomicBool>,
    pub set_hotkey: SetHotkey,
    pub updates: crate::update::Shared,
    pub update_action: crate::update::SendAction,
}

pub(super) const FILTERS: [&str; 6] = ["All", "Links", "Images", "Text", "Files", "Pinned"];

/// A file clip shows under Files, and under Images too when every file is an image.
pub(super) fn matches(filter: usize, clip: &Clip) -> bool {
    match filter {
        1 => clip.kind == "link",
        2 => {
            clip.kind == "image"
                || (clip.kind == "files"
                    && clip.file_count > 0
                    && clip.file_kinds().iter().all(|k| *k == Kind::Image))
        }
        3 => clip.is_text(),
        4 => clip.kind == "files",
        5 => clip.pinned,
        _ => true,
    }
}

pub(super) enum View {
    List,
    Text {
        id: i64,
        text: Option<String>,
    },
    Image {
        id: i64,
        image: Option<Arc<Image>>,
        caption: String,
    },
    Gallery(Gallery),
    Edit {
        id: i64,
        editor: Entity<TextareaState>,
    },
    Details {
        id: i64,
        rows: Vec<(&'static str, String)>,
    },
    Settings(super::settings::Settings),
    Updates,
}

pub(super) struct Gallery {
    pub id: i64,
    pub files: Vec<GalleryFile>,
    pub selected: usize,
    /// Showing the selected file's large preview instead of the grid.
    pub open: bool,
    /// A one-file clip opens straight into its preview; going back returns to the list.
    pub single: bool,
}

pub(super) struct GalleryFile {
    pub path: String,
    pub kind: Kind,
    /// None until the background check has run.
    pub missing: Option<bool>,
    pub thumb: Option<Arc<Image>>,
    pub large: Option<Arc<Image>>,
    pub caption: Option<String>,
}

pub(super) enum Status {
    Info(String),
    Error(String),
}

pub(super) struct Menu {
    pub entity: Entity<PopupMenu>,
    pub position: Point<Pixels>,
    pub corner: Anchor,
}

pub struct Popup {
    pub(super) view: View,
    pub(super) mode: super::mode::Mode,
    pub(super) store: Rc<RefCell<Store>>,
    pub(super) input: Entity<InputState>,
    pub(super) focus: FocusHandle,
    pub(super) clips: Vec<Clip>,
    pub(super) items: Vec<Option<usize>>,
    pub(super) selection: super::selection::Selection,
    pub(super) filter: usize,
    pub(super) list: ListState,
    pub(super) gallery_scroll: ScrollHandle,
    pub(super) clipboard: Clipboard,
    pub(super) target: Target,
    pub(super) config: Config,
    pub(super) status: Option<Status>,
    pub(super) menu: Option<Menu>,
    pub(super) paused: Arc<AtomicBool>,
    pub(super) set_hotkey: SetHotkey,
    pub(super) updates: crate::update::Shared,
    pub(super) update_action: crate::update::SendAction,
    /// A left press on a clip (and gallery file) that becomes a drag once the mouse moves.
    pub(super) press: Option<(i64, Option<usize>, Point<Pixels>)>,
    _subscriptions: Vec<Subscription>,
}

impl Popup {
    pub fn new(
        store: Rc<RefCell<Store>>,
        clipboard: Clipboard,
        target: Target,
        hooks: Hooks,
        config: Config,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let input = cx.new(|cx| InputState::new(window, cx).placeholder("Search your clipboard"));
        input.update(cx, |s, cx| s.focus(window, cx));
        let change = cx.subscribe_in(&input, window, |this, _, event, _, cx| {
            if matches!(event, InputEvent::Change) {
                this.selection = Default::default();
                this.reload(cx);
            }
        });
        let activation = cx.observe_window_activation(window, |this, window, _| {
            if !this.mode.is_mouse() && !platform::keep() && !window.is_window_active() {
                platform::hide_popup(window);
            }
        });
        let theme = cx.observe_window_appearance(window, |_, window, cx| {
            super::sync_theme(Some(window), cx);
        });
        let mut this = Self {
            view: View::List,
            mode: super::mode::Mode::Keyboard,
            store,
            input,
            focus: cx.focus_handle(),
            clips: vec![],
            items: vec![],
            selection: Default::default(),
            filter: 0,
            list: ListState::new(0, ListAlignment::Top, px(120.)),
            gallery_scroll: ScrollHandle::new(),
            clipboard,
            target,
            config,
            status: None,
            menu: None,
            paused: hooks.paused,
            set_hotkey: hooks.set_hotkey,
            updates: hooks.updates,
            update_action: hooks.update_action,
            press: None,
            _subscriptions: vec![change, activation, theme],
        };
        this.reload(cx);
        this
    }
    pub fn reopen(
        &mut self,
        target: Target,
        mouse: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if mouse && platform::visible(window) {
            platform::hide_popup(window);
            return;
        }
        if platform::visible(window) {
            self.mode.search();
            platform::mouse_mode(window, false, self.target);
            self.activate(window, cx);
            return;
        }
        self.mode = super::mode::Mode::opened(mouse);
        if !target.is_app() {
            self.target = target;
        }
        self.selection = Default::default();
        self.filter = 0;
        if matches!(self.status, Some(Status::Info(_))) {
            self.status = None;
        }
        self.view = View::List;
        self.menu = None;
        self.input.update(cx, |s, cx| {
            s.set_value("", window, cx);
            s.focus(window, cx);
        });
        self.reload(cx);
        platform::configure_popup(window);
        platform::mouse_mode(window, mouse, self.target);
        if !mouse {
            window.activate_window();
        }
        platform::configure_popup(window);
        // GPUI applies its first-open placement in a task queued by activation; place again after it.
        cx.spawn_in(window, async move |_, cx| {
            let _ = cx.update(|window, _| platform::configure_popup(window));
        })
        .detach();
    }
    /// Activates without letting GPUI's pending first placement move the popup.
    pub(super) fn activate(&self, window: &mut Window, cx: &mut Context<Self>) {
        let rect = platform::rect(window);
        window.activate_window();
        if let Some(rect) = rect {
            cx.spawn_in(window, async move |_, cx| {
                let _ = cx.update(|window, _| platform::set_rect(window, rect));
            })
            .detach();
        }
    }
    pub fn reload(&mut self, cx: &mut Context<Self>) {
        let result = self.store.borrow().list(&self.input.read(cx).value());
        match result {
            Ok(clips) => {
                self.clips = clips
                    .into_iter()
                    .filter(|c| matches(self.filter, c))
                    .collect()
            }
            Err(error) => {
                self.status = Some(Status::Error(error.to_string()));
                return;
            }
        }
        self.selection
            .sync(&self.clips.iter().map(|c| c.id).collect::<Vec<_>>());
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
    pub(super) fn set_filter(&mut self, filter: usize, cx: &mut Context<Self>) {
        self.filter = filter % FILTERS.len();
        self.selection = Default::default();
        self.reload(cx);
    }
    pub(super) fn cycle_filter(
        &mut self,
        backwards: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !matches!(self.view, View::List)
            || !self.input.read(cx).focus_handle(cx).is_focused(window)
        {
            return;
        }
        let n = FILTERS.len();
        self.set_filter(self.filter + if backwards { n - 1 } else { 1 }, cx);
        cx.stop_propagation();
    }
    pub(super) fn toggle_density(&mut self, cx: &mut Context<Self>) {
        self.config.density = self.config.density.toggled();
        let value = format!("\"{}\"", self.config.density.name());
        if let Err(error) = Config::save(&[("density", value)]) {
            self.status = Some(Status::Error(format!("Settings not saved: {error}")));
        }
        self.reload(cx);
    }
    pub(super) fn toggle_keep(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let keep = !self.config.keep_open;
        self.config.keep_open = keep;
        platform::set_keep(keep);
        if keep {
            // Remember where it is now, so the next open comes back here.
            platform::save_bounds(window);
        }
        if let Err(error) = Config::save(&[("keep_open", keep.to_string())]) {
            self.status = Some(Status::Error(format!("Settings not saved: {error}")));
        }
        cx.notify();
    }
    pub(super) fn compact(&self) -> bool {
        self.config.density == Density::Compact
    }
    /// Leaves a sub-view for the list (or the gallery grid for an opened file).
    pub(super) fn back(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let View::Gallery(g) = &mut self.view
            && g.open
            && !g.single
        {
            g.open = false;
            self.gallery_scroll.scroll_to_item(g.selected);
            cx.notify();
            return;
        }
        self.view = View::List;
        self.input.update(cx, |s, cx| s.focus(window, cx));
        self.reload(cx);
    }
    pub(super) fn key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.menu.is_some() {
            return;
        }
        let key = event.keystroke.key.as_str();
        let mods = event.keystroke.modifiers;
        let ctrl = mods.control && !mods.alt;
        let handled = match &mut self.view {
            View::List => {
                // A header drag ends outside GPUI, leaving focus on the root.
                if !self.input.read(cx).focus_handle(cx).is_focused(window)
                    && !self.focus.is_focused(window)
                {
                    return;
                }
                self.list_key(key, mods, window, cx)
            }
            View::Edit { .. } => match key {
                "escape" => {
                    self.back(window, cx);
                    true
                }
                "s" if ctrl => {
                    self.save_edit(window, cx);
                    true
                }
                _ => false,
            },
            View::Text { id, .. } | View::Image { id, .. } => {
                let id = *id;
                match key {
                    "escape" | "space" => {
                        self.back(window, cx);
                        true
                    }
                    "enter" => {
                        self.command(id, Command::Paste(mods.shift), window, cx);
                        true
                    }
                    "c" if ctrl => {
                        self.command(id, Command::Copy, window, cx);
                        true
                    }
                    _ => false,
                }
            }
            View::Settings(_) => self.settings_key(event, window, cx),
            View::Details { .. } | View::Updates => {
                if key == "escape" {
                    self.back(window, cx);
                }
                key == "escape"
            }
            View::Gallery(g) => {
                let (id, count, columns) = (g.id, g.files.len(), super::views::columns(window));
                let old = g.selected;
                match key {
                    "escape" => {
                        self.back(window, cx);
                        return cx.stop_propagation();
                    }
                    "space" => g.open = !g.open || g.single,
                    "left" => g.selected = g.selected.saturating_sub(1),
                    "right" => g.selected = (g.selected + 1).min(count.saturating_sub(1)),
                    "up" if !g.open => g.selected = g.selected.saturating_sub(columns),
                    "down" if !g.open => {
                        g.selected = (g.selected + columns).min(count.saturating_sub(1))
                    }
                    "enter" => {
                        let file = g.selected;
                        self.command(id, Command::PasteFile(file), window, cx);
                        return cx.stop_propagation();
                    }
                    _ => return,
                }
                if g.selected != old || g.open {
                    let ix = g.selected;
                    self.gallery_scroll.scroll_to_item(ix);
                    self.load_gallery_detail(ix, cx);
                }
                true
            }
        };
        if handled {
            cx.stop_propagation();
            cx.notify();
        }
    }
    fn list_key(
        &mut self,
        key: &str,
        mods: Modifiers,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let empty = self.input.read(cx).value().is_empty();
        match key {
            "escape" => platform::hide_popup(window),
            "tab" => self.set_filter(self.filter + if mods.shift { 5 } else { 1 }, cx),
            "1" | "2" | "3" | "4" | "5" | "6" if mods.control => {
                self.set_filter(key.parse::<usize>().unwrap() - 1, cx)
            }
            "d" if mods.control && !mods.alt && !mods.shift => self.toggle_density(cx),
            "," if mods.control => self.open_settings(window, cx),
            "up" | "down" => self.selection.navigate(
                &self.clips.iter().map(|c| c.id).collect::<Vec<_>>(),
                key == "down",
                mods.shift,
                mods.control,
            ),
            "enter" => self.selected_command(Command::Paste(mods.shift), window, cx),
            "c" if mods.control && !mods.shift && !mods.alt => {
                self.selected_command(Command::Copy, window, cx)
            }
            "space" if empty => self.selected_command(Command::Preview, window, cx),
            "p" if mods.control => self.selected_command(Command::Pin, window, cx),
            "delete" => self.selected_command(Command::Delete, window, cx),
            "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9"
                if !mods.control && !mods.alt && empty =>
            {
                let ix = key.parse::<usize>().unwrap() - 1;
                if ix < self.clips.len() {
                    self.selection.click(
                        &self.clips.iter().map(|c| c.id).collect::<Vec<_>>(),
                        self.clips[ix].id,
                        false,
                        false,
                    );
                    self.selected_command(Command::Paste(false), window, cx);
                }
            }
            _ => return false,
        }
        if let Some(ix) = self
            .items
            .iter()
            .position(|i| i.is_some_and(|ix| Some(self.clips[ix].id) == self.selection.focus))
        {
            self.list.scroll_to_reveal_item(ix);
        }
        true
    }
    /// Footer hints for the current view, most important first.
    pub(super) fn hints(&self) -> &'static [(&'static str, &'static str)] {
        match &self.view {
            View::List => &[
                ("Enter", "paste"),
                ("Shift+Enter", "plain text"),
                ("Ctrl+C", "copy"),
                ("Ctrl+P", "pin"),
                ("Del", "delete"),
                ("Ctrl+D", "density"),
                ("Ctrl+,", "settings"),
            ],
            View::Text { .. } | View::Image { .. } => {
                &[("Enter", "paste"), ("Ctrl+C", "copy"), ("Esc", "back")]
            }
            View::Gallery(g) if g.open => &[
                ("Enter", "paste this file"),
                ("← →", "next"),
                ("Esc", "back"),
            ],
            View::Gallery(_) => &[
                ("Enter", "paste file"),
                ("Space", "preview"),
                ("Arrows", "move"),
                ("Esc", "back"),
            ],
            View::Edit { .. } => &[("Ctrl+S", "save"), ("Esc", "cancel")],
            View::Details { .. } | View::Updates => &[("Esc", "back")],
            View::Settings(s) if s.recording => &[("Esc", "cancel recording")],
            View::Settings(_) => &[("Esc", "back")],
        }
    }
    pub(super) fn list_action(
        &mut self,
        command: Command,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if matches!(self.view, View::List)
            && self.menu.is_none()
            && (self.input.read(cx).focus_handle(cx).is_focused(window)
                || self.focus.is_focused(window))
        {
            self.selected_command(command, window, cx);
            cx.stop_propagation();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Clip, FILTERS, matches};
    #[test]
    fn file_clips_filter_under_files_and_all_image_sets_under_images() {
        let mut images = Clip::text("C:/a.png\nC:/b.jpg".into(), "Explorer".into());
        images.kind = "files".into();
        images.file_count = 2;
        let mut mixed = images.clone();
        mixed.text = "C:/a.png\nC:/b.pdf".into();
        let mut folder = images.clone();
        folder.file_kinds = "fi".into();
        let mut shot = Clip::text("1×1".into(), "Explorer".into());
        shot.kind = "image".into();
        let link = Clip::text("https://example.com".into(), "Chrome".into());
        let at = |f| {
            [&images, &mixed, &folder, &shot, &link]
                .map(|c| matches(f, c))
                .to_vec()
        };
        assert_eq!(at(2), [true, false, false, true, false]);
        assert_eq!(at(4), [true, true, true, false, false]);
        assert_eq!(at(1), [false, false, false, false, true]);
        assert_eq!(at(3), [false, false, false, false, true]);
        assert_eq!(FILTERS[4], "Files");
    }
}
