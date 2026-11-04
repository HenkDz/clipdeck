//! The Settings view: every config.toml setting as a control, applied immediately.
use super::{
    Popup,
    popup::{Status, View},
    theme::{LABEL, META, Palette, RADIUS, S1, S2, S3, S4, TITLE, palette},
    widgets::{button, icon},
};
use crate::config::{Config, Density};
use gpui::{prelude::FluentBuilder, *};
use gpui_component::switch::Switch;
use gpui_kit_assets::IconName;

const HISTORY: [usize; 10] = [
    100, 250, 500, 1000, 2500, 5000, 10_000, 25_000, 50_000, 100_000,
];
const CLIP_MB: [usize; 8] = [1, 2, 4, 8, 16, 32, 64, 128];
const DELAY_MS: [usize; 11] = [0, 20, 40, 60, 80, 120, 160, 250, 400, 600, 1000];

#[derive(Default)]
pub(super) struct Settings {
    /// Waiting for the next key chord to become the shortcut.
    pub recording: bool,
    /// A smaller history size waiting for confirmation, since it deletes clips.
    pub confirm: Option<usize>,
}

/// The next preset above or below `value`; values between presets snap to them.
pub(super) fn step(presets: &[usize], value: usize, up: bool) -> usize {
    if up {
        presets
            .iter()
            .copied()
            .find(|p| *p > value)
            .unwrap_or(value)
    } else {
        presets
            .iter()
            .rev()
            .copied()
            .find(|p| *p < value)
            .unwrap_or(value)
    }
}

/// A recorded key chord as a global shortcut, e.g. "Ctrl+Shift+V".
/// Needs Ctrl, Alt or Win so ordinary typing is never captured.
pub(super) fn shortcut(mods: Modifiers, key: &str) -> Option<String> {
    if !(mods.control || mods.alt || mods.platform)
        || key.is_empty()
        || matches!(key, "control" | "alt" | "shift" | "platform" | "function")
    {
        return None;
    }
    let mut parts = vec![];
    for (on, name) in [
        (mods.control, "Ctrl"),
        (mods.alt, "Alt"),
        (mods.shift, "Shift"),
        (mods.platform, "Super"),
    ] {
        if on {
            parts.push(name.to_owned());
        }
    }
    let mut chars = key.chars();
    let first = chars.next()?;
    parts.push(first.to_uppercase().chain(chars).collect());
    let text = parts.join("+");
    text.parse::<global_hotkey::hotkey::HotKey>()
        .is_ok()
        .then_some(text)
}

/// How a stored shortcut reads on Windows.
fn display(hotkey: &str) -> String {
    hotkey.replace("Super", "Win")
}

fn section(title: &'static str, p: Palette) -> Div {
    div()
        .pt(S3)
        .pb(S1)
        .text_size(LABEL)
        .font_weight(FontWeight::MEDIUM)
        .text_color(p.muted)
        .child(title)
}

fn row(
    label: &'static str,
    caption: impl Into<SharedString>,
    control: impl IntoElement,
    p: Palette,
) -> Div {
    div()
        .flex()
        .items_center()
        .gap(S3)
        .min_h(px(44.))
        .py(S1)
        .child(
            div()
                .flex_1()
                .min_w_0()
                .flex()
                .flex_col()
                .child(div().text_size(TITLE).line_height(px(18.)).child(label))
                .child(
                    div()
                        .text_size(META)
                        .line_height(px(16.))
                        .text_color(p.muted)
                        .child(caption.into()),
                ),
        )
        .child(div().flex_none().child(control))
}

impl Popup {
    pub fn open_settings(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.menu = None;
        self.view = View::Settings(Settings::default());
        self.focus.focus(window, cx);
        cx.notify();
    }

    fn save(&mut self, keys: &[(&str, String)]) {
        if let Err(error) = Config::save(keys) {
            self.status = Some(Status::Error(format!("Settings not saved: {error}")));
        }
    }

    fn set_history(&mut self, size: usize, cx: &mut Context<Self>) {
        let result = self.store.borrow_mut().set_limit(size);
        match result {
            Ok(()) => {
                self.config.history_size = size;
                self.save(&[("history_size", size.to_string())]);
            }
            Err(error) => self.status = Some(Status::Error(error.to_string())),
        }
        if let View::Settings(s) = &mut self.view {
            s.confirm = None;
        }
        self.reload(cx);
    }

    fn change_history(&mut self, up: bool, cx: &mut Context<Self>) {
        let size = step(&HISTORY, self.config.history_size, up);
        let kept = self.store.borrow().unpinned().unwrap_or(0);
        if !up && kept > size {
            // Fewer clips than are stored deletes some; ask first.
            if let View::Settings(s) = &mut self.view {
                s.confirm = Some(size);
            }
            cx.notify();
        } else {
            self.set_history(size, cx);
        }
    }

    /// Keys while Settings is shown; returns whether the key was used.
    pub(super) fn settings_key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let View::Settings(s) = &mut self.view else {
            return false;
        };
        let key = event.keystroke.key.as_str();
        if !s.recording {
            if key == "escape" {
                self.back(window, cx);
                return true;
            }
            return false;
        }
        if key == "escape" && !event.keystroke.modifiers.modified() {
            s.recording = false;
            return true;
        }
        let Some(hotkey) = shortcut(event.keystroke.modifiers, key) else {
            // Bare modifiers while the chord is being held; anything else is not a shortcut.
            if !matches!(key, "control" | "alt" | "shift" | "platform" | "function") {
                self.status = Some(Status::Info("Use Ctrl, Alt or Win with another key".into()));
            }
            return true;
        };
        s.recording = false;
        match (self.set_hotkey)(&hotkey) {
            Ok(()) => {
                self.status = None;
                self.config.hotkey = hotkey.clone();
                self.save(&[("hotkey", format!("\"{hotkey}\""))]);
            }
            Err(error) => {
                self.status = Some(Status::Error(format!(
                    "{} is not available: {error}",
                    display(&hotkey)
                )))
            }
        }
        true
    }

    fn stepper(
        &self,
        id: &'static str,
        value: String,
        can_down: bool,
        can_up: bool,
        on_step: impl Fn(&mut Self, bool, &mut Window, &mut Context<Self>) + Clone + 'static,
        cx: &mut Context<Self>,
    ) -> Div {
        let p = palette(cx);
        let side = |up: bool, enabled: bool| {
            let on_step = on_step.clone();
            div()
                .id((id, up as usize))
                .role(Role::Button)
                .aria_label(if up { "Increase" } else { "Decrease" })
                .size(px(28.))
                .flex()
                .items_center()
                .justify_center()
                .text_color(if enabled { p.secondary } else { p.border })
                .when(enabled, |el| {
                    el.cursor_pointer()
                        .hover(move |s| s.bg(p.hover).text_color(p.text))
                        .on_click(cx.listener(move |this, _, window, cx| {
                            on_step(this, up, window, cx);
                            cx.notify();
                        }))
                })
                .child(
                    gpui_component::Icon::new(if up { IconName::Plus } else { IconName::Minus })
                        .size(px(14.)),
                )
        };
        div()
            .h(px(30.))
            .flex()
            .items_center()
            .rounded(px(6.))
            .border_1()
            .border_color(p.border)
            .overflow_hidden()
            .child(side(false, can_down))
            .child(
                div()
                    .w(px(72.))
                    .h_full()
                    .flex()
                    .items_center()
                    .justify_center()
                    .border_l_1()
                    .border_r_1()
                    .border_color(p.hairline)
                    .text_size(META)
                    .font_weight(FontWeight::MEDIUM)
                    .child(value),
            )
            .child(side(true, can_up))
    }

    pub(super) fn settings_view(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let p = palette(cx);
        let View::Settings(state) = &self.view else {
            return div().into_any_element();
        };
        let (recording, confirm) = (state.recording, state.confirm);
        let config = self.config.clone();

        let recorder = div()
            .id("shortcut")
            .role(Role::Button)
            .aria_label("Change shortcut")
            .h(px(30.))
            .min_w(px(144.))
            .px(S3)
            .flex()
            .items_center()
            .justify_center()
            .gap(S2)
            .rounded(px(6.))
            .border_1()
            .border_color(if recording { p.accent } else { p.border })
            .bg(if recording { p.well } else { p.surface })
            .text_size(META)
            .font_weight(FontWeight::MEDIUM)
            .cursor_pointer()
            .hover(move |s| s.bg(p.hover))
            .child(icon(IconName::Keyboard, px(14.), p.muted))
            .child(if recording {
                "Press a shortcut…".to_owned()
            } else {
                display(&config.hotkey)
            })
            .on_click(cx.listener(|this, _, window, cx| {
                if let View::Settings(s) = &mut this.view {
                    s.recording = !s.recording;
                }
                this.focus.focus(window, cx);
                cx.notify();
            }));

        let density = div()
            .flex()
            .items_center()
            .p(px(2.))
            .gap(px(2.))
            .rounded(px(8.))
            .bg(p.well)
            .children(
                [
                    ("Comfortable", Density::Comfortable),
                    ("Compact", Density::Compact),
                ]
                .map(|(label, value)| {
                    let on = config.density == value;
                    div()
                        .id(label)
                        .role(Role::Tab)
                        .aria_label(label)
                        .h(px(26.))
                        .px(S3)
                        .flex()
                        .items_center()
                        .rounded(px(6.))
                        .text_size(META)
                        .cursor_pointer()
                        .when(on, |el| {
                            el.bg(p.surface)
                                .text_color(p.text)
                                .font_weight(FontWeight::MEDIUM)
                                .shadow_xs()
                        })
                        .when(!on, |el| {
                            el.text_color(p.secondary)
                                .hover(move |s| s.text_color(p.text))
                        })
                        .child(label)
                        .on_click(cx.listener(move |this, _, _, cx| {
                            if this.config.density != value {
                                this.toggle_density(cx);
                            }
                        }))
                }),
            );

        let keep = Switch::new("keep-open-switch")
            .checked(config.keep_open)
            .on_click(cx.listener(|this, checked: &bool, window, cx| {
                if this.config.keep_open != *checked {
                    this.toggle_keep(window, cx);
                }
            }));

        let history = self.stepper(
            "history",
            format_count(config.history_size),
            config.history_size > HISTORY[0],
            config.history_size < HISTORY[HISTORY.len() - 1],
            |this, up, _, cx| this.change_history(up, cx),
            cx,
        );
        let largest = self.stepper(
            "largest",
            format!("{} MB", config.max_clip_mb),
            config.max_clip_mb > CLIP_MB[0],
            config.max_clip_mb < CLIP_MB[CLIP_MB.len() - 1],
            |this, up, _, _| {
                let mb = step(&CLIP_MB, this.config.max_clip_mb, up);
                this.config.max_clip_mb = mb;
                this.store.borrow_mut().set_max_clip_mb(mb);
                this.save(&[("max_clip_mb", mb.to_string())]);
            },
            cx,
        );
        let delay = self.stepper(
            "delay",
            format!("{} ms", config.paste_delay_ms),
            config.paste_delay_ms > 0,
            (config.paste_delay_ms as usize) < DELAY_MS[DELAY_MS.len() - 1],
            |this, up, _, _| {
                let ms = step(&DELAY_MS, this.config.paste_delay_ms as usize, up);
                this.config.paste_delay_ms = ms as u64;
                this.save(&[("paste_delay_ms", ms.to_string())]);
            },
            cx,
        );

        let kept = self.store.borrow().unpinned().unwrap_or(0);
        let confirm_strip = confirm.map(|size| {
            div()
                .mb(S2)
                .p(S3)
                .flex()
                .items_center()
                .gap(S3)
                .rounded(RADIUS)
                .bg(p.well)
                .border_1()
                .border_color(p.hairline)
                .child(icon(IconName::TriangleAlert, px(16.), p.danger))
                .child(div().flex_1().min_w_0().text_size(META).child(format!(
                    "Keeping {} removes the {} oldest unpinned clips.",
                    format_count(size),
                    format_count(kept.saturating_sub(size))
                )))
                .child(
                    button("keep-history", None, "Cancel", false, p).on_click(cx.listener(
                        |this, _, _, cx| {
                            if let View::Settings(s) = &mut this.view {
                                s.confirm = None;
                            }
                            cx.notify();
                        },
                    )),
                )
                .child(
                    button("trim-history", None, "Remove", true, p)
                        .on_click(cx.listener(move |this, _, _, cx| this.set_history(size, cx))),
                )
        });

        let body = div()
            .id("settings")
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .px(S4)
            .pb(S3)
            .flex()
            .flex_col()
            .child(section("Shortcut", p))
            .child(row(
                "Open Clipdeck",
                if recording {
                    "Press the new keys, or Esc to cancel"
                } else {
                    "Works from any app"
                },
                recorder,
                p,
            ))
            .child(div().h(px(1.)).bg(p.hairline).mt(S1))
            .child(section("Window", p))
            .child(row("List density", "Ctrl+D switches it too", density, p))
            .child(row(
                "Keep open on top",
                "Stays visible after pasting; drag it by the chip row",
                keep,
                p,
            ))
            .child(div().h(px(1.)).bg(p.hairline).mt(S1))
            .child(section("History", p))
            .child(row(
                "Clips to keep",
                format!(
                    "{} stored now · pinned clips are always kept",
                    format_count(kept)
                ),
                history,
                p,
            ))
            .children(confirm_strip)
            .child(row("Largest clip", "Bigger copies are skipped", largest, p))
            .child(div().h(px(1.)).bg(p.hairline).mt(S1))
            .child(section("Pasting", p))
            .child(row(
                "Paste delay",
                "Raise it if pastes land before the app is ready",
                delay,
                p,
            ))
            .child(div().h(px(1.)).bg(p.hairline).mt(S1))
            .child(self.updates_view(cx));
        div()
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            .child(self.bar("Settings".into(), None, vec![], cx))
            .child(body)
            .into_any_element()
    }
}

fn format_count(n: usize) -> String {
    let digits = n.to_string();
    let mut out = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::{CLIP_MB, DELAY_MS, HISTORY, format_count, shortcut, step};
    use gpui::Modifiers;
    #[test]
    fn steppers_move_between_presets_and_snap_odd_values() {
        assert_eq!(step(&HISTORY, 1000, true), 2500);
        assert_eq!(step(&HISTORY, 1000, false), 500);
        assert_eq!(step(&HISTORY, 1200, false), 1000);
        assert_eq!(step(&HISTORY, 1200, true), 2500);
        assert_eq!(step(&HISTORY, 100_000, true), 100_000);
        assert_eq!(step(&CLIP_MB, 1, false), 1);
        assert_eq!(step(&DELAY_MS, 80, true), 120);
        assert_eq!(step(&DELAY_MS, 0, false), 0);
        assert_eq!(format_count(25_000), "25,000");
        assert_eq!(format_count(100), "100");
    }
    #[test]
    fn recorded_chords_become_valid_shortcuts() {
        let m = |control, alt, shift, platform| Modifiers {
            control,
            alt,
            shift,
            platform,
            ..Default::default()
        };
        assert_eq!(
            shortcut(m(true, false, true, false), "v").as_deref(),
            Some("Ctrl+Shift+V")
        );
        assert_eq!(
            shortcut(m(false, true, false, false), "space").as_deref(),
            Some("Alt+Space")
        );
        assert_eq!(
            shortcut(m(true, true, false, false), "f5").as_deref(),
            Some("Ctrl+Alt+F5")
        );
        assert_eq!(
            shortcut(m(false, false, false, true), "c").as_deref(),
            Some("Super+C")
        );
        assert_eq!(shortcut(m(false, false, true, false), "v"), None);
        assert_eq!(shortcut(m(true, false, false, false), "control"), None);
        "Ctrl+Shift+V"
            .parse::<global_hotkey::hotkey::HotKey>()
            .unwrap();
    }
}
