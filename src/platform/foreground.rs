use super::Target;
use std::sync::atomic::{AtomicUsize, Ordering};
use windows::Win32::{
    Foundation::HWND,
    UI::{Accessibility::*, WindowsAndMessaging::*},
};

// Last foreground window that can receive a paste: not Clipdeck, the taskbar, or the desktop.
static LAST: AtomicUsize = AtomicUsize::new(0);

const SHELL_CLASSES: [&str; 6] = [
    "Shell_TrayWnd",
    "Shell_SecondaryTrayWnd",
    "NotifyIconOverflowWindow",
    "TopLevelWindowForOverflowXamlIsland",
    "Progman",
    "WorkerW",
];

fn remember(hwnd: HWND) {
    if hwnd.is_invalid() {
        return;
    }
    let target = Target::from_hwnd(hwnd);
    if target.is_app() {
        return;
    }
    let mut class = [0u16; 64];
    let len = unsafe { GetClassNameW(hwnd, &mut class) } as usize;
    let class = String::from_utf16_lossy(&class[..len]);
    if !SHELL_CLASSES.contains(&class.as_str()) {
        LAST.store(hwnd.0 as usize, Ordering::SeqCst);
        super::frame::foreground_changed(hwnd);
    }
}

unsafe extern "system" fn on_foreground(
    _: HWINEVENTHOOK,
    _: u32,
    hwnd: HWND,
    object: i32,
    _: i32,
    _: u32,
    _: u32,
) {
    if object == OBJID_WINDOW.0 {
        remember(hwnd);
    }
}

/// Starts tracking the foreground window. Call on a thread that runs a message loop.
pub fn watch() {
    remember(unsafe { GetForegroundWindow() });
    unsafe {
        let _ = SetWinEventHook(
            EVENT_SYSTEM_FOREGROUND,
            EVENT_SYSTEM_FOREGROUND,
            None,
            Some(on_foreground),
            0,
            0,
            WINEVENT_OUTOFCONTEXT,
        );
    }
}

/// The paste target for opens that don't come from an app, such as the tray icon.
pub fn last_app() -> Target {
    match LAST.load(Ordering::SeqCst) {
        0 => Target::capture(),
        hwnd => Target::from_hwnd(HWND(hwnd as *mut _)),
    }
}
