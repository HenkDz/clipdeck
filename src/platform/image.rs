use std::io::Cursor;
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
