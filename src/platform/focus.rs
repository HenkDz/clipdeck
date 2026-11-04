use std::{thread, time::Duration};
use windows::{
    Win32::{
        Foundation::*,
        System::Threading::{AttachThreadInput, GetCurrentThreadId},
        UI::{Input::KeyboardAndMouse::*, WindowsAndMessaging::*},
    },
    core::Result,
};

#[derive(Clone, Copy)]
pub struct Target(usize);

impl Target {
    pub(super) fn raw(self) -> usize {
        self.0
    }
    pub fn is_app(self) -> bool {
        let mut pid = 0;
        unsafe {
            GetWindowThreadProcessId(HWND(self.0 as *mut _), Some(&mut pid));
        }
        pid == std::process::id()
    }
    pub fn capture() -> Self {
        Self(unsafe { GetForegroundWindow() }.0 as usize)
    }
    pub(super) fn from_hwnd(hwnd: HWND) -> Self {
        Self(hwnd.0 as usize)
    }
    pub(super) fn restore(self) -> bool {
        unsafe {
            let hwnd = HWND(self.0 as *mut _);
            if !IsWindow(Some(hwnd)).as_bool() {
                return false;
            }
            if GetForegroundWindow() == hwnd {
                return true;
            }
            if SetForegroundWindow(hwnd).as_bool() {
                return GetForegroundWindow() == hwnd;
            }
            let current = GetCurrentThreadId();
            let foreground = GetWindowThreadProcessId(GetForegroundWindow(), None);
            if foreground != current && AttachThreadInput(current, foreground, true).as_bool() {
                let _ = SetForegroundWindow(hwnd);
                let _ = AttachThreadInput(current, foreground, false);
            }
            GetForegroundWindow() == hwnd
        }
    }

    pub fn paste(self, mouse: bool) -> Result<()> {
        unsafe {
            let hwnd = HWND(self.0 as *mut _);
            if !IsWindow(Some(hwnd)).as_bool() {
                return Err(windows::core::Error::new(
                    windows::core::HRESULT(0x80004005u32 as i32),
                    "Paste target closed",
                ));
            }
            // The hotkey/Enter is user input; direct foreground activation is attempted first.
            let restored = if mouse {
                GetForegroundWindow() == hwnd
            } else {
                self.restore()
            };
            if !restored || GetForegroundWindow() != hwnd {
                return Err(windows::core::Error::new(
                    windows::core::HRESULT(0x80004005u32 as i32),
                    "Windows refused foreground activation",
                ));
            }
            // Never release keys belonging to the user; wait for the opening chord to finish.
            let keys_up = || {
                [VK_CONTROL, VK_SHIFT, VK_MENU, VK_RETURN, VIRTUAL_KEY(0x56)]
                    .iter()
                    .all(|key| GetAsyncKeyState(i32::from(key.0)) >= 0)
            };
            for _ in 0..100 {
                if keys_up() {
                    break;
                }
                thread::sleep(Duration::from_millis(10));
            }
            if !keys_up() || GetForegroundWindow() != hwnd {
                return Err(windows::core::Error::new(
                    windows::core::HRESULT(0x80004005u32 as i32),
                    "Paste cancelled: held keys or changed foreground",
                ));
            }
            let input = |key, flags| INPUT {
                r#type: INPUT_KEYBOARD,
                Anonymous: INPUT_0 {
                    ki: KEYBDINPUT {
                        wVk: key,
                        dwFlags: flags,
                        ..Default::default()
                    },
                },
            };
            let inputs = [
                input(VK_CONTROL, KEYBD_EVENT_FLAGS(0)),
                input(VIRTUAL_KEY(0x56), KEYBD_EVENT_FLAGS(0)),
                input(VIRTUAL_KEY(0x56), KEYEVENTF_KEYUP),
                input(VK_CONTROL, KEYEVENTF_KEYUP),
            ];
            if SendInput(&inputs, std::mem::size_of::<INPUT>() as i32) != inputs.len() as u32 {
                return Err(windows::core::Error::from_win32());
            }
            Ok(())
        }
    }
}

// Raw native handles are obtained and consumed only at this platform boundary.
pub fn configure_popup(window: &gpui::Window) {
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    if let Ok(handle) = HasWindowHandle::window_handle(window)
        && let RawWindowHandle::Win32(handle) = handle.as_raw()
    {
        unsafe {
            let hwnd = HWND(handle.hwnd.get() as *mut _);
            use windows::Win32::{Graphics::Gdi::*, UI::HiDpi::*};
            let style = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
            SetWindowLongPtrW(
                hwnd,
                GWL_EXSTYLE,
                (style | WS_EX_TOOLWINDOW.0 as isize) & !(WS_EX_APPWINDOW.0 as isize),
            );
            // Resizing stays available; maximize/minimize do not apply to the popup.
            let style = GetWindowLongPtrW(hwnd, GWL_STYLE);
            SetWindowLongPtrW(
                hwnd,
                GWL_STYLE,
                (style | WS_THICKFRAME.0 as isize)
                    & !((WS_MAXIMIZEBOX.0 | WS_MINIMIZEBOX.0) as isize),
            );
            let geometry = super::frame::geometry();
            let remembered = geometry.position.filter(|_| super::frame::keep());
            let mut cursor = POINT::default();
            let _ = GetCursorPos(&mut cursor);
            let anchor = remembered.map_or(cursor, |(x, y)| POINT { x, y });
            let monitor = MonitorFromPoint(anchor, MONITOR_DEFAULTTONEAREST);
            let mut info = MONITORINFO {
                cbSize: std::mem::size_of::<MONITORINFO>() as u32,
                ..Default::default()
            };
            if !GetMonitorInfoW(monitor, &mut info).as_bool() {
                return;
            }
            let mut dx = 96;
            let mut dy = 96;
            let _ = GetDpiForMonitor(monitor, MDT_EFFECTIVE_DPI, &mut dx, &mut dy);
            let r = info.rcWork;
            let work = super::placement::Rect {
                left: r.left,
                top: r.top,
                right: r.right,
                bottom: r.bottom,
            };
            let size = (
                geometry.size.0 as i32 * dx as i32 / 96,
                geometry.size.1 as i32 * dy as i32 / 96,
            );
            let placement = match remembered {
                Some(origin) => super::placement::fit(origin, size, work),
                None => super::placement::clamp((cursor.x, cursor.y), size, work),
            };
            super::frame::place(
                hwnd,
                RECT {
                    left: placement.left,
                    top: placement.top,
                    right: placement.right,
                    bottom: placement.bottom,
                },
            );
        }
    }
}

pub fn hide_popup(window: &gpui::Window) {
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    if let Ok(handle) = HasWindowHandle::window_handle(window)
        && let RawWindowHandle::Win32(handle) = handle.as_raw()
    {
        unsafe {
            let _ = ShowWindow(HWND(handle.hwnd.get() as *mut _), SW_HIDE);
        }
    }
}
