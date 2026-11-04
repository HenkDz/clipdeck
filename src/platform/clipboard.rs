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
static CURRENT: std::sync::Mutex<(Vec<i64>, u32)> = std::sync::Mutex::new((Vec::new(), 0));
static EVENTS: std::sync::OnceLock<async_channel::Sender<Event>> = std::sync::OnceLock::new();
pub fn set_current(id: i64, sequence: u32) {
    set_current_ids(vec![id], sequence);
}
pub fn set_current_ids(ids: Vec<i64>, sequence: u32) {
    if unsafe { GetClipboardSequenceNumber() } == sequence
        && let Ok(mut current) = CURRENT.lock()
    {
        *current = (ids.into_iter().filter(|id| *id > 0).collect(), sequence);
    }
}
pub fn is_current(id: i64) -> bool {
    CURRENT
        .lock()
        .is_ok_and(|c| c.1 == unsafe { GetClipboardSequenceNumber() } && c.0.contains(&id))
}
pub(super) fn send(event: Event) {
    if let Some(sender) = EVENTS.get() {
        let _ = sender.send_blocking(event);
    }
}
fn changed() {
    send(Event::ClipboardChanged);
}
pub fn clipboard_batch_sent(clips: Vec<(i64, Vec<u8>)>, plain: bool) {
    send(Event::PasteBatch { clips, plain });
}
pub fn paste_sent(id: i64, hash: Vec<u8>, plain: bool) {
    send(Event::PasteSent { id, hash, plain });
}
thread_local! { static SENDER: RefCell<Option<async_channel::Sender<Event>>> = const { RefCell::new(None) }; }
thread_local! { static PAUSED: RefCell<Option<Arc<AtomicBool>>> = const { RefCell::new(None) }; }

#[derive(Clone, Copy)]
pub struct Clipboard(usize);

static CLIPBOARD_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
struct Open {
    _lock: std::sync::MutexGuard<'static, ()>,
    owner: HWND,
    opened: bool,
}
impl Open {
    fn finish(mut self) -> u32 {
        // Writes finalize the sequence at close; keep our owner lock until it is read.
        unsafe {
            if CloseClipboard().is_err() {
                return 0;
            }
            self.opened = false;
            let sequence = GetClipboardSequenceNumber();
            if GetClipboardOwner().is_ok_and(|owner| owner == self.owner)
                && GetClipboardSequenceNumber() == sequence
            {
                sequence
            } else {
                0
            }
        }
    }
}
impl Drop for Open {
    fn drop(&mut self) {
        if self.opened {
            unsafe {
                let _ = CloseClipboard();
            }
        }
    }
}

fn open(owner: HWND) -> Result<Open> {
    // Windows permits overlapping opens for the same owner HWND, even across our threads.
    let lock = CLIPBOARD_LOCK.lock().map_err(|_| {
        windows::core::Error::new(
            windows::core::HRESULT(0x80004005u32 as i32),
            "Clipboard access lock failed",
        )
    })?;
    for _ in 0..10 {
        if unsafe { OpenClipboard(Some(owner)) }.is_ok() {
            return Ok(Open {
                _lock: lock,
                owner,
                opened: true,
            });
        }
        thread::sleep(Duration::from_millis(10));
    }
    unsafe {
        OpenClipboard(Some(owner))?;
    }
    Ok(Open {
        _lock: lock,
        owner,
        opened: true,
    })
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
        let captured = super::capture::read()?;
        let sequence = GetClipboardSequenceNumber();
        drop(_guard);
        if let Some((mut clip, raw)) = captured {
            clip.sequence = sequence;
            let sender = SENDER.with(|s| s.borrow().clone());
            // Image decoding and Shell extensions are slow; never hold the clipboard or
            // its message loop while they run.
            thread::spawn(move || {
                if let Some(raw) = raw
                    && let Err(error) = super::capture::finish(&mut clip, raw)
                {
                    if let Some(sender) = &sender {
                        let _ =
                            sender.send_blocking(Event::Error(format!("Clipboard image: {error}")));
                    }
                    return;
                }
                if clip.kind == "files" {
                    super::preview::summarize_files(&mut clip);
                }
                if let Some(sender) = sender {
                    let _ = sender.send_blocking(Event::Clip {
                        clip: Box::new(clip),
                        own_write,
                    });
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
    /// Puts a subset of a file clip on the clipboard, as Explorer's copy would.
    pub fn write_files(self, paths: &[&str]) -> Result<()> {
        let own = format(&marker())?;
        let effect = format("Preferred DropEffect")?;
        let guard = open(HWND(self.0 as *mut _))?;
        unsafe {
            EmptyClipboard()?;
            put(own, &1u32.to_le_bytes())?;
            put(effect, &1u32.to_le_bytes())?;
            put(15, &crate::files::hdrop(paths))?;
        }
        guard.finish();
        changed();
        Ok(())
    }
    pub fn write_clips(
        self,
        clips: &[crate::clip::Clip],
        plain: bool,
    ) -> Result<crate::clip::Clip> {
        let mut batch = if clips.len() == 1 {
            clips[0].clone()
        } else {
            super::batch::prepare(clips, plain).map_err(|e| {
                windows::core::Error::new(windows::core::HRESULT(0x80070057u32 as i32), e)
            })?
        };
        batch.sequence = self.write_clip(&batch, plain)?;
        if batch.sequence == 0 {
            return Err(windows::core::Error::new(
                windows::core::HRESULT(0x80004005u32 as i32),
                "Clipboard copy could not be confirmed; please try again",
            ));
        }
        set_current_ids(clips.iter().map(|c| c.id).collect(), batch.sequence);
        changed();
        // Show the copied markers before waiting for Shell extensions to make the group thumbnail.
        if clips.len() > 1 && batch.kind == "files" {
            super::preview::summarize_files(&mut batch);
        }
        Ok(batch)
    }
    pub fn write(self, text: &str) -> Result<u32> {
        let wide: Vec<u8> = text
            .encode_utf16()
            .chain(Some(0))
            .flat_map(u16::to_le_bytes)
            .collect();
        let own = format(&marker())?;
        let guard = open(HWND(self.0 as *mut _))?;
        unsafe {
            EmptyClipboard()?;
            put(own, &1u32.to_le_bytes())?;
            put(TEXT, &wide)?;
            Ok(guard.finish())
        }
    }
    pub fn write_clip(self, clip: &crate::clip::Clip, plain: bool) -> Result<u32> {
        if plain || (clip.kind != "image" && clip.formats.is_empty()) {
            let sequence = self.write(&clip.text)?;
            set_current(clip.id, sequence);
            changed();
            return Ok(sequence);
        }
        let dib = if clip.kind == "image" {
            Some(super::image::dib_from_png(&clip.payload).map_err(|e| {
                windows::core::Error::new(windows::core::HRESULT(0x80070057u32 as i32), e)
            })?)
        } else {
            None
        };
        let guard = open(HWND(self.0 as *mut _))?;
        unsafe {
            EmptyClipboard()?;
            put(format(&marker())?, &1u32.to_le_bytes())?;
            if let Some(dib) = dib {
                put(format("PNG")?, &clip.payload)?;
                put(8, &dib)?;
            } else {
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
        }
        let sequence = guard.finish();
        set_current(clip.id, sequence);
        changed();
        Ok(sequence)
    }
}

const READ_TIMER: usize = 1;
/// Writers such as .NET's `Clipboard.SetDataObject` (ShareX) publish their data and then
/// reopen the clipboard to render it. Reading at once holds the clipboard while they
/// retry, and their copy fails. Wait until updates have been quiet this long.
const READ_DELAY_MS: u32 = 150;

unsafe extern "system" fn proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    if msg == WM_CLIPBOARDUPDATE {
        // Restarting the timer coalesces bursts of updates into one read.
        unsafe { SetTimer(Some(hwnd), READ_TIMER, READ_DELAY_MS, None) };
        return LRESULT(0);
    }
    if msg == WM_TIMER && wp.0 == READ_TIMER {
        unsafe {
            let _ = KillTimer(Some(hwnd), READ_TIMER);
        }
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
        changed();
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
    let _ = EVENTS.set(sender.clone());
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
