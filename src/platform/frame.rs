use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use std::sync::atomic::{AtomicBool, AtomicIsize, AtomicUsize, Ordering};
use windows::Win32::{Foundation::*, Graphics::Dwm::*, UI::WindowsAndMessaging::*};

static POPUP: AtomicUsize = AtomicUsize::new(0);
static TARGET: AtomicUsize = AtomicUsize::new(0);
static MOUSE: AtomicBool = AtomicBool::new(false);
static PROC: AtomicIsize = AtomicIsize::new(0);

pub(super) fn hwnd(window: &gpui::Window) -> Option<HWND> {
    match HasWindowHandle::window_handle(window).ok()?.as_raw() {
        RawWindowHandle::Win32(h) => Some(HWND(h.hwnd.get() as *mut _)),
        _ => None,
    }
}

unsafe extern "system" fn proc(h: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    if msg == WM_MOUSEACTIVATE && MOUSE.load(Ordering::SeqCst) {
        return LRESULT(MA_NOACTIVATE as isize);
    }
    unsafe {
        CallWindowProcW(
            std::mem::transmute::<isize, WNDPROC>(PROC.load(Ordering::SeqCst)),
            h,
            msg,
            wp,
            lp,
        )
    }
}

pub fn mouse_mode(window: &gpui::Window, mouse: bool, target: super::Target) {
    let Some(h) = hwnd(window) else { return };
    TARGET.store(target.raw(), Ordering::SeqCst);
    MOUSE.store(mouse, Ordering::SeqCst);
    unsafe {
        if POPUP.swap(h.0 as usize, Ordering::SeqCst) == 0 {
            PROC.store(
                SetWindowLongPtrW(h, GWLP_WNDPROC, proc as *const () as isize),
                Ordering::SeqCst,
            );
        }
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
    if MOUSE.load(Ordering::SeqCst) && h.0 as usize != TARGET.load(Ordering::SeqCst) {
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
