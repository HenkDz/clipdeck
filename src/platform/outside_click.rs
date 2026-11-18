use std::sync::atomic::{AtomicUsize, Ordering};
use tray_icon::{Rect, TrayIcon};
use windows::{
    Win32::{Foundation::*, System::LibraryLoader::GetModuleHandleW, UI::WindowsAndMessaging::*},
    core::Result,
};

const WM_OUTSIDE_PRESS: u32 = WM_APP + 0x35;
const WM_WATCH_CLICKS: u32 = WM_APP + 0x36;
static RECEIVER: AtomicUsize = AtomicUsize::new(0);

pub(super) fn bind(receiver: HWND) {
    RECEIVER.store(receiver.0 as usize, Ordering::Relaxed);
}

fn receiver() -> HWND {
    HWND(RECEIVER.load(Ordering::Relaxed) as *mut _)
}

pub(super) fn watch_visible(visible: bool) {
    let receiver = receiver();
    if !receiver.is_invalid() {
        let enabled = visible && !super::frame::keep();
        let _ = unsafe {
            PostMessageW(
                Some(receiver),
                WM_WATCH_CLICKS,
                WPARAM(usize::from(enabled)),
                LPARAM(0),
            )
        };
    }
}

pub(super) struct Watch(HHOOK);

impl Watch {
    fn start() -> Result<Self> {
        let hook = unsafe {
            SetWindowsHookExW(
                WH_MOUSE_LL,
                Some(mouse),
                Some(GetModuleHandleW(None)?.into()),
                0,
            )?
        };
        Ok(Self(hook))
    }
}

impl Drop for Watch {
    fn drop(&mut self) {
        let _ = unsafe { UnhookWindowsHookEx(self.0) };
    }
}

unsafe extern "system" fn mouse(code: i32, event: WPARAM, data: LPARAM) -> LRESULT {
    if code == HC_ACTION as i32
        && matches!(
            event.0 as u32,
            WM_LBUTTONDOWN | WM_RBUTTONDOWN | WM_MBUTTONDOWN | WM_XBUTTONDOWN
        )
        && super::frame::dismissible_popup().is_some()
    {
        let point = unsafe { (*(data.0 as *const MSLLHOOKSTRUCT)).pt };
        // Hit-testing and Shell queries run after the hook returns, so input is never held up by them.
        let _ = post_press(receiver(), point);
    }
    unsafe { CallNextHookEx(None, code, event, data) }
}

pub(super) fn post_press(receiver: HWND, point: POINT) -> Result<()> {
    unsafe {
        PostMessageW(
            Some(receiver),
            WM_OUTSIDE_PRESS,
            WPARAM(point.x as usize),
            LPARAM(point.y as isize),
        )
    }
}

pub(super) fn handle(msg: &MSG, tray: Option<&TrayIcon>, watch: &mut Option<Watch>) -> bool {
    if msg.message == WM_WATCH_CLICKS {
        if msg.wParam.0 == 0 {
            *watch = None;
        } else if watch.is_none() {
            *watch = Watch::start()
                .inspect_err(|error| eprintln!("Outside-click watcher: {error}"))
                .ok();
        }
        return true;
    }
    if msg.message != WM_OUTSIDE_PRESS {
        return false;
    }
    if let Some(popup) = super::frame::dismissible_popup() {
        let point = POINT {
            x: msg.wParam.0 as i32,
            y: msg.lParam.0 as i32,
        };
        let hit = unsafe { WindowFromPoint(point) };
        let root = unsafe { GetAncestor(hit, GA_ROOT) };
        if !super::Target::from_hwnd(hit).is_app()
            && !(super::foreground::is_shell(root)
                && tray
                    .and_then(TrayIcon::rect)
                    .is_some_and(|rect| contains(rect, point)))
        {
            let _ = unsafe {
                PostMessageW(
                    Some(popup),
                    super::frame::WM_DISMISS_OUTSIDE,
                    WPARAM(0),
                    LPARAM(0),
                )
            };
        }
    }
    true
}

fn contains(rect: Rect, point: POINT) -> bool {
    let (x, y) = (f64::from(point.x), f64::from(point.y));
    x >= rect.position.x
        && x < rect.position.x + f64::from(rect.size.width)
        && y >= rect.position.y
        && y < rect.position.y + f64::from(rect.size.height)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tray_icon_exemption_is_limited_to_its_physical_bounds() {
        let mut rect = Rect::default();
        (rect.position.x, rect.position.y) = (-40., 100.);
        (rect.size.width, rect.size.height) = (20, 20);
        assert!(contains(rect, POINT { x: -40, y: 100 }));
        assert!(!contains(rect, POINT { x: -20, y: 100 }));
        assert!(!contains(rect, POINT { x: -40, y: 120 }));
    }
}
