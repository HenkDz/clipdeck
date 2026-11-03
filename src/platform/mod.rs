mod clipboard;
mod focus;

pub use clipboard::{Clipboard, start_watcher};
pub use focus::{Target, configure_popup, hide_popup};

pub enum Event {
    Clip { text: String, own_write: bool },
    Open(Target),
    Error(String),
}
