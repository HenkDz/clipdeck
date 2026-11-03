use super::{Popup, commands::Command};
use gpui::*;
use gpui_component::{
    ActiveTheme, Icon,
    button::{Button, ButtonVariants},
    menu::ContextMenuExt,
};
use gpui_kit_assets::IconName;
use std::sync::Arc;
pub(super) fn icon(name: &str) -> Icon {
    let name = match name {
        "link" => IconName::Link,
        "email" => IconName::Mail,
        "image" => IconName::Image,
        "files" => IconName::Folder,
        "pin" => IconName::Pin,
        "delete" => IconName::Trash,
        "plain" => IconName::ClipboardPaste,
        _ => IconName::FileText,
    };
    Icon::new(name).size_4()
}
impl Popup {
    pub(super) fn row(
        &mut self,
        item: usize,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(ix) = self.items[item] else {
            let label = self.items[item + 1]
                .map(|ix| self.clips[ix].section())
                .unwrap_or("");
            return div()
                .h(px(26.))
                .px_3()
                .py_1()
                .border_t_1()
                .border_color(cx.theme().border)
                .text_xs()
                .text_color(cx.theme().muted_foreground)
                .child(label)
                .into_any_element();
        };
        let clip = &self.clips[ix];
        let id = clip.id;
        let selected = ix == self.selected;
        let image = if clip.kind == "image" {
            img(Arc::new(Image::from_bytes(
                ImageFormat::Png,
                clip.thumbnail.clone(),
            )))
            .w(px(56.))
            .h(px(36.))
            .object_fit(ObjectFit::Contain)
            .into_any_element()
        } else {
            div()
                .size(px(28.))
                .rounded_lg()
                .flex()
                .items_center()
                .justify_center()
                .bg(cx.theme().accent)
                .child(icon(&clip.kind))
                .into_any_element()
        };
        let view = cx.entity().downgrade();
        div()
            .id(("clip", id as u64))
            .w_full()
            .h(px(56.))
            .px_3()
            .py_2()
            .flex()
            .items_center()
            .gap_2()
            .border_t_1()
            .border_color(cx.theme().border)
            .bg(if selected {
                cx.theme().secondary
            } else {
                cx.theme().background
            })
            .child(image)
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .child(div().text_sm().truncate().child(if clip.kind == "image" {
                        "Screenshot".into()
                    } else {
                        clip.text
                            .lines()
                            .next()
                            .unwrap_or("")
                            .chars()
                            .take(180)
                            .collect::<String>()
                    }))
                    .child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .truncate()
                            .child(clip.meta()),
                    ),
            )
            .children(selected.then(|| {
                div().flex().flex_none().items_center().gap_1().children(
                    [
                        ("plain", Command::Paste(true)),
                        ("pin", Command::Pin),
                        ("delete", Command::Delete),
                    ]
                    .into_iter()
                    .filter(|(name, _)| *name != "plain" || clip.is_text())
                    .map(|(name, cmd)| {
                        Button::new((name, id as u64))
                            .ghost()
                            .compact()
                            .icon(icon(name))
                            .accessibility_label(match cmd {
                                Command::Paste(_) => "Paste as plain text",
                                Command::Pin => {
                                    if clip.pinned {
                                        "Unpin"
                                    } else {
                                        "Pin"
                                    }
                                }
                                _ => "Delete",
                            })
                            .tooltip(match cmd {
                                Command::Paste(_) => "Paste as plain text",
                                Command::Pin => {
                                    if clip.pinned {
                                        "Unpin"
                                    } else {
                                        "Pin"
                                    }
                                }
                                _ => "Delete",
                            })
                            .on_click(cx.listener(move |this, _, window, cx| {
                                cx.stop_propagation();
                                this.command(id, cmd, window, cx);
                            }))
                    }),
                )
            }))
            .children((clip.pinned && !selected).then(|| icon("pin")))
            .on_click(cx.listener(move |this, _, _, cx| {
                this.selected = ix;
                cx.notify();
            }))
            .context_menu(move |menu, window, cx| {
                super::menu::build(menu, view.clone(), id, window, cx)
            })
            .into_any_element()
    }
}
