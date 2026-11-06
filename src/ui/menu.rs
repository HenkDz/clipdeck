use super::{
    Popup,
    commands::{Command, FILE_TRANSFORMS, TEXT_TRANSFORMS},
    rows::file_icon,
    theme::{S3, palette},
    widgets::keycap,
};
use crate::{clip::Clip, files::Kind};
use gpui::*;
use gpui_component::{
    ActiveTheme, Icon,
    menu::{PopupMenu, PopupMenuItem},
};
use gpui_kit_assets::IconName;

fn glyph(cmd: Command) -> IconName {
    match cmd {
        Command::Preview => IconName::Eye,
        Command::OpenLocation => IconName::FolderOpen,
        Command::OpenExternal | Command::Open => IconName::ExternalLink,
        Command::Paste(false) | Command::PasteFile(_) => IconName::ClipboardPaste,
        Command::Paste(true) => IconName::ClipboardType,
        Command::Pin => IconName::Pin,
        Command::Delete => IconName::Trash,
        Command::Edit => IconName::Pencil,
        Command::Copy => IconName::Copy,
        Command::Details => IconName::Info,
        Command::Save => IconName::Download,
        Command::PasteImages => IconName::Images,
        Command::CopyFile(_) => IconName::Copy,
        Command::FileText(_, false) => IconName::TextAlignStart,
        Command::FileText(_, true) => IconName::Type,
        Command::Transform(t) => match t {
            "UPPERCASE" => IconName::CaseUpper,
            "lowercase" => IconName::CaseLower,
            "Trim spaces" => IconName::Scissors,
            "One line" => IconName::TextWrap,
            "Paths as text" => IconName::TextAlignStart,
            _ => IconName::Type,
        },
    }
}

fn shortcut(cmd: Command) -> Option<&'static str> {
    match cmd {
        Command::Preview => Some("Space"),
        Command::Copy => Some("Ctrl+C"),
        Command::Paste(false) => Some("Enter"),
        Command::Paste(true) => Some("Shift+Enter"),
        Command::Pin => Some("Ctrl+P"),
        Command::Delete => Some("Del"),
        _ => None,
    }
}

/// A menu row: icon column, label, and the shortcut right-aligned as a key cap.
fn entry(
    label: impl Into<SharedString>,
    icon: IconName,
    key: Option<&'static str>,
    tone: Tone,
    on_click: impl Fn(&mut Window, &mut App) + 'static,
) -> PopupMenuItem {
    let label = label.into();
    PopupMenuItem::element(move |_, cx| {
        let p = palette(cx);
        div()
            .flex()
            .items_center()
            .w_full()
            .gap(S3)
            .text_color(match tone {
                Tone::Normal => cx.theme().foreground,
                Tone::Danger => p.danger,
                Tone::Disabled => p.muted,
            })
            .child(div().flex_1().truncate().child(label.clone()))
            .children(key.map(|key| keycap(key, p)))
    })
    .icon(Icon::new(icon))
    .on_click(move |_, window, cx| on_click(window, cx))
    .disabled(tone == Tone::Disabled)
}

#[derive(Clone, Copy, PartialEq)]
enum Tone {
    Normal,
    Danger,
    Disabled,
}

fn item(
    label: impl Into<SharedString>,
    view: WeakEntity<Popup>,
    id: i64,
    cmd: Command,
) -> PopupMenuItem {
    action(label, view, id, cmd, true)
}

fn action(
    label: impl Into<SharedString>,
    view: WeakEntity<Popup>,
    id: i64,
    cmd: Command,
    enabled: bool,
) -> PopupMenuItem {
    let tone = if !enabled {
        Tone::Disabled
    } else if cmd == Command::Delete {
        Tone::Danger
    } else {
        Tone::Normal
    };
    entry(label, glyph(cmd), shortcut(cmd), tone, move |window, cx| {
        let _ = view.update(cx, |this, cx| this.command(id, cmd, window, cx));
    })
}

pub(super) fn build(
    mut menu: PopupMenu,
    view: WeakEntity<Popup>,
    id: i64,
    clip: &Clip,
    selection: (usize, bool),
    window: &mut Window,
    cx: &mut Context<PopupMenu>,
) -> PopupMenu {
    if selection.0 > 1 {
        return menu
            .min_w(px(232.))
            .max_w(px(280.))
            .item(item("Copy selected", view.clone(), id, Command::Copy))
            .item(item(
                "Paste selected",
                view.clone(),
                id,
                Command::Paste(false),
            ))
            .separator()
            .item(item(
                if selection.1 {
                    "Unpin selected"
                } else {
                    "Pin selected"
                },
                view.clone(),
                id,
                Command::Pin,
            ))
            .item(item("Delete selected", view, id, Command::Delete));
    }
    let media = matches!(clip.kind.as_str(), "image" | "files");
    menu = menu.min_w(px(232.)).max_w(px(280.)).item(item(
        "Preview",
        view.clone(),
        id,
        Command::Preview,
    ));
    if media {
        menu = menu
            .item(item(
                "Show in folder",
                view.clone(),
                id,
                Command::OpenLocation,
            ))
            .item(item(
                "Open in default app",
                view.clone(),
                id,
                Command::OpenExternal,
            ));
    }
    if clip.kind == "link" {
        menu = menu.item(item("Open in default app", view.clone(), id, Command::Open));
    }
    menu = menu
        .separator()
        .item(item("Paste", view.clone(), id, Command::Paste(false)))
        .item(action(
            "Paste as plain text",
            view.clone(),
            id,
            Command::Paste(true),
            clip.is_text(),
        ));
    let subview = view.clone();
    let clip_for_sub = clip.clone();
    menu = menu
        .submenu_with_icon(
            Some(Icon::new(IconName::WandSparkles)),
            "Paste as…",
            window,
            cx,
            move |sub, window, cx| paste_as(sub, subview.clone(), &clip_for_sub, window, cx),
        )
        .separator()
        .item(
            item(
                if clip.pinned { "Unpin" } else { "Pin" },
                view.clone(),
                id,
                Command::Pin,
            )
            .icon(Icon::new(if clip.pinned {
                IconName::PinOff
            } else {
                IconName::Pin
            })),
        )
        .item(action(
            "Edit",
            view.clone(),
            id,
            Command::Edit,
            clip.is_text(),
        ))
        .separator()
        .item(item("Delete", view.clone(), id, Command::Delete))
        .separator();
    menu.submenu_with_icon(
        Some(Icon::new(IconName::Ellipsis)),
        "More",
        window,
        cx,
        move |sub, _, _| {
            sub.min_w(px(220.))
                .item(item("Copy to clipboard", view.clone(), id, Command::Copy))
                .item(item("Details", view.clone(), id, Command::Details))
        },
    )
}

fn paste_as(
    mut sub: PopupMenu,
    view: WeakEntity<Popup>,
    clip: &Clip,
    window: &mut Window,
    cx: &mut Context<PopupMenu>,
) -> PopupMenu {
    let id = clip.id;
    sub = sub.min_w(px(220.)).max_w(px(300.));
    match clip.kind.as_str() {
        "image" => sub.item(item("Save as file…", view, id, Command::Save)),
        "files" => {
            for transform in FILE_TRANSFORMS {
                sub = sub.item(item(
                    transform,
                    view.clone(),
                    id,
                    Command::Transform(transform),
                ));
            }
            let kinds = clip.file_kinds();
            let images = kinds.iter().filter(|k| **k == Kind::Image).count();
            if images > 0 && images < kinds.len() {
                sub = sub.item(item(
                    format!("Only the images ({images})"),
                    view.clone(),
                    id,
                    Command::PasteImages,
                ));
            }
            if kinds.len() > 1 {
                let paths: Vec<(String, Kind)> = clip
                    .paths()
                    .iter()
                    .zip(kinds)
                    .take(40)
                    .map(|(p, k)| (crate::files::name(p).to_owned(), k))
                    .collect();
                let more = clip.file_count.saturating_sub(paths.len());
                sub = sub.separator().submenu_with_icon(
                    Some(Icon::new(IconName::Files)),
                    "One file",
                    window,
                    cx,
                    move |mut files, _, _| {
                        files = files.min_w(px(200.)).max_w(px(320.)).scrollable(true);
                        for (ix, (name, kind)) in paths.iter().enumerate() {
                            let view = view.clone();
                            files = files.item(entry(
                                name.clone(),
                                file_icon(*kind),
                                None,
                                Tone::Normal,
                                move |window, cx| {
                                    let _ = view.update(cx, |this, cx| {
                                        this.command(id, Command::PasteFile(ix), window, cx)
                                    });
                                },
                            ));
                        }
                        if more > 0 {
                            files = files.item(
                                PopupMenuItem::new(format!("{more} more in Preview"))
                                    .disabled(true),
                            );
                        }
                        files
                    },
                );
            }
            sub
        }
        _ => {
            for transform in TEXT_TRANSFORMS {
                sub = sub.item(item(
                    transform,
                    view.clone(),
                    id,
                    Command::Transform(transform),
                ));
            }
            sub
        }
    }
}

/// The header's menu: list density, keep open, settings file.
pub(super) fn header(
    menu: PopupMenu,
    view: WeakEntity<Popup>,
    compact: bool,
    keep: bool,
) -> PopupMenu {
    let density = |label: &'static str, on: bool| {
        let view = view.clone();
        PopupMenuItem::new(label)
            .checked(on)
            .on_click(move |_, _, cx| {
                let _ = view.update(cx, |this, cx| {
                    if this.compact() != (label == "Compact") {
                        this.toggle_density(cx);
                    }
                });
            })
    };
    let keep_view = view.clone();
    let settings_view = view.clone();
    menu.min_w(px(232.))
        .item(PopupMenuItem::label("List density"))
        .item(density("Comfortable", !compact))
        .item(density("Compact", compact))
        .separator()
        .item(
            PopupMenuItem::new("Keep open on top")
                .checked(keep)
                .on_click(move |_, window, cx| {
                    let _ = keep_view.update(cx, |this, cx| this.toggle_keep(window, cx));
                }),
        )
        .separator()
        .item(entry(
            "Settings",
            IconName::Settings2,
            Some("Ctrl+,"),
            Tone::Normal,
            move |window, cx| {
                let _ = settings_view.update(cx, |this, cx| this.open_settings(window, cx));
            },
        ))
}

/// The menu for one file in the gallery or its large preview.
pub(super) fn file(
    menu: PopupMenu,
    view: WeakEntity<Popup>,
    id: i64,
    ix: usize,
    path: String,
    missing: bool,
    in_grid: bool,
) -> PopupMenu {
    let present = !missing;
    let run = |cmd: Command, label: &'static str, key: Option<&'static str>, enabled: bool| {
        let view = view.clone();
        let tone = if enabled {
            Tone::Normal
        } else {
            Tone::Disabled
        };
        entry(label, glyph(cmd), key, tone, move |window, cx| {
            let _ = view.update(cx, |this, cx| this.command(id, cmd, window, cx));
        })
    };
    let open = |label: &'static str, icon: IconName, reveal: bool| {
        let (view, path) = (view.clone(), path.clone());
        let tone = if present {
            Tone::Normal
        } else {
            Tone::Disabled
        };
        entry(label, icon, None, tone, move |_, cx| {
            let path = path.clone();
            let _ = view.update(cx, |this, cx| this.open_file(path, reveal, cx));
        })
    };
    let mut menu = menu
        .min_w(px(232.))
        .max_w(px(280.))
        .item(run(
            Command::PasteFile(ix),
            "Paste this file",
            Some("Enter"),
            present,
        ))
        .item(run(Command::CopyFile(ix), "Copy this file", None, present))
        .item(run(
            Command::FileText(ix, false),
            "Paste path as text",
            None,
            true,
        ))
        .item(run(
            Command::FileText(ix, true),
            "Paste file name",
            None,
            true,
        ))
        .separator();
    if in_grid {
        let view = view.clone();
        menu = menu.item(entry(
            "Preview",
            IconName::Eye,
            Some("Space"),
            Tone::Normal,
            move |_, cx| {
                let _ = view.update(cx, |this, cx| this.open_gallery_file(ix, cx));
            },
        ));
    }
    menu.item(open("Open in default app", IconName::ExternalLink, false))
        .item(open("Show in folder", IconName::FolderOpen, true))
}
