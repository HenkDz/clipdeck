use super::{
    Popup,
    theme::{META, S1, S2, S3, palette},
    widgets::button,
};
use crate::update::{Action, State};
use gpui::*;
use gpui_component::switch::Switch;

impl Popup {
    pub fn open_updates(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.menu = None;
        self.view = super::popup::View::Updates;
        self.focus.focus(window, cx);
        cx.notify();
    }
    pub(super) fn updates_view(&self, cx: &mut Context<Self>) -> Div {
        let p = palette(cx);
        let state = self.updates.borrow().clone();
        let (message, action) = match &state {
            State::Idle => (
                "Check for a signed update from GitHub".to_owned(),
                Some(("Check for updates", Action::Check(true))),
            ),
            State::Checking => ("Checking GitHub…".into(), None),
            State::Current => (
                "You have the latest version".into(),
                Some(("Check again", Action::Check(true))),
            ),
            State::Available(release) => (
                format!("Clipdeck {} is available", release.manifest.version),
                Some(("Download update", Action::Download)),
            ),
            State::Downloading(done, total) => (
                format!(
                    "Downloading update · {}%",
                    done.saturating_mul(100) / total.max(&1)
                ),
                None,
            ),
            State::Ready(_) => (
                "Download verified. Ready to update.".into(),
                Some(("Update and restart", Action::Install)),
            ),
            State::Installing | State::Restarting => {
                ("Preparing to close Clipdeck and install…".into(), None)
            }
            State::Message(message) | State::Error(message) => (
                message.clone(),
                Some(("Check for updates", Action::Check(true))),
            ),
        };
        let notes = match &state {
            State::Available(release) => Some(release.manifest.notes.clone()),
            _ => None,
        };
        let offered = state.offered();
        let switch = Switch::new("check-updates-switch")
            .label("Automatically check daily")
            .checked(self.config.check_updates)
            .on_click(cx.listener(|this, checked: &bool, _, cx| {
                match crate::config::Config::save(&[("check_updates", checked.to_string())]) {
                    Ok(()) => {
                        this.config.check_updates = *checked;
                        if *checked {
                            (this.update_action)(Action::Check(false));
                        }
                    }
                    Err(error) => {
                        this.status = Some(super::popup::Status::Error(format!(
                            "Settings not saved: {error}"
                        )))
                    }
                }
                cx.notify();
            }));
        div()
            .flex()
            .flex_col()
            .gap(S2)
            .pt(S3)
            .child(
                div()
                    .id("update-version")
                    .role(Role::Status)
                    .aria_label(format!("Clipdeck version {}", env!("CARGO_PKG_VERSION")))
                    .text_size(META)
                    .text_color(p.muted)
                    .child(format!("Updates · version {}", env!("CARGO_PKG_VERSION"))),
            )
            .child(
                div()
                    .id("update-message")
                    .role(Role::Status)
                    .aria_label(message.clone())
                    .text_size(META)
                    .text_color(if matches!(state, State::Error(_)) {
                        p.danger
                    } else {
                        p.secondary
                    })
                    .child(message),
            )
            .children(action.map(|(label, action)| {
                button("update-action", None, label, true, p)
                    .on_click(cx.listener(move |this, _, _, _| (this.update_action)(action)))
            }))
            .children(offered.then(|| {
                button("update-release-notes", None, "Release notes", false, p).on_click(
                    cx.listener(|this, _, _, _| {
                        let state = this.updates.borrow();
                        let version = match &*state {
                            State::Available(r) => Some(r.manifest.version),
                            State::Ready(s) => Some(s.release.manifest.version),
                            _ => None,
                        };
                        if let Some(version) = version {
                            let _ = crate::platform::open_external(&format!(
                                "https://github.com/HenkDz/clipdeck/releases/tag/v{version}"
                            ));
                        }
                    }),
                )
            }))
            .child(div().flex().items_center().gap(S3).pt(S1).child(switch))
            .child(
                div()
                    .text_size(META)
                    .text_color(p.muted)
                    .child("Update checks contact GitHub. Clipboard contents stay on your PC."),
            )
            .children(notes.map(|text| {
                div()
                    .pt(S2)
                    .text_size(META)
                    .text_color(p.secondary)
                    .child(text.lines().take(6).collect::<Vec<_>>().join("\n"))
            }))
    }
}
