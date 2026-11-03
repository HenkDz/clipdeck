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
    pub fn capture() -> Self {
        Self(unsafe { GetForegroundWindow() }.0 as usize)
    }

    pub fn paste(self) -> Result<()> {
        unsafe {
            let hwnd = HWND(self.0 as *mut _);
            if !IsWindow(Some(hwnd)).as_bool() {
                return Err(windows::core::Error::new(
                    windows::core::HRESULT(0x80004005u32 as i32),
                    "Paste target closed",
                ));
            }
            // The hotkey/Enter is user input; direct foreground activation is attempted first.
            let mut restored = SetForegroundWindow(hwnd).as_bool();
            eprintln!("Direct foreground restore: {restored}");
            if !restored {
                let current = GetCurrentThreadId();
                let foreground = GetWindowThreadProcessId(GetForegroundWindow(), None);
                if foreground != current && AttachThreadInput(current, foreground, true).as_bool() {
                    restored = SetForegroundWindow(hwnd).as_bool();
                    eprintln!("Attached foreground restore: {restored}");
                    let _ = AttachThreadInput(current, foreground, false);
                }
            }
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

pub fn cursor() -> (f32, f32) {
    let mut point = POINT::default();
    unsafe {
        let _ = GetCursorPos(&mut point);
    }
    (point.x as f32, point.y as f32)
}

// Raw native handles are obtained and consumed only at this platform boundary.
pub fn configure_popup(window: &gpui::Window) {
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    if let Ok(handle) = HasWindowHandle::window_handle(window)
        && let RawWindowHandle::Win32(handle) = handle.as_raw()
    {
        unsafe {
            let hwnd = HWND(handle.hwnd.get() as *mut _);
            let (x, y) = cursor();
            let _ = SetWindowPos(
                hwnd,
                Some(HWND_TOPMOST),
                x as i32,
                y as i32,
                0,
                0,
                SWP_NOSIZE | SWP_SHOWWINDOW,
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
