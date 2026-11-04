use super::clipboard::{bytes, format};
use crate::{
    clip::{Clip, Format},
    files::Route,
};
use windows::{
    Win32::{System::DataExchange::*, UI::Shell::*},
    core::Result,
};

/// The CF_HDROP file list, or `Err(())` when it exceeds the capture limits.
unsafe fn drop_files() -> std::result::Result<Option<Vec<String>>, ()> {
    unsafe {
        if IsClipboardFormatAvailable(15).is_err() {
            return Ok(None);
        }
        let Ok(handle) = GetClipboardData(15) else {
            return Ok(None);
        };
        let drop = HDROP(handle.0);
        let count = DragQueryFileW(drop, u32::MAX, None);
        if count > 10000 {
            return Err(());
        }
        let mut files = vec![];
        for index in 0..count {
            let len = DragQueryFileW(drop, index, None);
            if len > 32768 {
                return Err(());
            }
            let mut name = vec![0u16; len as usize + 1];
            DragQueryFileW(drop, index, Some(&mut name));
            files.push(String::from_utf16_lossy(&name[..len as usize]));
        }
        Ok(Some(files))
    }
}

/// Image bytes copied while the clipboard was open, still to be decoded by [`finish`].
pub enum RawImage {
    Png,
    Dib,
}

/// Reads the clipboard's content. Call with the clipboard open; it only copies bytes,
/// so the clipboard can be closed again quickly.
pub fn read() -> Result<Option<(Clip, Option<RawImage>)>> {
    unsafe {
        let mut raw = None;
        let mut clip = Clip::text(String::new(), super::source::name());
        let png_format = format("PNG")?;
        let has_image = [png_format, 17, 8]
            .into_iter()
            .any(|id| IsClipboardFormatAvailable(id).is_ok());
        let Ok(files) = drop_files() else {
            return Ok(None);
        };
        let route = crate::files::route(
            files.as_deref(),
            has_image,
            IsClipboardFormatAvailable(13).is_ok(),
        );
        if route == Route::Files {
            let files = files.unwrap_or_default();
            clip.kind = "files".into();
            clip.file_count = files.len();
            clip.text = files.join("\n");
            clip.payload = clip.text.as_bytes().to_vec();
            clip.formats.push(Format {
                name: "CF_HDROP".into(),
                data: bytes(15)?,
            });
        } else if route == Route::Image {
            // Only copy bytes here; decoding waits until the clipboard is closed.
            raw = Some(if IsClipboardFormatAvailable(png_format).is_ok() {
                clip.payload = bytes(png_format)?;
                RawImage::Png
            } else {
                let id = if IsClipboardFormatAvailable(17).is_ok() {
                    17
                } else {
                    8
                };
                clip.payload = bytes(id)?;
                RawImage::Dib
            });
            clip.kind = "image".into();
            if let Some(file) = files.as_ref().and_then(|f| f.first()) {
                clip.file_path = file.clone();
            } else {
                let id = format("FileNameW")?;
                if IsClipboardFormatAvailable(id).is_ok() {
                    clip.file_path = String::from_utf16_lossy(
                        &bytes(id)?
                            .as_chunks::<2>()
                            .0
                            .iter()
                            .map(|b| u16::from_le_bytes(*b))
                            .take_while(|c| *c != 0)
                            .collect::<Vec<_>>(),
                    );
                }
            }
        } else if route == Route::Text {
            let data = bytes(13)?;
            let wide: Vec<u16> = data
                .as_chunks::<2>()
                .0
                .iter()
                .map(|b| u16::from_le_bytes(*b))
                .take_while(|c| *c != 0)
                .collect();
            clip = Clip::text(String::from_utf16_lossy(&wide), clip.source);
            clip.formats.push(Format {
                name: "CF_UNICODETEXT".into(),
                data,
            });
            for name in ["HTML Format", "Rich Text Format"] {
                let id = format(name)?;
                if IsClipboardFormatAvailable(id).is_ok() {
                    clip.formats.push(Format {
                        name: name.into(),
                        data: bytes(id)?,
                    });
                }
            }
        } else {
            return Ok(None);
        }
        clip.source_exe = super::source::exe_name();
        Ok((!clip.payload.is_empty()).then_some((clip, raw)))
    }
}

/// Turns copied image bytes into the stored PNG, thumbnail and dimensions.
/// Runs after the clipboard is closed, off the listener's message loop.
pub fn finish(clip: &mut Clip, raw: RawImage) -> std::result::Result<(), String> {
    let (png, thumbnail, dimensions) = match raw {
        RawImage::Png => {
            let (thumbnail, dimensions) = super::image::png_metadata(&clip.payload)?;
            (std::mem::take(&mut clip.payload), thumbnail, dimensions)
        }
        RawImage::Dib => super::image::png_from_dib(&clip.payload)?,
    };
    clip.payload = png;
    clip.thumbnail = thumbnail;
    clip.text = dimensions;
    Ok(())
}
