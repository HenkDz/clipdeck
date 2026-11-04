use std::{
    io::Cursor,
    path::{Path, PathBuf},
};
use windows::{
    Win32::{Foundation::*, Graphics::Gdi::*, System::Com::*, UI::Shell::*},
    core::HSTRING,
};

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

pub fn load(id: i64, location: bool) -> Result<Option<String>, String> {
    let clip = super::locations::load_clip(id)?;
    let paths = match clip.kind.as_str() {
        "image" => vec![
            super::locations::resolve_image(&clip, true)?
                .ok_or("Cannot locate this image")?
                .path,
        ],
        "files" => clip.text.lines().map(PathBuf::from).collect(),
        _ if location => return Err("This clip has no file location".into()),
        _ => return Ok(Some(clip.text)),
    };
    if paths.is_empty() || paths.iter().any(|path| !path.exists()) {
        return Err("One or more copied files are no longer available".into());
    }
    if location {
        reveal(&paths)?;
    } else {
        super::open_external(&paths[0].to_string_lossy())?;
    }
    Ok(None)
}

fn reveal(paths: &[PathBuf]) -> Result<(), String> {
    unsafe {
        let initialized = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        if initialized != RPC_E_CHANGED_MODE {
            initialized.ok().map_err(|e| e.to_string())?;
        }
        let result = (|| {
            let folder = super::locations::common_folder(paths).ok_or_else(|| {
                windows::core::Error::new(
                    windows::core::HRESULT(0x80070057u32 as i32),
                    "Copied files have no common folder",
                )
            })?;
            if !paths
                .iter()
                .all(|path| path.parent() == Some(folder.as_path()))
            {
                return super::open_external(&folder.to_string_lossy()).map_err(|e| {
                    windows::core::Error::new(windows::core::HRESULT(0x80004005u32 as i32), e)
                });
            }
            let mut allocated = vec![];
            let result = (|| {
                let mut parent = std::ptr::null_mut();
                SHParseDisplayName(
                    &HSTRING::from(folder.as_os_str()),
                    None,
                    &mut parent,
                    0,
                    None,
                )?;
                allocated.push(parent);
                let mut children = vec![];
                for path in paths {
                    let mut pidl = std::ptr::null_mut();
                    SHParseDisplayName(&HSTRING::from(path.as_os_str()), None, &mut pidl, 0, None)?;
                    allocated.push(pidl);
                    children.push(ILFindLastID(pidl).cast_const());
                }
                SHOpenFolderAndSelectItems(parent, Some(&children), 0)
            })();
            for pidl in allocated {
                CoTaskMemFree(Some(pidl.cast()));
            }
            result
        })();
        if initialized.is_ok() {
            CoUninitialize();
        }
        result.map_err(|e: windows::core::Error| e.to_string())
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
