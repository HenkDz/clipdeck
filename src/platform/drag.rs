//! Dragging clips out of the popup into other apps, through the Shell's data object
//! and drag loop (its default drop source; no COM classes of our own).
use super::clipboard::format;
use crate::clip::Format;
use std::{cell::RefCell, mem::ManuallyDrop, path::PathBuf};
use windows::{
    Win32::{
        Foundation::*,
        System::{
            Com::{
                DVASPECT_CONTENT, FORMATETC, IDataObject, STGMEDIUM, STGMEDIUM_0, TYMED_HGLOBAL,
            },
            Memory::*,
            Ole::{DROPEFFECT, DROPEFFECT_COPY, IDropSource},
        },
        UI::{Shell::*, WindowsAndMessaging::*},
    },
    core::Result,
};

/// What a drag carries; formats mirror what a paste of the same clip writes.
pub enum Payload {
    /// Unicode text plus the clip's original rich formats.
    Text {
        text: String,
        formats: Vec<Format>,
    },
    Files(Vec<String>),
    /// PNG and a bitmap, plus the image as a file for targets that only take files.
    Image {
        png: Vec<u8>,
        file: PathBuf,
    },
}

struct Pending {
    data: IDataObject,
    id: i64,
    hash: Vec<u8>,
}

thread_local! {
    static PENDING: RefCell<Option<Pending>> = const { RefCell::new(None) };
}

pub(super) const WM_DRAG: u32 = WM_APP + 0x32;

fn hglobal(data: &[u8]) -> Result<HGLOBAL> {
    unsafe {
        let memory = GlobalAlloc(GMEM_MOVEABLE, data.len().max(1))?;
        let ptr = GlobalLock(memory);
        if ptr.is_null() {
            let _ = GlobalFree(Some(memory));
            return Err(windows::core::Error::from_win32());
        }
        std::ptr::copy_nonoverlapping(data.as_ptr(), ptr.cast(), data.len());
        let _ = GlobalUnlock(memory);
        Ok(memory)
    }
}

fn set(data: &IDataObject, id: u32, bytes: &[u8]) -> Result<()> {
    let format = FORMATETC {
        cfFormat: id as u16,
        ptd: std::ptr::null_mut(),
        dwAspect: DVASPECT_CONTENT.0,
        lindex: -1,
        tymed: TYMED_HGLOBAL.0 as u32,
    };
    let medium = STGMEDIUM {
        tymed: TYMED_HGLOBAL.0 as u32,
        u: STGMEDIUM_0 {
            hGlobal: hglobal(bytes)?,
        },
        pUnkForRelease: ManuallyDrop::new(None),
    };
    // The data object takes ownership of the memory.
    unsafe { data.SetData(&format, &medium, true) }
}

fn data_object(payload: &Payload) -> Result<IDataObject> {
    let data: IDataObject = unsafe { SHCreateDataObject(None, None, None::<&IDataObject>)? };
    let files = |paths: &[&str]| -> Result<()> {
        set(
            &data,
            format("Preferred DropEffect")?,
            &DROPEFFECT_COPY.0.to_le_bytes(),
        )?;
        set(&data, 15, &crate::files::hdrop(paths))
    };
    match payload {
        Payload::Text { text, formats } => {
            let wide: Vec<u8> = text
                .encode_utf16()
                .chain(Some(0))
                .flat_map(u16::to_le_bytes)
                .collect();
            set(&data, 13, &wide)?;
            for f in formats.iter().filter(|f| f.name != "CF_UNICODETEXT") {
                set(&data, format(&f.name)?, &f.data)?;
            }
        }
        Payload::Files(paths) => files(&paths.iter().map(String::as_str).collect::<Vec<_>>())?,
        Payload::Image { png, file } => {
            files(&[&file.to_string_lossy()])?;
            set(&data, format("PNG")?, png)?;
            if let Ok(dib) = super::image::dib_from_png(png) {
                set(&data, 8, &dib)?;
            }
        }
    }
    Ok(data)
}

/// Prepares a drag of clip `id` and starts it from the popup's message loop, outside
/// GPUI's event handling, while the mouse button is still down.
pub fn start(
    window: &gpui::Window,
    payload: Payload,
    id: i64,
    hash: Vec<u8>,
) -> std::result::Result<(), String> {
    let h = super::frame::hwnd(window).ok_or("No popup window")?;
    let data = data_object(&payload).map_err(|e| e.to_string())?;
    PENDING.with(|p| *p.borrow_mut() = Some(Pending { data, id, hash }));
    unsafe { PostMessageW(Some(h), WM_DRAG, WPARAM(0), LPARAM(0)) }.map_err(|e| e.to_string())
}

/// Runs the modal drag loop; a drop counts like a paste.
pub(super) fn run(h: HWND) {
    let Some(pending) = PENDING.with(|p| p.borrow_mut().take()) else {
        return;
    };
    let effect: Result<DROPEFFECT> = unsafe {
        SHDoDragDrop(
            Some(h),
            &pending.data,
            None::<&IDropSource>,
            DROPEFFECT_COPY,
        )
    };
    if let Ok(effect) = effect
        && effect.0 & DROPEFFECT_COPY.0 != 0
    {
        super::clipboard::send(super::Event::Dropped {
            id: pending.id,
            hash: pending.hash,
        });
    }
}

/// The image as a file: its original if it still exists, else a PNG in the temp folder
/// named after its capture time, which is what apps show for the dropped file.
pub fn image_file(clip: &crate::clip::Clip) -> std::result::Result<PathBuf, String> {
    let original = PathBuf::from(&clip.file_path);
    if !clip.file_path.is_empty() && original.is_file() {
        return Ok(original);
    }
    let dir = std::env::temp_dir().join("Clipdeck-drag");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let file = dir.join(format!(
        "Clipdeck {}.png",
        super::time::stamp(clip.created)?
    ));
    if !file.is_file() {
        std::fs::write(&file, &clip.payload).map_err(|e| e.to_string())?;
    }
    Ok(file)
}
