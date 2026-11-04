mod frame;
pub mod preview;
pub use frame::{mouse_mode, native_border, visible};
mod capture;
mod clipboard;
mod focus;
mod foreground;
mod image;
mod instance;
mod placement;
mod source;
mod time;
mod tray;
pub use time::is_today;

pub use clipboard::{Clipboard, start_watcher};
pub use clipboard::{current_id, set_current};
pub use focus::{Target, configure_popup, hide_popup};
pub use instance::Instance;
pub use tray::{open_external, open_settings};

pub enum Event {
    ClipboardChanged,
    Clip {
        clip: Box<crate::clip::Clip>,
        own_write: bool,
    },
    Open(Target),
    TrayOpen(Target),
    Pause,
    Quit,
    Settings,
    Error(String),
}
