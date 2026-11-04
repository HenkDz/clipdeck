use rusqlite::{Connection, OpenFlags};
use std::path::{Path, PathBuf};

pub fn backup(dir: &Path, schema: i64) -> Result<Option<PathBuf>, String> {
    let db_path = crate::config::directory()
        .map_err(|e| e.to_string())?
        .join("history.db");
    snapshot(&db_path, &dir.join("history-before.db"), schema)
}
fn snapshot(db_path: &Path, output: &Path, schema: i64) -> Result<Option<PathBuf>, String> {
    if !db_path.exists() {
        return Ok(None);
    }
    let db = Connection::open_with_flags(db_path, OpenFlags::SQLITE_OPEN_READ_ONLY)
        .map_err(|e| e.to_string())?;
    db.busy_timeout(std::time::Duration::from_secs(5))
        .map_err(|e| e.to_string())?;
    let version: i64 = db
        .pragma_query_value(None, "user_version", |r| r.get(0))
        .map_err(|e| e.to_string())?;
    if version > schema {
        return Err("Update cannot open the existing history schema".into());
    }
    if output.exists() {
        return Err("History backup already exists".into());
    }
    db.execute(
        "VACUUM INTO ?1",
        [output.to_str().ok_or("Unsupported history backup path")?],
    )
    .map_err(|e| e.to_string())?;
    let copied = Connection::open_with_flags(output, OpenFlags::SQLITE_OPEN_READ_ONLY)
        .map_err(|e| e.to_string())?;
    let result: String = copied
        .query_row("PRAGMA quick_check", [], |r| r.get(0))
        .map_err(|e| e.to_string())?;
    if result != "ok" {
        return Err("History backup failed its integrity check".into());
    }
    Ok(Some(output.to_owned()))
}
pub fn restore(backup: &Path, nonce: &str) -> Result<(), String> {
    let db_path = crate::config::directory()
        .map_err(|e| e.to_string())?
        .join("history.db");
    restore_to(&db_path, backup, nonce)
}
fn restore_to(db_path: &Path, backup: &Path, nonce: &str) -> Result<(), String> {
    let db = Connection::open(db_path).map_err(|e| e.to_string())?;
    db.busy_timeout(std::time::Duration::from_secs(5))
        .map_err(|e| e.to_string())?;
    let busy: i64 = db
        .query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |r| r.get(0))
        .map_err(|e| e.to_string())?;
    drop(db);
    if busy != 0 {
        return Err("History is still in use; the recovery backup has been preserved".into());
    }
    let replacement = db_path.with_file_name(format!("history.restore-{nonce}.db"));
    let failed = db_path.with_file_name(format!("history.failed-{nonce}.db"));
    let mut file = std::fs::File::options()
        .write(true)
        .create_new(true)
        .open(&replacement)
        .map_err(|e| e.to_string())?;
    std::io::copy(
        &mut std::fs::File::open(backup).map_err(|e| e.to_string())?,
        &mut file,
    )
    .map_err(|e| e.to_string())?;
    file.sync_all().map_err(|e| e.to_string())?;
    drop(file);
    super::native::replace(db_path, &replacement, &failed)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn consistent_wal_backup_can_restore_a_failed_schema_change() {
        let dir = std::env::temp_dir().join(format!(
            "clipdeck-update-db-{}",
            super::super::crypto::nonce().unwrap()
        ));
        std::fs::create_dir(&dir).unwrap();
        let path = dir.join("history.db");
        let before = dir.join("before.db");
        let db = Connection::open(&path).unwrap();
        db.execute_batch("PRAGMA journal_mode=WAL; PRAGMA user_version=5; CREATE TABLE clips(value); INSERT INTO clips VALUES('private fixture');").unwrap();
        snapshot(&path, &before, 5).unwrap();
        assert!(snapshot(&path, &dir.join("too-old.db"), 4).is_err());
        db.execute_batch("PRAGMA user_version=6; DELETE FROM clips;")
            .unwrap();
        drop(db);
        restore_to(&path, &before, "test").unwrap();
        let db = Connection::open(&path).unwrap();
        assert_eq!(
            db.query_row("SELECT value FROM clips", [], |r| r.get::<_, String>(0))
                .unwrap(),
            "private fixture"
        );
        assert_eq!(
            db.pragma_query_value(None, "user_version", |r| r.get::<_, i64>(0))
                .unwrap(),
            5
        );
        drop(db);
        for name in ["history.db", "before.db", "history.failed-test.db"] {
            std::fs::remove_file(dir.join(name)).unwrap();
        }
        std::fs::remove_dir(dir).unwrap();
    }
}
