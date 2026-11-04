use gpui::{App, Window, rgb};
use gpui_component::Theme;
pub fn sync_theme(window: Option<&mut Window>, cx: &mut App) {
    Theme::sync_system_appearance(window, cx);
    Theme::update(cx, |theme| {
        let dark = theme.is_dark();
        let c = &mut theme.colors;
        c.background = rgb(if dark { 0x262624 } else { 0xffffff }).into();
        c.foreground = rgb(if dark { 0xeceae4 } else { 0x1f1e1d }).into();
        c.muted_foreground = rgb(0x8a8984).into();
        c.border = rgb(if dark { 0x454542 } else { 0xe5e4e0 }).into();
        c.secondary = rgb(if dark { 0x3c3c39 } else { 0xeeedeb }).into();
        c.accent = rgb(if dark { 0x7ba9cf } else { 0x336d99 }).into();
        c.accent_foreground = c.foreground;
        c.popover = c.background;
        c.popover_foreground = c.foreground;
        c.input = c.border;
        c.ring = c.muted_foreground;
    });
}
