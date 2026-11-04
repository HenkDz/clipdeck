use std::io::Cursor;
fn decode_png(png: &[u8]) -> Result<image::DynamicImage, String> {
    let (width, height) =
        image::ImageReader::with_format(Cursor::new(png), image::ImageFormat::Png)
            .into_dimensions()
            .map_err(|e| e.to_string())?;
    if width > 16384 || height > 16384 || width as u64 * height as u64 > 32_000_000 {
        return Err("Image dimensions exceed capture limit".into());
    }
    let mut reader = image::ImageReader::with_format(Cursor::new(png), image::ImageFormat::Png);
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(16384);
    limits.max_image_height = Some(16384);
    limits.max_alloc = Some(256 * 1024 * 1024);
    reader.limits(limits);
    let image = reader.decode().map_err(|e| e.to_string())?;
    if image.width() as u64 * image.height() as u64 > 32_000_000 {
        return Err("Image dimensions exceed capture limit".into());
    }
    Ok(image)
}
pub fn png_metadata(png: &[u8]) -> Result<(Vec<u8>, String), String> {
    let image = decode_png(png)?;
    let mut thumb = Cursor::new(vec![]);
    image
        .thumbnail(112, 80)
        .write_to(&mut thumb, image::ImageFormat::Png)
        .map_err(|e| e.to_string())?;
    Ok((
        thumb.into_inner(),
        format!("{}×{}", image.width(), image.height()),
    ))
}
pub fn dib_from_png(png: &[u8]) -> Result<Vec<u8>, String> {
    let rgba = decode_png(png)?.to_rgba8();
    // A bottom-up 32-bit BI_RGB DIB is accepted by legacy clipboard consumers.
    let mut dib = vec![0u8; 40];
    dib[0..4].copy_from_slice(&40u32.to_le_bytes());
    dib[4..8].copy_from_slice(&(rgba.width() as i32).to_le_bytes());
    dib[8..12].copy_from_slice(&(rgba.height() as i32).to_le_bytes());
    dib[12..14].copy_from_slice(&1u16.to_le_bytes());
    dib[14..16].copy_from_slice(&32u16.to_le_bytes());
    dib[20..24].copy_from_slice(&(rgba.width() * rgba.height() * 4).to_le_bytes());
    for row in rgba.rows().rev() {
        for p in row {
            dib.extend_from_slice(&[p[2], p[1], p[0], p[3]]);
        }
    }
    Ok(dib)
}
pub fn png_from_dib(dib: &[u8]) -> Result<(Vec<u8>, Vec<u8>, String), String> {
    if dib.len() < 40 || dib.len() > 128 * 1024 * 1024 {
        return Err("Invalid DIB size".into());
    }
    let u32_at = |n| u32::from_le_bytes(dib[n..n + 4].try_into().unwrap());
    let header = u32_at(0) as usize;
    if ![40, 52, 56, 108, 124].contains(&header) || header > dib.len() {
        return Err("Unsupported DIB header".into());
    }
    let width = i32::from_le_bytes(dib[4..8].try_into().unwrap());
    let height = i32::from_le_bytes(dib[8..12].try_into().unwrap());
    if width <= 0
        || height == 0
        || width > 16384
        || height.unsigned_abs() > 16384
        || width as u64 * height.unsigned_abs() as u64 > 32_000_000
    {
        return Err("Image dimensions exceed capture limit".into());
    }
    let depth = u16::from_le_bytes(dib[14..16].try_into().unwrap());
    let colors = u32_at(32) as usize;
    let palette = if colors > 0 {
        colors
    } else if depth <= 8 {
        1usize << depth
    } else {
        0
    };
    let masks = if header == 40 && u32_at(16) == 3 {
        12
    } else if header == 40 && u32_at(16) == 6 {
        16
    } else {
        0
    };
    let offset = 14usize + header + masks + palette.saturating_mul(4);
    if offset > dib.len() + 14 {
        return Err("Truncated DIB".into());
    }
    let mut bmp = Vec::with_capacity(dib.len() + 14);
    bmp.extend(b"BM");
    bmp.extend(((dib.len() + 14) as u32).to_le_bytes());
    bmp.extend([0u8; 4]);
    bmp.extend((offset as u32).to_le_bytes());
    bmp.extend(dib);
    let image = image::load_from_memory_with_format(&bmp, image::ImageFormat::Bmp)
        .map_err(|e| e.to_string())?;
    let mut png = Cursor::new(vec![]);
    image
        .write_to(&mut png, image::ImageFormat::Png)
        .map_err(|e| e.to_string())?;
    let mut thumb = Cursor::new(vec![]);
    image
        .thumbnail(112, 80)
        .write_to(&mut thumb, image::ImageFormat::Png)
        .map_err(|e| e.to_string())?;
    Ok((
        png.into_inner(),
        thumb.into_inner(),
        format!("{}×{}", image.width(), image.height()),
    ))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn fullscreen_screenshot_is_admitted_and_pastes_as_dib() {
        let mut dib = vec![0u8; 40 + 3440 * 1392 * 4];
        dib[..4].copy_from_slice(&40u32.to_le_bytes());
        dib[4..8].copy_from_slice(&3440i32.to_le_bytes());
        dib[8..12].copy_from_slice(&1392i32.to_le_bytes());
        dib[12..14].copy_from_slice(&1u16.to_le_bytes());
        dib[14..16].copy_from_slice(&32u16.to_le_bytes());
        for p in dib[40..].as_chunks_mut::<4>().0 {
            p.copy_from_slice(&[40, 80, 160, 255]);
        }
        assert!(dib.len() > 16 * 1024 * 1024);
        let (png, thumb, dimensions) = png_from_dib(&dib).unwrap();
        let mut clip = crate::clip::Clip::text(dimensions, "Screenshot fixture".into());
        clip.kind = "image".into();
        clip.payload = png.clone();
        clip.thumbnail = thumb;
        clip.formats.push(crate::clip::Format {
            name: "CF_DIB".into(),
            data: dib,
        });
        let dir =
            std::env::temp_dir().join(format!("clipdeck-fullscreen-{}.db", std::process::id()));
        let mut store = crate::store::Store::open(&dir, 10, 16).unwrap();
        store.record(&clip, false, false).unwrap();
        let mut saved = store.list("").unwrap().remove(0);
        store.hydrate(&mut saved).unwrap();
        assert_eq!(saved.payload, png);
        assert!(saved.formats.is_empty());
        assert_eq!(store.skipped, 0);
        let dib = dib_from_png(&saved.payload).unwrap();
        assert_eq!(i32::from_le_bytes(dib[4..8].try_into().unwrap()), 3440);
        assert_eq!(dib[40..44], [40, 80, 160, 255]);
        assert_eq!(png_from_dib(&dib).unwrap().2, "3440×1392");
        drop(store);
        std::fs::remove_file(dir).unwrap();
    }
    #[test]
    fn registered_png_keeps_alpha_and_original_bytes() {
        let image = image::RgbaImage::from_pixel(2, 3, image::Rgba([20, 40, 60, 90]));
        let mut png = Cursor::new(vec![]);
        image.write_to(&mut png, image::ImageFormat::Png).unwrap();
        let png = png.into_inner();
        let original = png.clone();
        assert_eq!(png_metadata(&png).unwrap().1, "2×3");
        assert_eq!(png, original);
        assert_eq!(&dib_from_png(&png).unwrap()[40..44], &[60, 40, 20, 90]);
        assert!(png_metadata(b"broken").is_err());
    }
    #[test]
    fn dib_png_and_bad_input() {
        assert!(png_from_dib(&[0; 10]).is_err());
        let mut dib = vec![0u8; 44];
        dib[0..4].copy_from_slice(&40u32.to_le_bytes());
        dib[4..8].copy_from_slice(&1i32.to_le_bytes());
        dib[8..12].copy_from_slice(&1i32.to_le_bytes());
        dib[12..14].copy_from_slice(&1u16.to_le_bytes());
        dib[14..16].copy_from_slice(&24u16.to_le_bytes());
        dib[40..43].copy_from_slice(&[0, 0, 255]);
        let (png, thumb, size) = png_from_dib(&dib).unwrap();
        assert_eq!(size, "1×1");
        assert!(png.starts_with(b"\x89PNG"));
        assert!(!thumb.is_empty());
    }
}
