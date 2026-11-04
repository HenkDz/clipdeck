use crate::clip::{Clip, Format};
use std::path::PathBuf;

pub fn prepare(clips: &[Clip], plain: bool) -> Result<Clip, String> {
    if clips.is_empty() {
        return Err("No clips selected".into());
    }
    if clips.iter().all(Clip::is_text) {
        let text = clips
            .iter()
            .map(|c| c.text.as_str())
            .collect::<Vec<_>>()
            .join("\n");
        if text.len() > 128 * 1024 * 1024 {
            return Err("Selection exceeds 128 MiB".into());
        }
        return Ok(Clip::text(text, "Clipdeck selection".into()));
    }
    if plain
        || !clips
            .iter()
            .all(|c| matches!(c.kind.as_str(), "image" | "files"))
    {
        return Err("Select only text clips, or only images and files, for bulk copy/paste".into());
    }
    let mut paths = vec![];
    for clip in clips {
        let items = if clip.kind == "image" {
            vec![
                super::locations::resolve_image(clip, true)?
                    .ok_or("Cannot locate this image")?
                    .path,
            ]
        } else {
            clip.text.lines().map(PathBuf::from).collect()
        };
        for path in items {
            if !path.exists() {
                return Err(format!("File is no longer available: {}", path.display()));
            }
            if !paths.contains(&path) {
                paths.push(path);
            }
        }
    }
    let drop = hdrop(&paths)?;
    let mut clip = Clip::text(
        paths
            .iter()
            .map(|p| p.to_string_lossy())
            .collect::<Vec<_>>()
            .join("\n"),
        "Clipdeck selection".into(),
    );
    clip.kind = "files".into();
    clip.formats = vec![
        Format {
            name: "CF_HDROP".into(),
            data: drop,
        },
        Format {
            name: "Preferred DropEffect".into(),
            data: 1u32.to_le_bytes().to_vec(),
        },
    ];
    Ok(clip)
}
fn hdrop(paths: &[PathBuf]) -> Result<Vec<u8>, String> {
    if paths.is_empty() || paths.len() > 10000 {
        return Err("Select between 1 and 10,000 files".into());
    }
    let mut size = 22usize;
    let mut names = vec![];
    for path in paths {
        let name = path.to_str().ok_or("Invalid copied file path")?;
        let length = name.encode_utf16().count();
        if length == 0 || length > 32768 || name.contains('\0') {
            return Err("Invalid copied file path".into());
        }
        size = size.saturating_add((length + 1) * 2);
        if size > 128 * 1024 * 1024 {
            return Err("Selection exceeds 128 MiB".into());
        }
        names.push(name);
    }
    Ok(crate::files::hdrop(&names))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn text_order_and_file_drop_preserve_boundaries_and_unicode() {
        let clips = [
            Clip::text("second".into(), "test".into()),
            Clip::text("first".into(), "test".into()),
        ];
        assert_eq!(prepare(&clips, false).unwrap().text, "second\nfirst");
        let paths = [PathBuf::from("C:\\a, été.png"), PathBuf::from("C:\\二.png")];
        let data = hdrop(&paths).unwrap();
        assert_eq!(&data[..4], &20u32.to_le_bytes());
        assert_eq!(&data[16..20], &1u32.to_le_bytes());
        let wide = data[20..]
            .as_chunks::<2>()
            .0
            .iter()
            .map(|b| u16::from_le_bytes(*b))
            .collect::<Vec<_>>();
        assert_eq!(
            String::from_utf16_lossy(&wide),
            "C:\\a, été.png\0C:\\二.png\0\0"
        );
        assert!(hdrop(&[]).is_err());
        assert!(hdrop(&[PathBuf::from("bad\0path")]).is_err());
        let mut image = Clip::text("pixels".into(), "test".into());
        image.kind = "image".into();
        assert!(prepare(&[clips[0].clone(), image], false).is_err());
    }
    #[test]
    fn missing_files_fail_before_a_batch_is_written() {
        let mut clip = Clip::text(
            "C:\\clipdeck-nonexistent-fixture\nC:\\also-missing".into(),
            "test".into(),
        );
        clip.kind = "files".into();
        assert!(prepare(&[clip.clone(), clip], false).is_err());
    }
}
