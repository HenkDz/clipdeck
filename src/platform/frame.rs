use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use std::sync::{
    Mutex,
    atomic::{AtomicBool, AtomicIsize, AtomicUsize, Ordering},
};
use windows::Win32::{
    Foundation::*, Graphics::Dwm::*, UI::HiDpi::GetDpiForWindow, UI::WindowsAndMessaging::*,
};

static POPUP: AtomicUsize = AtomicUsize::new(0);
static TARGET: AtomicUsize = AtomicUsize::new(0);
static MOUSE: AtomicBool = AtomicBool::new(false);
static KEEP: AtomicBool = AtomicBool::new(false);
static PROC: AtomicIsize = AtomicIsize::new(0);

/// Popup size in logical pixels and, for keep-open mode, its last screen position.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Geometry {
    pub size: (u32, u32),
    pub position: Option<(i32, i32)>,
}
static GEOMETRY: Mutex<Geometry> = Mutex::new(Geometry {
    size: (480, 520),
    position: None,
});
pub fn set_geometry(geometry: Geometry) {
    *GEOMETRY.lock().unwrap() = geometry;
}
pub(super) fn geometry() -> Geometry {
    *GEOMETRY.lock().unwrap()
}
/// Keep-open mode: the popup survives focus loss and pastes, and its header drags it.
pub fn set_keep(keep: bool) {
    KEEP.store(keep, Ordering::SeqCst);
    let hwnd = HWND(POPUP.load(Ordering::SeqCst) as *mut _);
    if !hwnd.is_invalid() {
        unsafe {
            let _ = SetWindowPos(
                hwnd,
                Some(if keep { HWND_TOPMOST } else { HWND_NOTOPMOST }),
                0,
                0,
                0,
                0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
            );
        }
    }
}
pub fn keep() -> bool {
    KEEP.load(Ordering::SeqCst)
}

pub(super) fn hwnd(window: &gpui::Window) -> Option<HWND> {
    match HasWindowHandle::window_handle(window).ok()?.as_raw() {
        RawWindowHandle::Win32(h) => Some(HWND(h.hwnd.get() as *mut _)),
        _ => None,
    }
}

/// Resize hit-test for a frameless window: thin edges, larger corners.
fn edge_hit(rect: RECT, x: i32, y: i32, border: i32, corner: i32) -> Option<u32> {
    let left = x < rect.left + border;
    let right = x >= rect.right - border;
    let top = y < rect.top + border;
    let bottom = y >= rect.bottom - border;
    let near_left = x < rect.left + corner;
    let near_right = x >= rect.right - corner;
    let near_top = y < rect.top + corner;
    let near_bottom = y >= rect.bottom - corner;
    Some(if (bottom && near_right) || (right && near_bottom) {
        HTBOTTOMRIGHT
    } else if (bottom && near_left) || (left && near_bottom) {
        HTBOTTOMLEFT
    } else if (top && near_right) || (right && near_top) {
        HTTOPRIGHT
    } else if (top && near_left) || (left && near_top) {
        HTTOPLEFT
    } else if left {
        HTLEFT
    } else if right {
        HTRIGHT
    } else if top {
        HTTOP
    } else if bottom {
        HTBOTTOM
    } else {
        return None;
    })
}

fn remember_bounds(h: HWND) {
    let mut rect = RECT::default();
    unsafe {
        if GetWindowRect(h, &mut rect).is_err() {
            return;
        }
    }
    let dpi = unsafe { GetDpiForWindow(h) }.max(96) as i32;
    let width = ((rect.right - rect.left) * 96 / dpi) as u32;
    let height = ((rect.bottom - rect.top) * 96 / dpi) as u32;
    let geometry = Geometry {
        size: (width, height),
        position: Some((rect.left, rect.top)),
    };
    set_geometry(geometry);
    super::clipboard::send(super::Event::Geometry(geometry));
}

/// The popup's screen rectangle, to put back after GPUI's deferred first placement.
pub fn rect(window: &gpui::Window) -> Option<RECT> {
    let h = hwnd(window)?;
    let mut rect = RECT::default();
    unsafe { GetWindowRect(h, &mut rect) }.ok()?;
    Some(rect)
}
pub fn set_rect(window: &gpui::Window, rect: RECT) {
    if let Some(h) = hwnd(window) {
        place(h, rect);
    }
}

/// Records the current bounds, e.g. when keep-open mode starts at the popup's current spot.
pub fn save_bounds(window: &gpui::Window) {
    if let Some(h) = hwnd(window) {
        remember_bounds(h);
    }
}

unsafe extern "system" fn proc(h: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    let original = || unsafe {
        CallWindowProcW(
            std::mem::transmute::<isize, WNDPROC>(PROC.load(Ordering::SeqCst)),
            h,
            msg,
            wp,
            lp,
        )
    };
    match msg {
        // GPUI's frameless resize inset otherwise leaves transparent strips around the client.
        WM_NCCALCSIZE => LRESULT(0),
        WM_MOUSEACTIVATE if MOUSE.load(Ordering::SeqCst) => LRESULT(MA_NOACTIVATE as isize),
        WM_NCHITTEST => {
            let mut rect = RECT::default();
            let dpi = unsafe { GetDpiForWindow(h) }.max(96) as i32;
            let (x, y) = (lp.0 as i16 as i32, (lp.0 >> 16) as i16 as i32);
            if unsafe { GetWindowRect(h, &mut rect) }.is_ok()
                && let Some(hit) = edge_hit(rect, x, y, 6 * dpi / 96, 16 * dpi / 96)
            {
                return LRESULT(hit as isize);
            }
            original()
        }
        // A caption double-click would maximize the popup.
        WM_NCLBUTTONDBLCLK if wp.0 == HTCAPTION as usize => LRESULT(0),
        WM_GETMINMAXINFO => {
            let result = original();
            let dpi = unsafe { GetDpiForWindow(h) }.max(96) as i32;
            let (w, h) = crate::config::MIN_SIZE;
            unsafe {
                let info = &mut *(lp.0 as *mut MINMAXINFO);
                info.ptMinTrackSize = POINT {
                    x: w as i32 * dpi / 96,
                    y: h as i32 * dpi / 96,
                };
            }
            result
        }
        WM_PLACE => {
            if let Some(r) = PENDING.lock().unwrap().take() {
                unsafe {
                    let _ = SetWindowPos(
                        h,
                        Some(
                            if KEEP.load(Ordering::SeqCst) || MOUSE.load(Ordering::SeqCst) {
                                HWND_TOPMOST
                            } else {
                                HWND_NOTOPMOST
                            },
                        ),
                        r.left,
                        r.top,
                        r.right - r.left,
                        r.bottom - r.top,
                        SWP_NOACTIVATE | SWP_FRAMECHANGED,
                    );
                }
            }
            LRESULT(0)
        }
        super::drag::WM_DRAG => {
            super::drag::run(h);
            LRESULT(0)
        }
        WM_EXITSIZEMOVE => {
            let result = original();
            remember_bounds(h);
            result
        }
        _ => original(),
    }
}

fn subclass(h: HWND) {
    if POPUP.swap(h.0 as usize, Ordering::SeqCst) == 0 {
        unsafe {
            PROC.store(
                SetWindowLongPtrW(h, GWLP_WNDPROC, proc as *const () as isize),
                Ordering::SeqCst,
            );
        }
    }
}

const WM_PLACE: u32 = WM_APP + 0x31;
static PENDING: Mutex<Option<RECT>> = Mutex::new(None);
/// Moves the popup from its own message loop. Called during a GPUI update, a direct
/// SetWindowPos would deliver WM_SIZE while the window is borrowed and GPUI would
/// keep laying out at the old size.
pub(super) fn place(h: HWND, rect: RECT) {
    subclass(h);
    *PENDING.lock().unwrap() = Some(rect);
    unsafe {
        let _ = PostMessageW(Some(h), WM_PLACE, WPARAM(0), LPARAM(0));
    }
}

pub fn mouse_mode(window: &gpui::Window, mouse: bool, target: super::Target) {
    let Some(h) = hwnd(window) else { return };
    TARGET.store(target.raw(), Ordering::SeqCst);
    MOUSE.store(mouse, Ordering::SeqCst);
    unsafe {
        subclass(h);
        let style = GetWindowLongPtrW(h, GWL_EXSTYLE);
        SetWindowLongPtrW(
            h,
            GWL_EXSTYLE,
            if mouse {
                style | WS_EX_NOACTIVATE.0 as isize
            } else {
                style & !(WS_EX_NOACTIVATE.0 as isize)
            },
        );
        let _ = ShowWindow(h, if mouse { SW_SHOWNOACTIVATE } else { SW_SHOW });
        if mouse && !target.restore() {
            eprintln!("Windows refused the tray paste target's foreground restore");
        }
    }
}

pub(super) fn foreground_changed(h: HWND) {
    if MOUSE.load(Ordering::SeqCst)
        && !KEEP.load(Ordering::SeqCst)
        && h.0 as usize != TARGET.load(Ordering::SeqCst)
    {
        unsafe {
            let _ = ShowWindow(HWND(POPUP.load(Ordering::SeqCst) as *mut _), SW_HIDE);
        }
    }
}

pub fn visible(window: &gpui::Window) -> bool {
    hwnd(window).is_some_and(|h| unsafe { IsWindowVisible(h).as_bool() })
}

// Return false on Windows 10 so the client draws the sole, square border.
pub fn native_border(window: &gpui::Window, color: u32) -> bool {
    let Some(h) = hwnd(window) else { return false };
    unsafe {
        let round = DWMWCP_ROUND;
        let supported = DwmSetWindowAttribute(
            h,
            DWMWA_WINDOW_CORNER_PREFERENCE,
            &round as *const _ as _,
            4,
        )
        .is_ok();
        let color = COLORREF(((color & 255) << 16) | (color & 0xff00) | ((color >> 16) & 255));
        if supported {
            let _ = DwmSetWindowAttribute(h, DWMWA_BORDER_COLOR, &color as *const _ as _, 4);
        }
        supported
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn resize_edges_and_corners() {
        let r = RECT {
            left: -400,
            top: 100,
            right: 80,
            bottom: 620,
        };
        let hit = |x, y| edge_hit(r, x, y, 6, 16);
        assert_eq!(hit(-200, 300), None);
        assert_eq!(hit(-400, 300), Some(HTLEFT));
        assert_eq!(hit(79, 300), Some(HTRIGHT));
        assert_eq!(hit(-200, 100), Some(HTTOP));
        assert_eq!(hit(-200, 619), Some(HTBOTTOM));
        // The bottom-right grip is wider than the edge itself.
        assert_eq!(hit(70, 619), Some(HTBOTTOMRIGHT));
        assert_eq!(hit(79, 606), Some(HTBOTTOMRIGHT));
        assert_eq!(hit(-399, 101), Some(HTTOPLEFT));
        assert_eq!(hit(79, 105), Some(HTTOPRIGHT));
        assert_eq!(hit(-395, 615), Some(HTBOTTOMLEFT));
        assert_eq!(hit(73, 300), None);
    }
}
