use super::Event;
use std::{cell::RefCell, sync::mpsc, thread, time::Duration};
use windows::{
    Win32::{
        Foundation::*,
        System::{DataExchange::*, LibraryLoader::GetModuleHandleW, Memory::*},
        UI::WindowsAndMessaging::*,
    },
    core::{HSTRING, PCWSTR, Result, w},
};

const TEXT: u32 = 13;
thread_local! { static SENDER: RefCell<Option<async_channel::Sender<Event>>> = const { RefCell::new(None) }; }

#[derive(Clone, Copy)]
pub struct Clipboard(usize);

struct Open;
impl Drop for Open {
    fn drop(&mut self) {
        unsafe {
            let _ = CloseClipboard();
        }
    }
}

fn open(owner: HWND) -> Result<Open> {
    for _ in 0..10 {
        if unsafe { OpenClipboard(Some(owner)) }.is_ok() {
            return Ok(Open);
        }
        thread::sleep(Duration::from_millis(10));
    }
    unsafe {
        OpenClipboard(Some(owner))?;
    }
    Ok(Open)
}

fn format(name: &str) -> Result<u32> {
    let id = unsafe { RegisterClipboardFormatW(PCWSTR(HSTRING::from(name).as_ptr())) };
    if id == 0 {
        Err(windows::core::Error::from_win32())
    } else {
        Ok(id)
    }
}

fn marker() -> String {
    format!("Clipdeck.OwnWrite.{}", std::process::id())
}

unsafe fn bytes(id: u32) -> Result<Vec<u8>> {
    unsafe {
        let handle = GetClipboardData(id)?;
        let global = HGLOBAL(handle.0);
        let size = GlobalSize(global);
        let ptr = GlobalLock(global);
        if ptr.is_null() {
            return Err(windows::core::Error::from_win32());
        }
        let data = std::slice::from_raw_parts(ptr.cast::<u8>(), size).to_vec();
        let _ = GlobalUnlock(global);
        Ok(data)
    }
}

fn read(owner: HWND) -> Result<Option<Event>> {
    let _guard = open(owner)?;
    unsafe {
        for name in [
            "ExcludeClipboardContentFromMonitorProcessing",
            "Clipboard Viewer Ignore",
        ] {
            if IsClipboardFormatAvailable(format(name)?).is_ok() {
                return Ok(None);
            }
        }
        let history = format("CanIncludeInClipboardHistory")?;
        if IsClipboardFormatAvailable(history).is_ok() {
            let data = bytes(history)?;
            // Fail closed on malformed privacy metadata as well as DWORD zero.
            if data.len() < 4 || u32::from_le_bytes(data[..4].try_into().unwrap()) == 0 {
                return Ok(None);
            }
        }
        if IsClipboardFormatAvailable(TEXT).is_err() {
            return Ok(None);
        }
        let own_write = IsClipboardFormatAvailable(format(&marker())?).is_ok();
        let data = bytes(TEXT)?;
        let wide: Vec<u16> = data
            .as_chunks::<2>()
            .0
            .iter()
            .map(|b| u16::from_le_bytes(*b))
            .take_while(|c| *c != 0)
            .collect();
        Ok(Some(Event::Clip {
            text: String::from_utf16_lossy(&wide),
            own_write,
        }))
    }
}

unsafe fn put(id: u32, data: &[u8]) -> Result<()> {
    unsafe {
        let memory = GlobalAlloc(GMEM_MOVEABLE, data.len())?;
        let ptr = GlobalLock(memory);
        if ptr.is_null() {
            let _ = GlobalFree(Some(memory));
            return Err(windows::core::Error::from_win32());
        }
        std::ptr::copy_nonoverlapping(data.as_ptr(), ptr.cast(), data.len());
        let _ = GlobalUnlock(memory);
        if let Err(error) = SetClipboardData(id, Some(HANDLE(memory.0))) {
            let _ = GlobalFree(Some(memory));
            return Err(error);
        }
        Ok(())
    }
}

impl Clipboard {
    pub fn write(self, text: &str) -> Result<()> {
        let wide: Vec<u8> = text
            .encode_utf16()
            .chain(Some(0))
            .flat_map(u16::to_le_bytes)
            .collect();
        let own = format(&marker())?;
        let _guard = open(HWND(self.0 as *mut _))?;
        unsafe {
            EmptyClipboard()?;
            put(own, &1u32.to_le_bytes())?;
            put(TEXT, &wide)
        }
    }
}

unsafe extern "system" fn proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    if msg == WM_CLIPBOARDUPDATE {
        let event = match read(hwnd) {
            Ok(event) => event,
            Err(e) => Some(Event::Error(format!("Clipboard read: {e}"))),
        };
        if let Some(event) = event {
            SENDER.with(|s| {
                if let Some(sender) = s.borrow().as_ref() {
                    let _ = sender.send_blocking(event);
                }
            });
        }
        return LRESULT(0);
    }
    unsafe { DefWindowProcW(hwnd, msg, wp, lp) }
}

pub fn start_watcher(sender: async_channel::Sender<Event>) -> Result<Clipboard> {
    let (ready, result) = mpsc::sync_channel(1);
    thread::spawn(move || unsafe {
        SENDER.with(|s| *s.borrow_mut() = Some(sender));
        let setup = || -> Result<HWND> {
            let instance = GetModuleHandleW(None)?;
            let class = WNDCLASSW {
                lpfnWndProc: Some(proc),
                hInstance: instance.into(),
                lpszClassName: w!("ClipdeckClipboard"),
                ..Default::default()
            };
            if RegisterClassW(&class) == 0 {
                return Err(windows::core::Error::from_win32());
            }
            let hwnd = CreateWindowExW(
                WINDOW_EX_STYLE::default(),
                class.lpszClassName,
                w!(""),
                WINDOW_STYLE::default(),
                0,
                0,
                0,
                0,
                Some(HWND_MESSAGE),
                None,
                Some(instance.into()),
                None,
            )?;
            AddClipboardFormatListener(hwnd)?;
            Ok(hwnd)
        };
        match setup() {
            Ok(hwnd) => {
                let _ = ready.send(Ok(Clipboard(hwnd.0 as usize)));
                let mut msg = MSG::default();
                while GetMessageW(&mut msg, None, 0, 0).as_bool() {
                    let _ = TranslateMessage(&msg);
                    DispatchMessageW(&msg);
                }
                let _ = RemoveClipboardFormatListener(hwnd);
                let _ = DestroyWindow(hwnd);
            }
            Err(e) => {
                let _ = ready.send(Err(e));
            }
        }
    });
    result
        .recv()
        .map_err(|_| windows::core::Error::from_win32())?
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "writes the real Windows clipboard; run with the app stopped"]
    fn live_listener_privacy_formats_and_own_marker() {
        let (sender, receiver) = async_channel::unbounded();
        let clipboard = start_watcher(sender).unwrap();
        let owner = HWND(clipboard.0 as *mut _);
        let cases = [
            (None, None, true),
            (
                Some("ExcludeClipboardContentFromMonitorProcessing"),
                Some(1u32),
                false,
            ),
            (Some("Clipboard Viewer Ignore"), Some(1u32), false),
            (Some("CanIncludeInClipboardHistory"), Some(0u32), false),
            (Some("CanIncludeInClipboardHistory"), Some(1u32), true),
        ];
        for (name, flag, recorded) in cases {
            while receiver.try_recv().is_ok() {}
            {
                let _guard = open(owner).unwrap();
                unsafe {
                    EmptyClipboard().unwrap();
                    if let Some(name) = name {
                        put(format(name).unwrap(), &flag.unwrap().to_le_bytes()).unwrap();
                    }
                    let text: Vec<u8> = "Clipdeck privacy fixture"
                        .encode_utf16()
                        .chain(Some(0))
                        .flat_map(u16::to_le_bytes)
                        .collect();
                    put(TEXT, &text).unwrap();
                }
            }
            thread::sleep(Duration::from_millis(250));
            let event = receiver.try_recv();
            if recorded {
                assert!(
                    matches!(
                        event,
                        Ok(Event::Clip {
                            own_write: false,
                            ..
                        })
                    ),
                    "{name:?}"
                );
            } else {
                assert!(event.is_err(), "excluded format {name:?} emitted an event");
            }
        }
        clipboard.write("Clipdeck own write fixture").unwrap();
        thread::sleep(Duration::from_millis(250));
        assert!(matches!(
            receiver.try_recv(),
            Ok(Event::Clip {
                own_write: true,
                ..
            })
        ));
    }
}
