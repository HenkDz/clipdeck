use gpui::{App, Hsla, Pixels, Window, px, rgb};
use gpui_component::{ActiveTheme, Theme};

// One spacing scale and one type scale for every surface.
pub(super) const S1: Pixels = px(4.);
pub(super) const S2: Pixels = px(8.);
pub(super) const S3: Pixels = px(12.);
pub(super) const S4: Pixels = px(16.);
/// Row titles and body text.
pub(super) const TITLE: Pixels = px(13.);
/// Meta lines, captions, buttons and footer hints.
pub(super) const META: Pixels = px(12.);
/// Section labels and key caps.
pub(super) const LABEL: Pixels = px(11.);
/// Corner radius of rows, fields and wells.
pub(super) const RADIUS: Pixels = px(8.);

/// Colors from docs/mockup.html. Overlays are pre-blended onto the surface so rings and
/// fades can match a row's background exactly.
#[derive(Clone, Copy)]
pub(super) struct Palette {
    pub surface: Hsla,
    pub text: Hsla,
    pub secondary: Hsla,
    pub muted: Hsla,
    /// Separators and quiet outlines.
    pub hairline: Hsla,
    /// Chip and key-cap outlines.
    pub border: Hsla,
    pub hover: Hsla,
    pub selected: Hsla,
    /// Recessed areas: search field, thumbnail slots, previews.
    pub well: Hsla,
    pub link: (Hsla, Hsla),
    pub mail: (Hsla, Hsla),
    pub file: (Hsla, Hsla),
    pub neutral: (Hsla, Hsla),
    pub accent: Hsla,
    pub success: Hsla,
    pub danger: Hsla,
    pub chip: Hsla,
    pub on_chip: Hsla,
}

fn c(hex: u32) -> Hsla {
    rgb(hex).into()
}

pub(super) fn palette(cx: &App) -> Palette {
    if cx.theme().is_dark() {
        Palette {
            surface: c(0x262624),
            text: c(0xeceae4),
            secondary: c(0xb4b2a9),
            muted: c(0x8a8984),
            hairline: c(0x363634),
            border: c(0x4a4a47),
            hover: c(0x2e2e2c),
            selected: c(0x393937),
            well: c(0x1e1e1d),
            link: (c(0x0c447c), c(0xb5d4f4)),
            mail: (c(0x3c3489), c(0xcecbf6)),
            file: (c(0x4d3b16), c(0xf0cf8e)),
            neutral: (c(0x353533), c(0xb4b2a9)),
            accent: c(0x6aa7e8),
            success: c(0x7fc98c),
            danger: c(0xf09595),
            chip: c(0xeceae4),
            on_chip: c(0x1f1e1d),
        }
    } else {
        Palette {
            surface: c(0xffffff),
            text: c(0x1f1e1d),
            secondary: c(0x5f5e5a),
            muted: c(0x8a8984),
            hairline: c(0xebeae6),
            border: c(0xd6d5d0),
            hover: c(0xf6f5f2),
            selected: c(0xedece8),
            well: c(0xf4f3ee),
            link: (c(0xe6f1fb), c(0x185fa5)),
            mail: (c(0xeeedfe), c(0x534ab7)),
            file: (c(0xf7efdd), c(0x86580f)),
            neutral: (c(0xf1f0ec), c(0x5f5e5a)),
            accent: c(0x2f73b7),
            success: c(0x2f7d3c),
            danger: c(0xa32d2d),
            chip: c(0x1f1e1d),
            on_chip: c(0xffffff),
        }
    }
}

pub fn sync_theme(window: Option<&mut Window>, cx: &mut App) {
    Theme::sync_system_appearance(window, cx);
    let p = palette(cx);
    let dark = cx.theme().is_dark();
    Theme::update(cx, |theme| {
        let c = &mut theme.colors;
        c.background = p.surface;
        c.foreground = p.text;
        c.muted_foreground = p.muted;
        c.border = p.hairline;
        c.secondary = p.hover;
        // Menus highlight with the neutral selection, not a saturated accent.
        c.accent = p.selected;
        c.accent_foreground = p.text;
        c.popover = if dark {
            rgb(0x2c2c2a).into()
        } else {
            p.surface
        };
        c.popover_foreground = p.text;
        c.input = p.border;
        c.ring = p.muted;
        c.success = p.success;
        c.danger = p.danger;
    });
}
