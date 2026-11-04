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
pub use focus::{Target, configure_popup, hide_popup};
pub use instance::Instance;
pub use tray::{open_external, open_settings};

pub enum Event {
    Clip {
        clip: crate::clip::Clip,
        own_write: bool,
    },
    Open(Target),
    Pause,
    Quit,
    Settings,
    Error(String),
}
