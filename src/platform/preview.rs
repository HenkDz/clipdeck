use std::{io::Cursor, path::Path};
use windows::{
    Win32::{Foundation::*, Graphics::Gdi::*, System::Com::*, UI::Shell::*},
    core::HSTRING,
};

#[derive(Default)]
pub struct Preview {
    pub text: String,
    pub png: Vec<u8>,
    pub file: Option<String>,
}

#[derive(Debug, PartialEq)]
pub enum FileType {
    Image,
    Video,
    Other,
}
pub fn file_type(path: &str) -> FileType {
    match Path::new(path)
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_ascii_lowercase()
        .as_str()
    {
        "png" | "jpg" | "jpeg" | "bmp" | "gif" | "webp" | "tif" | "tiff" => FileType::Image,
        "mp4" | "mov" | "mkv" | "webm" | "avi" | "wmv" => FileType::Video,
        _ => FileType::Other,
    }
}

pub fn thumbnail(path: &str, side: i32) -> Result<Vec<u8>, String> {
    unsafe {
        CoInitializeEx(None, COINIT_APARTMENTTHREADED)
            .ok()
            .map_err(|e| e.to_string())?;
        let result = (|| {
            let factory: IShellItemImageFactory =
                SHCreateItemFromParsingName(&HSTRING::from(path), None)
                    .map_err(|e| e.to_string())?;
            let bitmap = factory
                .GetImage(SIZE { cx: side, cy: side }, SIIGBF_RESIZETOFIT)
                .map_err(|e| e.to_string())?;
            let result = bitmap_png(bitmap);
            let _ = DeleteObject(bitmap.into());
            result
        })();
        CoUninitialize();
        result
    }
}

unsafe fn bitmap_png(bitmap: HBITMAP) -> Result<Vec<u8>, String> {
    unsafe {
        let mut bm = BITMAP::default();
        if GetObjectW(
            bitmap.into(),
            std::mem::size_of::<BITMAP>() as i32,
            Some(&mut bm as *mut _ as _),
        ) == 0
        {
            return Err("Cannot read Shell thumbnail".into());
        }
        if bm.bmWidth <= 0 || bm.bmHeight <= 0 || bm.bmWidth > 4096 || bm.bmHeight > 4096 {
            return Err("Invalid thumbnail dimensions".into());
        }
        let mut info = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: bm.bmWidth,
                biHeight: -bm.bmHeight,
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            ..Default::default()
        };
        let mut bytes = vec![0u8; bm.bmWidth as usize * bm.bmHeight as usize * 4];
        let dc = GetDC(None);
        let lines = GetDIBits(
            dc,
            bitmap,
            0,
            bm.bmHeight as u32,
            Some(bytes.as_mut_ptr().cast()),
            &mut info,
            DIB_RGB_COLORS,
        );
        ReleaseDC(None, dc);
        if lines == 0 {
            return Err("Cannot decode Shell thumbnail".into());
        }
        for pixel in bytes.as_chunks_mut::<4>().0 {
            pixel.swap(0, 2);
            pixel[3] = 255;
        }
        let image = image::RgbaImage::from_raw(bm.bmWidth as u32, bm.bmHeight as u32, bytes)
            .ok_or("Invalid thumbnail")?;
        let mut out = Cursor::new(Vec::new());
        image
            .write_to(&mut out, image::ImageFormat::Png)
            .map_err(|e| e.to_string())?;
        Ok(out.into_inner())
    }
}

pub fn load(id: i64) -> Result<Preview, String> {
    let path = crate::config::directory()
        .map_err(|e| e.to_string())?
        .join("history.db");
    let db =
        rusqlite::Connection::open_with_flags(path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
            .map_err(|e| e.to_string())?;
    let (kind, text, payload): (String, String, Vec<u8>) = db
        .query_row(
            "SELECT kind,text,coalesce(payload,cast(text as blob)) FROM clips WHERE id=?1",
            [id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .map_err(|e| e.to_string())?;
    let mut preview = Preview::default();
    match kind.as_str() {
        "image" => {
            let reader = image::ImageReader::new(Cursor::new(&payload))
                .with_guessed_format()
                .map_err(|e| e.to_string())?;
            let (w, h) = reader.into_dimensions().map_err(|e| e.to_string())?;
            preview.text = format!("{w} × {h} · {} bytes", payload.len());
            preview.png = payload;
        }
        "files" => {
            let file = text.lines().next().ok_or("Empty file list")?;
            let size = std::fs::metadata(file).map_err(|e| e.to_string())?.len();
            preview.text = format!(
                "{file}\n{size} bytes · {:?}\n{}",
                file_type(file),
                if text.lines().count() > 1 {
                    "Showing first file"
                } else {
                    ""
                }
            );
            preview.png = thumbnail(file, 1600).unwrap_or_default();
            if file_type(file) == FileType::Video
                && let Some(duration) = duration(file)
            {
                preview.text.push_str(&format!(
                    "\nDuration: {}:{:02}",
                    duration / 60,
                    duration % 60
                ));
            }
            if preview.png.is_empty() {
                preview.text.push_str("\nNo Shell thumbnail available");
            }
            preview.file = Some(file.to_owned());
        }
        _ => preview.text = text,
    }
    Ok(preview)
}

fn duration(path: &str) -> Option<u64> {
    unsafe {
        CoInitializeEx(None, COINIT_APARTMENTTHREADED).ok().ok()?;
        let result = (|| {
            let item: IShellItem2 = SHCreateItemFromParsingName(&HSTRING::from(path), None).ok()?;
            let mut key = PROPERTYKEY::default();
            PropertiesSystem::PSGetPropertyKeyFromName(
                windows::core::w!("System.Media.Duration"),
                &mut key,
            )
            .ok()?;
            item.GetUInt64(&key)
                .ok()
                .filter(|v| *v > 0)
                .map(|v| v / 10_000_000)
        })();
        CoUninitialize();
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn routes_extensions_case_insensitively() {
        for path in ["a.MP4", "b.mov", "c.mkv", "a.webm", "a.avi", "a.wmv"] {
            assert_eq!(file_type(path), FileType::Video);
        }
        assert_eq!(file_type("C:\\foo.PNG"), FileType::Image);
        assert_eq!(file_type("archive.mp4.txt"), FileType::Other);
        assert_eq!(file_type("no-extension"), FileType::Other);
    }
}
