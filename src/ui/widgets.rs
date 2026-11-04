//! Small controls drawn to the popup's own spacing and type scale.
use super::theme::{LABEL, META, Palette, S1, S2};
use gpui::{prelude::FluentBuilder, *};
use gpui_component::{Icon, tooltip::Tooltip};
use gpui_kit_assets::IconName;

pub(super) fn icon(name: IconName, size: Pixels, color: Hsla) -> Icon {
    Icon::new(name).size(size).text_color(color)
}

/// A square ghost button holding one icon, with a tooltip and an accessible name.
pub(super) fn icon_button(
    id: impl Into<ElementId>,
    name: IconName,
    label: &'static str,
    side: Pixels,
    p: Palette,
) -> Stateful<Div> {
    ghost(id, label, side, p).child(Icon::new(name).size(px(16.)))
}

/// The empty square ghost button; the icon inherits its text color.
pub(super) fn ghost(
    id: impl Into<ElementId>,
    label: &'static str,
    side: Pixels,
    p: Palette,
) -> Stateful<Div> {
    div()
        .id(id)
        .role(Role::Button)
        .aria_label(label)
        .tooltip(move |window, cx| Tooltip::new(label).build(window, cx))
        .size(side)
        .flex_none()
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(6.))
        .cursor_pointer()
        .text_color(p.secondary)
        .hover(move |s| s.bg(p.selected).text_color(p.text))
}

/// A labelled button for view headers; `primary` is the filled main action.
pub(super) fn button(
    id: impl Into<ElementId>,
    name: Option<IconName>,
    label: &'static str,
    primary: bool,
    p: Palette,
) -> Stateful<Div> {
    let (bg, fg, hover) = if primary {
        (p.chip, p.on_chip, p.chip.opacity(0.85))
    } else {
        (p.surface, p.text, p.hover)
    };
    div()
        .id(id)
        .role(Role::Button)
        .aria_label(label)
        .h(px(28.))
        .px(S2)
        .flex_none()
        .flex()
        .items_center()
        .gap(S1)
        .rounded(px(6.))
        .border_1()
        .border_color(if primary { p.chip } else { p.border })
        .bg(bg)
        .text_color(fg)
        .text_size(META)
        .font_weight(FontWeight::MEDIUM)
        .cursor_pointer()
        .hover(move |s| s.bg(hover))
        .when_some(name, |el, name| el.child(icon(name, px(14.), fg)))
        .child(label)
}

/// A small key cap, e.g. `Enter` or `Ctrl+P`.
pub(super) fn keycap(label: &str, p: Palette) -> Div {
    div()
        .flex_none()
        .h(px(18.))
        .px(S1)
        .flex()
        .items_center()
        .rounded(px(4.))
        .border_1()
        .border_color(p.border)
        .text_size(LABEL)
        .line_height(px(16.))
        .text_color(p.secondary)
        .child(label.to_owned())
}
