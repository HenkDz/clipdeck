use super::clipboard::{bytes, format};
use crate::clip::{Clip, Format};
use windows::{
    Win32::{System::DataExchange::*, UI::Shell::*},
    core::Result,
};

pub fn read() -> Result<Option<Clip>> {
    unsafe {
        let mut clip = Clip::text(String::new(), super::source::name());
        if IsClipboardFormatAvailable(15).is_ok() {
            let handle = GetClipboardData(15)?;
            let drop = HDROP(handle.0);
            let count = DragQueryFileW(drop, u32::MAX, None);
            if count > 10000 {
                return Ok(None);
            }
            let mut files = vec![];
            for index in 0..count {
                let len = DragQueryFileW(drop, index, None);
                if len > 32768 {
                    return Ok(None);
                }
                let mut name = vec![0u16; len as usize + 1];
                DragQueryFileW(drop, index, Some(&mut name));
                files.push(String::from_utf16_lossy(&name[..len as usize]));
            }
            clip.kind = "files".into();
            clip.file_count = count as usize;
            clip.text = files.join("\n");
            clip.payload = clip.text.as_bytes().to_vec();
            clip.formats.push(Format {
                name: "CF_HDROP".into(),
                data: bytes(15)?,
            });
        } else if let Some(id) = [17, 8]
            .into_iter()
            .find(|id| IsClipboardFormatAvailable(*id).is_ok())
        {
            let dib = bytes(id)?;
            match super::image::png_from_dib(&dib) {
                Ok((png, thumbnail, dimensions)) => {
                    clip.kind = "image".into();
                    clip.payload = png;
                    clip.thumbnail = thumbnail;
                    clip.text = dimensions;
                    clip.formats.push(Format {
                        name: if id == 17 { "CF_DIBV5" } else { "CF_DIB" }.into(),
                        data: dib,
                    });
                }
                Err(error) => {
                    eprintln!("Image capture: {error}");
                    return Ok(None);
                }
            }
        } else if IsClipboardFormatAvailable(13).is_ok() {
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
        Ok((!clip.payload.is_empty()).then_some(clip))
    }
}
