use super::{Popup, commands::Command};
use gpui::*;
use gpui_component::menu::{PopupMenu, PopupMenuItem};
use gpui_component::{ActiveTheme, kbd::Kbd};
fn item(label: &str, view: WeakEntity<Popup>, id: i64, cmd: Command) -> PopupMenuItem {
    let key = match cmd {
        Command::Preview => Some("space"),
        Command::Paste(false) => Some("enter"),
        Command::Paste(true) => Some("shift-enter"),
        Command::Pin => Some("ctrl-p"),
        Command::Delete => Some("delete"),
        _ => None,
    };
    let glyph = match cmd {
        Command::Paste(true) => "plain",
        Command::Pin => "pin",
        Command::Delete => "delete",
        Command::Edit => "edit",
        Command::Copy => "copy",
        _ => "text",
    };
    let label = label.to_owned();
    PopupMenuItem::element(move |_, cx| {
        div()
            .flex()
            .items_center()
            .w_full()
            .min_w(px(160.))
            .gap_3()
            .text_color(if matches!(cmd, Command::Delete) {
                cx.theme().danger
            } else {
                cx.theme().foreground
            })
            .child(div().flex_1().child(label.clone()))
            .children(key.map(|key| Kbd::new(Keystroke::parse(key).unwrap())))
    })
    .icon(super::rows::icon(glyph))
    .on_click(move |_, window, cx| {
        let _ = view.update(cx, |this, cx| this.command(id, cmd, window, cx));
    })
}
pub(super) fn build(
    mut menu: PopupMenu,
    view: WeakEntity<Popup>,
    id: i64,
    clip: &crate::clip::Clip,
    window: &mut Window,
    cx: &mut Context<PopupMenu>,
) -> PopupMenu {
    menu = menu
        .min_w(px(220.))
        .max_w(px(230.))
        .item(item("Preview", view.clone(), id, Command::Preview))
        .item(
            item(
                "Open file location",
                view.clone(),
                id,
                Command::OpenLocation,
            )
            .disabled(!matches!(clip.kind.as_str(), "image" | "files")),
        )
        .separator()
        .item(item("Paste", view.clone(), id, Command::Paste(false)))
        .item(
            item(
                "Paste as plain text",
                view.clone(),
                id,
                Command::Paste(true),
            )
            .disabled(!clip.is_text()),
        );
    let subview = view.clone();
    let kind = clip.kind.clone();
    menu = menu
        .submenu("Paste as…", window, cx, move |mut sub, _, _| {
            if kind == "image" {
                sub = sub.item(item("Save as file…", subview.clone(), id, Command::Save));
            } else if kind == "link" {
                sub = sub.item(item("Open in browser", subview.clone(), id, Command::Open));
            } else if matches!(kind.as_str(), "text" | "email") {
                for transform in ["UPPERCASE", "lowercase", "Trim spaces", "One line"] {
                    sub = sub.item(item(
                        transform,
                        subview.clone(),
                        id,
                        Command::Transform(transform),
                    ));
                }
            } else {
                sub = sub.item(PopupMenuItem::new("Use Paste to copy files").disabled(true));
            }
            sub.min_w(px(200.)).max_w(px(220.))
        })
        .separator()
        .item(item(
            if clip.pinned { "Unpin" } else { "Pin" },
            view.clone(),
            id,
            Command::Pin,
        ))
        .item(item("Edit", view.clone(), id, Command::Edit).disabled(!clip.is_text()))
        .separator()
        .item(item("Delete", view.clone(), id, Command::Delete))
        .separator();
    menu.submenu("More", window, cx, move |sub, _, _| {
        sub.min_w(px(200.))
            .max_w(px(220.))
            .item(item(
                "Copy without pasting",
                view.clone(),
                id,
                Command::Copy,
            ))
            .item(item("Clip details", view.clone(), id, Command::Details))
    })
}
