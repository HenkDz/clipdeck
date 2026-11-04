use crate::files::Kind;
use image::{RgbaImage, imageops};
use std::{
    io::Cursor,
    path::{Path, PathBuf},
};
use windows::{
    Win32::{Foundation::*, Graphics::Gdi::*, System::Com::*, UI::Shell::*},
    core::HSTRING,
};

/// A Shell thumbnail (media) or type icon at up to `side` pixels, with real transparency.
pub fn shell_image(path: &str, side: i32, icon_only: bool) -> Result<RgbaImage, String> {
    unsafe {
        let initialized = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        if initialized != RPC_E_CHANGED_MODE {
            initialized.ok().map_err(|e| e.to_string())?;
        }
        let result = (|| {
            let factory: IShellItemImageFactory =
                SHCreateItemFromParsingName(&HSTRING::from(path), None)
                    .map_err(|e| e.to_string())?;
            let flags = if icon_only {
                SIIGBF_ICONONLY
            } else {
                SIIGBF_RESIZETOFIT
            };
            let bitmap = factory
                .GetImage(SIZE { cx: side, cy: side }, flags)
                .map_err(|e| e.to_string())?;
            let result = bitmap_rgba(bitmap);
            let _ = DeleteObject(bitmap.into());
            result
        })();
        if initialized.is_ok() {
            CoUninitialize();
        }
        result
    }
}

pub fn png(image: &RgbaImage) -> Result<Vec<u8>, String> {
    let mut out = Cursor::new(Vec::new());
    image
        .write_to(&mut out, image::ImageFormat::Png)
        .map_err(|e| e.to_string())?;
    Ok(out.into_inner())
}

pub fn thumbnail(path: &str, side: i32) -> Result<Vec<u8>, String> {
    png(&shell_image(path, side, false)?)
}

unsafe fn bitmap_rgba(bitmap: HBITMAP) -> Result<RgbaImage, String> {
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
        bgra_to_rgba(&mut bytes);
        RgbaImage::from_raw(bm.bmWidth as u32, bm.bmHeight as u32, bytes)
            .ok_or_else(|| "Invalid thumbnail".into())
    }
}

/// Shell bitmaps are premultiplied BGRA; photo thumbnails often carry no alpha at all.
fn bgra_to_rgba(bytes: &mut [u8]) {
    let opaque = bytes.as_chunks::<4>().0.iter().all(|p| p[3] == 0);
    for p in bytes.as_chunks_mut::<4>().0 {
        p.swap(0, 2);
        if opaque {
            p[3] = 255;
        } else if p[3] > 0 && p[3] < 255 {
            let alpha = p[3] as u32;
            for c in &mut p[..3] {
                *c = (*c as u32 * 255 / alpha).min(255) as u8;
            }
        }
    }
}

/// Cells of the leading mosaic on an 88x64 canvas (twice the 44x32 row slot).
/// More than four files leave the last cell free for the row's count badge.
pub const MOSAIC: (u32, u32) = (88, 64);
pub fn mosaic_cells(count: usize) -> Vec<(u32, u32, u32, u32)> {
    let (w, h, gap) = (MOSAIC.0, MOSAIC.1, 4);
    let half_w = (w - gap) / 2;
    let half_h = (h - gap) / 2;
    let right = half_w + gap;
    let lower = half_h + gap;
    match count {
        0 => vec![],
        1 => vec![(0, 0, w, h)],
        2 => vec![(0, 0, half_w, h), (right, 0, half_w, h)],
        3 => vec![
            (0, 0, half_w, h),
            (right, 0, half_w, half_h),
            (right, lower, half_w, half_h),
        ],
        4 => vec![
            (0, 0, half_w, half_h),
            (right, 0, half_w, half_h),
            (0, lower, half_w, half_h),
            (right, lower, half_w, half_h),
        ],
        _ => mosaic_cells(4)[..3].to_vec(),
    }
}

fn rounded(image: &mut RgbaImage, radius: f32) {
    let (w, h) = (image.width() as f32, image.height() as f32);
    for (x, y, p) in image.enumerate_pixels_mut() {
        let (cx, cy) = (x as f32 + 0.5, y as f32 + 0.5);
        let dx = (radius - cx).max(cx - (w - radius)).max(0.);
        let dy = (radius - cy).max(cy - (h - radius)).max(0.);
        let coverage = (radius + 0.5 - (dx * dx + dy * dy).sqrt()).clamp(0., 1.);
        p[3] = (p[3] as f32 * coverage) as u8;
    }
}

fn cell_image(source: Option<&RgbaImage>, media: bool, w: u32, h: u32) -> RgbaImage {
    // A translucent neutral tile reads on both light and dark rows.
    let mut cell = RgbaImage::from_pixel(w, h, image::Rgba([128, 128, 128, 52]));
    if let Some(source) = source.filter(|s| s.width() > 0 && s.height() > 0) {
        if media {
            let scale = (w as f32 / source.width() as f32).max(h as f32 / source.height() as f32);
            let sw = ((source.width() as f32 * scale).ceil() as u32).max(w);
            let sh = ((source.height() as f32 * scale).ceil() as u32).max(h);
            let resized = imageops::resize(source, sw, sh, imageops::Triangle);
            let cropped = imageops::crop_imm(&resized, (sw - w) / 2, (sh - h) / 2, w, h).to_image();
            imageops::overlay(&mut cell, &cropped, 0, 0);
        } else {
            let side = (w.min(h) as f32 * 0.72) as u32;
            let icon = imageops::resize(source, side, side, imageops::Triangle);
            let (x, y) = ((w - side) / 2, (h - side) / 2);
            imageops::overlay(&mut cell, &icon, x as i64, y as i64);
        }
    }
    rounded(&mut cell, 6.);
    cell
}

pub fn mosaic(paths: &[&str], kinds: &[Kind]) -> Result<Vec<u8>, String> {
    let mut canvas = RgbaImage::new(MOSAIC.0, MOSAIC.1);
    for (ix, (x, y, w, h)) in mosaic_cells(paths.len()).into_iter().enumerate() {
        let media = kinds.get(ix).is_some_and(|k| k.is_media());
        let source = shell_image(paths[ix], if media { 128 } else { 48 }, !media).ok();
        let cell = cell_image(source.as_ref(), media, w, h);
        imageops::overlay(&mut canvas, &cell, x as i64, y as i64);
    }
    png(&canvas)
}

/// Runs off the UI thread at capture: per-file kinds and the leading thumbnail.
pub fn summarize_files(clip: &mut crate::clip::Clip) {
    let paths: Vec<&str> = clip.text.lines().collect();
    // Folder checks touch the filesystem; very long lists fall back to extensions.
    let kinds: Vec<Kind> = paths
        .iter()
        .enumerate()
        .map(|(ix, p)| Kind::of(p, ix < 1000 && Path::new(p).is_dir()))
        .collect();
    clip.file_kinds = crate::files::codes(&kinds);
    clip.thumbnail = match kinds.as_slice() {
        [kind] if kind.is_media() => thumbnail(paths[0], 128).unwrap_or_default(),
        [_] => vec![],
        _ => mosaic(&paths, &kinds).unwrap_or_default(),
    };
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

pub fn reveal(paths: &[PathBuf]) -> Result<(), String> {
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
            assert_eq!(Kind::of(path, false), Kind::Video);
        }
        assert_eq!(Kind::of(r"C:\foo.PNG", false), Kind::Image);
        assert_eq!(Kind::of("archive.mp4.txt", false), Kind::Document);
        assert_eq!(Kind::of("no-extension", false), Kind::Other);
        assert_eq!(Kind::of(r"C:\assets", true), Kind::Folder);
    }
    #[test]
    fn mosaic_cells_stay_inside_and_leave_room_for_the_badge() {
        for count in 1..=6 {
            let cells = mosaic_cells(count);
            assert_eq!(cells.len(), count.min(4).min(if count > 4 { 3 } else { 4 }));
            for (x, y, w, h) in cells {
                assert!(x + w <= MOSAIC.0 && y + h <= MOSAIC.1);
            }
        }
        // The fourth slot stays empty for "+N" when there are more than four files.
        assert_eq!(mosaic_cells(5), mosaic_cells(4)[..3]);
    }
    #[test]
    fn shell_alpha_is_unpremultiplied_and_opaque_thumbnails_kept() {
        let mut photo = vec![10, 20, 30, 0, 40, 50, 60, 0];
        bgra_to_rgba(&mut photo);
        assert_eq!(photo, [30, 20, 10, 255, 60, 50, 40, 255]);
        let mut icon = vec![0, 0, 0, 0, 50, 25, 100, 128];
        bgra_to_rgba(&mut icon);
        assert_eq!(icon, [0, 0, 0, 0, 199, 49, 99, 128]);
    }
}
