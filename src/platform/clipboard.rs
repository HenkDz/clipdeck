use super::Event;
use std::{
    cell::RefCell,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    thread,
    time::Duration,
};
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
thread_local! { static PAUSED: RefCell<Option<Arc<AtomicBool>>> = const { RefCell::new(None) }; }

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

pub(super) fn format(name: &str) -> Result<u32> {
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

pub(super) unsafe fn bytes(id: u32) -> Result<Vec<u8>> {
    unsafe {
        let handle = GetClipboardData(id)?;
        let global = HGLOBAL(handle.0);
        let size = GlobalSize(global);
        if size > 128 * 1024 * 1024 {
            return Err(windows::core::Error::new(
                windows::core::HRESULT(0x80070057u32 as i32),
                "Clipboard format exceeds 128 MiB",
            ));
        }
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
        let own_write = IsClipboardFormatAvailable(format(&marker())?).is_ok();
        if own_write {
            return Ok(None);
        }
        let clip = super::capture::read()?;
        drop(_guard);
        if let Some(mut clip) = clip {
            let sender = SENDER.with(|s| s.borrow().clone());
            // Shell extensions may block; never hold the clipboard or its message loop.
            thread::spawn(move || {
                if clip.kind == "files"
                    && !matches!(
                        super::preview::file_type(clip.text.lines().next().unwrap_or("")),
                        super::preview::FileType::Other
                    )
                {
                    clip.thumbnail =
                        super::preview::thumbnail(clip.text.lines().next().unwrap_or(""), 96)
                            .unwrap_or_default();
                }
                if let Some(sender) = sender {
                    let _ = sender.send_blocking(Event::Clip { clip, own_write });
                }
            });
        }
        Ok(None)
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
    pub fn write_clip(self, clip: &crate::clip::Clip, plain: bool) -> Result<()> {
        if plain || clip.formats.is_empty() {
            return self.write(&clip.text);
        }
        let _guard = open(HWND(self.0 as *mut _))?;
        unsafe {
            EmptyClipboard()?;
            put(format(&marker())?, &1u32.to_le_bytes())?;
            for f in &clip.formats {
                let id = match f.name.as_str() {
                    "CF_UNICODETEXT" => 13,
                    "CF_DIB" => 8,
                    "CF_DIBV5" => 17,
                    "CF_HDROP" => 15,
                    name => format(name)?,
                };
                put(id, &f.data)?;
            }
        }
        Ok(())
    }
}

unsafe extern "system" fn proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    if msg == WM_CLIPBOARDUPDATE {
        if PAUSED.with(|p| {
            p.borrow()
                .as_ref()
                .is_some_and(|p| p.load(Ordering::SeqCst))
        }) {
            return LRESULT(0);
        }
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

pub fn start_watcher(
    sender: async_channel::Sender<Event>,
    paused: Arc<AtomicBool>,
) -> Result<Clipboard> {
    let (ready, result) = mpsc::sync_channel(1);
    thread::spawn(move || unsafe {
        SENDER.with(|s| *s.borrow_mut() = Some(sender.clone()));
        PAUSED.with(|s| *s.borrow_mut() = Some(paused.clone()));
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
                super::foreground::watch();
                let _tray = match super::tray::create(sender.clone(), paused) {
                    Ok(tray) => Some(tray),
                    Err(e) => {
                        let _ = sender.send_blocking(Event::Error(format!("Tray: {e}")));
                        None
                    }
                };
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
#[path = "clipboard_tests.rs"]
mod tests;
