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

pub fn read() -> Result<Option<Clip>> {
    unsafe {
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
            let (png, thumbnail, dimensions) = if IsClipboardFormatAvailable(png_format).is_ok() {
                let png = bytes(png_format)?;
                let (thumbnail, dimensions) = super::image::png_metadata(&png).map_err(|e| {
                    windows::core::Error::new(windows::core::HRESULT(0x80070057u32 as i32), e)
                })?;
                (png, thumbnail, dimensions)
            } else {
                let id = if IsClipboardFormatAvailable(17).is_ok() {
                    17
                } else {
                    8
                };
                super::image::png_from_dib(&bytes(id)?).map_err(|e| {
                    windows::core::Error::new(windows::core::HRESULT(0x80070057u32 as i32), e)
                })?
            };
            clip.kind = "image".into();
            clip.payload = png;
            clip.thumbnail = thumbnail;
            clip.text = dimensions;
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
        Ok((!clip.payload.is_empty()).then_some(clip))
    }
}
