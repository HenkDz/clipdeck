pub mod drag;
mod frame;
pub mod preview;
pub use frame::{
    Geometry, keep, mouse_mode, native_border, rect, save_bounds, set_geometry, set_keep, set_rect,
    visible,
};
mod batch;
mod capture;
mod clipboard;
mod focus;
mod foreground;
mod icon;
mod image;
mod instance;
pub mod locations;
mod placement;
mod source;
pub mod startup;
mod time;
mod tray;
pub use time::{is_today, when};

pub use clipboard::{Clipboard, start_watcher};
pub use clipboard::{clipboard_batch_sent, paste_sent};
pub use clipboard::{is_current, set_current, set_current_ids};
pub use focus::{Target, configure_popup, hide_popup};
pub use foreground::last_app;
pub use instance::Instance;
pub use tray::open_external;

pub enum Event {
    Update(crate::update::Action),
    UpdateState(crate::update::State),
    PasteSent {
        id: i64,
        hash: Vec<u8>,
        plain: bool,
    },
    PasteBatch {
        clips: Vec<(i64, Vec<u8>)>,
        plain: bool,
    },
    ImageLocationChanged,
    /// A clip was dropped into another app.
    Dropped {
        id: i64,
        hash: Vec<u8>,
    },
    Geometry(Geometry),
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
    ShowSettings,
    ShowUpdates,
    Error(String),
}
