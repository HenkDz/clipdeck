use crate::clip::Clip;
use rusqlite::{Connection, OpenFlags, params};
use std::{
    io::Write,
    path::{Path, PathBuf},
    time::{Duration, UNIX_EPOCH},
};
use windows::{
    Win32::{System::Com::CoTaskMemFree, UI::Shell::*},
    core::GUID,
};

pub struct Location {
    pub path: PathBuf,
    pub title: String,
}

pub fn known_folder(id: &GUID) -> Result<PathBuf, String> {
    unsafe {
        let ptr = SHGetKnownFolderPath(id, KF_FLAG_DEFAULT, None).map_err(|e| e.to_string())?;
        let path = ptr
            .to_string()
            .map(PathBuf::from)
            .map_err(|e| e.to_string());
        CoTaskMemFree(Some(ptr.0.cast()));
        path
    }
}
fn database(read_only: bool) -> Result<Connection, String> {
    let path = crate::config::directory()
        .map_err(|e| e.to_string())?
        .join("history.db");
    let flags = if read_only {
        OpenFlags::SQLITE_OPEN_READ_ONLY
    } else {
        OpenFlags::SQLITE_OPEN_READ_WRITE
    };
    let db = Connection::open_with_flags(path, flags).map_err(|e| e.to_string())?;
    db.busy_timeout(Duration::from_secs(2))
        .map_err(|e| e.to_string())?;
    Ok(db)
}
pub fn load_clip(id: i64) -> Result<Clip, String> {
    database(true)?.query_row("SELECT kind,text,coalesce(payload,cast(text AS BLOB)),source,updated,file_path,screenshot_title,source_exe FROM clips WHERE id=?1",[id],|r| {
        let mut clip = Clip::text(r.get(1)?,r.get(3)?);
        clip.id=id; clip.kind=r.get(0)?; clip.payload=r.get(2)?; clip.updated=r.get(4)?;
        clip.file_path=r.get(5)?; clip.screenshot_title=r.get(6)?; clip.source_exe=r.get(7)?;
        Ok(clip)
    }).map_err(|e| e.to_string())
}
fn remember(clip: &Clip, location: &Location) -> Result<(), String> {
    database(false)?.execute("UPDATE clips SET file_path=?1,screenshot_title=?2 WHERE id=?3 AND updated=?4 AND kind='image' AND payload=?5",
        params![location.path.to_string_lossy(),location.title,clip.id,clip.updated,clip.payload]).map_err(|e| e.to_string())?;
    Ok(())
}
pub fn resolve_image(clip: &Clip, save: bool) -> Result<Option<Location>, String> {
    if !clip.file_path.is_empty() && Path::new(&clip.file_path).is_file() {
        return Ok(Some(Location {
            path: PathBuf::from(&clip.file_path),
            title: clip.screenshot_title.clone(),
        }));
    }
    let mut location = None;
    if clip.source_exe.eq_ignore_ascii_case("ShareX.exe")
        || (clip.source_exe.is_empty() && clip.source.to_ascii_lowercase().contains("sharex"))
    {
        let root = known_folder(&FOLDERID_Documents)?.join("ShareX");
        location = sharex(&root, clip.updated, &clip.text)?;
    }
    if location.is_none() {
        let folder = known_folder(&FOLDERID_Pictures)?.join("Screenshots");
        location = recent_image(&folder, clip.updated, 5, &clip.text, 0).map(|path| Location {
            path,
            title: String::new(),
        });
    }
    if location.is_none() && save {
        let folder = known_folder(&FOLDERID_Pictures)?.join("Clipdeck");
        location = Some(Location {
            path: export(&folder, clip.updated, clip.id, &clip.payload)?,
            title: String::new(),
        });
    }
    if let Some(location) = &location {
        remember(clip, location)?;
    }
    Ok(location)
}

fn sharex(root: &Path, captured: i64, dimensions: &str) -> Result<Option<Location>, String> {
    let history = root.join("History.db");
    if history.exists() {
        let db = Connection::open_with_flags(history, OpenFlags::SQLITE_OPEN_READ_ONLY)
            .map_err(|e| e.to_string())?;
        // SQLite's busy handler retries briefly without copying a potentially live WAL database.
        db.busy_timeout(Duration::from_secs(2))
            .map_err(|e| e.to_string())?;
        return sharex_history(&db, captured).map_err(|e| e.to_string());
    }
    let config = root.join("ApplicationConfig.json");
    if !config.exists() {
        return Ok(None);
    }
    let json = std::fs::read_to_string(config).map_err(|e| e.to_string())?;
    let db = Connection::open_in_memory().map_err(|e| e.to_string())?;
    let (custom,path): (Option<bool>,Option<String>) = db.query_row("SELECT json_extract(?1,'$.UseCustomScreenshotsPath'),json_extract(?1,'$.CustomScreenshotsPath')",[json],|r| Ok((r.get(0)?,r.get(1)?))).map_err(|e| e.to_string())?;
    if custom == Some(true)
        && let Some(path) = path
    {
        return Ok(
            recent_image(Path::new(&path), captured, 10, dimensions, 2).map(|path| Location {
                path,
                title: String::new(),
            }),
        );
    }
    Ok(None)
}
fn sharex_history(db: &Connection, captured: i64) -> rusqlite::Result<Option<Location>> {
    let mut statement = db.prepare("SELECT FilePath,CASE WHEN json_valid(Tags) THEN coalesce(json_extract(Tags,'$.WindowTitle'),json_extract(Tags,'$.ProcessName'),'') ELSE '' END FROM History WHERE Type='Image' AND abs(unixepoch(DateTime)-?1)<=10 ORDER BY unixepoch(DateTime) DESC")?;
    for row in statement.query_map([captured], |r| {
        Ok(Location {
            path: PathBuf::from(r.get::<_, String>(0)?),
            title: r.get(1)?,
        })
    })? {
        let location = row?;
        if location.path.is_file() {
            return Ok(Some(location));
        }
    }
    Ok(None)
}

fn recent_image(
    root: &Path,
    captured: i64,
    tolerance: i64,
    dimensions: &str,
    depth: usize,
) -> Option<PathBuf> {
    let entries = std::fs::read_dir(root).ok()?;
    let mut newest: Option<(i64, PathBuf)> = None;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() && depth > 0 {
            if let Some(found) = recent_image(&path, captured, tolerance, dimensions, depth - 1)
                && let Some(time) = created(&found)
                && newest.as_ref().is_none_or(|(t, _)| time > *t)
            {
                newest = Some((time, found));
            }
            continue;
        }
        if path
            .extension()
            .is_none_or(|ext| !ext.eq_ignore_ascii_case("png"))
        {
            continue;
        }
        let Some(time) = created(&path) else { continue };
        if (time - captured).abs() > tolerance {
            continue;
        }
        let Ok(reader) = image::ImageReader::open(&path) else {
            continue;
        };
        let Ok(reader) = reader.with_guessed_format() else {
            continue;
        };
        let Ok((w, h)) = reader.into_dimensions() else {
            continue;
        };
        if format!("{w}×{h}") == dimensions && newest.as_ref().is_none_or(|(t, _)| time > *t) {
            newest = Some((time, path));
        }
    }
    newest.map(|(_, path)| path)
}
fn created(path: &Path) -> Option<i64> {
    let meta = std::fs::metadata(path).ok()?;
    meta.created()
        .or_else(|_| meta.modified())
        .ok()?
        .duration_since(UNIX_EPOCH)
        .ok()
        .map(|d| d.as_secs() as i64)
}
fn export(folder: &Path, captured: i64, id: i64, png: &[u8]) -> Result<PathBuf, String> {
    std::fs::create_dir_all(folder).map_err(|e| e.to_string())?;
    let stamp = super::time::stamp(captured)?;
    for suffix in [String::new(), format!("-{id}")] {
        let path = folder.join(format!("{stamp}{suffix}.png"));
        if path.exists() && std::fs::read(&path).is_ok_and(|bytes| bytes == png) {
            return Ok(path);
        }
        match std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
        {
            Ok(mut file) => {
                if let Err(error) = file.write_all(png) {
                    drop(file);
                    let _ = std::fs::remove_file(&path);
                    return Err(error.to_string());
                }
                return Ok(path);
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error.to_string()),
        }
    }
    Err("A different screenshot already exists at this location".into())
}

pub fn common_folder(paths: &[PathBuf]) -> Option<PathBuf> {
    let mut folder = paths.first()?.parent()?;
    while !paths.iter().all(|path| path.starts_with(folder)) {
        folder = folder.parent()?;
    }
    Some(folder.to_path_buf())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn snipping_dimensions_and_custom_path_only_when_history_is_missing() {
        let root =
            std::env::temp_dir().join(format!("clipdeck-config-source-{}", std::process::id()));
        let custom = root.join("custom");
        std::fs::create_dir_all(&custom).unwrap();
        let png = custom.join("screen.png");
        image::RgbaImage::new(5, 7).save(&png).unwrap();
        let captured = created(&png).unwrap();
        assert_eq!(
            recent_image(&custom, captured + 5, 5, "5×7", 0),
            Some(png.clone())
        );
        assert!(recent_image(&custom, captured + 6, 5, "5×7", 0).is_none());
        assert!(recent_image(&custom, captured, 5, "7×5", 0).is_none());
        let config = root.join("ApplicationConfig.json");
        let db = Connection::open_in_memory().unwrap();
        let json:String=db.query_row("SELECT json_object('UseCustomScreenshotsPath',json('true'),'CustomScreenshotsPath',?1)",[custom.to_string_lossy().as_ref()],|r|r.get(0)).unwrap();
        std::fs::write(&config, json).unwrap();
        assert_eq!(sharex(&root, captured, "5×7").unwrap().unwrap().path, png);
        let history = root.join("History.db");
        let db = Connection::open(&history).unwrap();
        db.execute_batch("CREATE TABLE History(FilePath TEXT,DateTime TEXT,Type TEXT,Tags TEXT)")
            .unwrap();
        drop(db);
        assert!(sharex(&root, captured, "5×7").unwrap().is_none());
        std::fs::remove_file(history).unwrap();
        std::fs::remove_file(config).unwrap();
        std::fs::remove_file(png).unwrap();
        std::fs::remove_dir(custom).unwrap();
        std::fs::remove_dir(root).unwrap();
    }
    #[test]
    fn sharex_offsets_window_and_existing_file_are_respected() {
        let dir = std::env::temp_dir().join(format!("clipdeck-source-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("screenshot.png");
        std::fs::write(&file, b"fixture").unwrap();
        let db = Connection::open_in_memory().unwrap();
        db.execute_batch("CREATE TABLE History(FilePath TEXT,DateTime TEXT,Type TEXT,Tags TEXT)")
            .unwrap();
        db.execute("INSERT INTO History VALUES(?1,'2026-11-04T08:31:35.9887681+01:00','Image','{\"WindowTitle\":\"Ditto\"}')",[file.to_string_lossy().as_ref()]).unwrap();
        let captured: i64 = db
            .query_row("SELECT unixepoch('2026-11-04T07:31:35Z')", [], |r| r.get(0))
            .unwrap();
        let found = sharex_history(&db, captured + 10).unwrap().unwrap();
        assert_eq!(found.path, file);
        assert_eq!(found.title, "Ditto");
        assert!(sharex_history(&db, captured + 11).unwrap().is_none());
        std::fs::remove_file(file).unwrap();
        assert!(sharex_history(&db, captured).unwrap().is_none());
        std::fs::remove_dir(dir).unwrap();
    }
    #[test]
    fn export_never_overwrites_and_common_folder_handles_multiple_files() {
        let dir = std::env::temp_dir().join(format!("clipdeck-export-test-{}", std::process::id()));
        let first = export(&dir, 1700000000, 1, b"first").unwrap();
        assert_eq!(export(&dir, 1700000000, 1, b"first").unwrap(), first);
        let second = export(&dir, 1700000000, 2, b"second").unwrap();
        assert_ne!(first, second);
        assert_eq!(std::fs::read(&first).unwrap(), b"first");
        assert_eq!(
            common_folder(&[first.clone(), second.clone()]),
            Some(dir.clone())
        );
        assert_eq!(
            common_folder(&[dir.join("a/x"), dir.join("b/y")]),
            Some(dir.clone())
        );
        std::fs::remove_file(first).unwrap();
        std::fs::remove_file(second).unwrap();
        std::fs::remove_dir(dir).unwrap();
    }
    #[test]
    #[ignore = "reads the owner's real ShareX history; read-only desktop verification"]
    fn real_sharex_history_uses_redirected_documents() {
        let root = known_folder(&FOLDERID_Documents).unwrap().join("ShareX");
        let db =
            Connection::open_with_flags(root.join("History.db"), OpenFlags::SQLITE_OPEN_READ_ONLY)
                .unwrap();
        let captured: i64 = db
            .query_row(
                "SELECT max(unixepoch(DateTime)) FROM History WHERE Type='Image'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        let found = sharex_history(&db, captured).unwrap().unwrap();
        assert!(found.path.is_file());
        println!(
            "Known Documents: {}; newest real ShareX image: {}",
            root.display(),
            found.path.display()
        );
        let clip_time:i64=database(true).unwrap().query_row("SELECT updated FROM clips WHERE kind='image' AND lower(source) LIKE '%sharex%' ORDER BY rank DESC LIMIT 1",[],|r|r.get(0)).expect("owner ShareX clip in Clipdeck");
        let matched = sharex_history(&db, clip_time)
            .unwrap()
            .expect("real Clipdeck capture matches ShareX timestamp");
        println!(
            "Real Clipdeck capture at {clip_time} resolves to {}",
            matched.path.display()
        );
    }
}
