mod tests;
use crate::clip::{Clip, Format};
use rusqlite::{Connection, Result, params};
use sha2::{Digest, Sha256};
use std::path::Path;

pub struct Store {
    db: Connection,
    limit: usize,
    pub fts: bool,
    max_clip_bytes: usize,
    pub skipped: usize,
}

const SEARCH_BYTES: usize = 64 * 1024;

fn search_text(text: &str) -> String {
    fn prefix(s: &str) -> &str {
        let mut end = s.len().min(SEARCH_BYTES);
        while !s.is_char_boundary(end) {
            end -= 1;
        }
        &s[..end]
    }
    prefix(&prefix(text).to_lowercase()).to_owned()
}

pub(crate) fn content_hash(kind: &str, payload: &[u8]) -> Vec<u8> {
    let mut hash = Sha256::new();
    hash.update(kind.as_bytes());
    hash.update([0]);
    hash.update(payload);
    hash.finalize().to_vec()
}

pub fn migrate(db: &mut Connection) -> Result<bool> {
    let version: i64 = db.pragma_query_value(None, "user_version", |r| r.get(0))?;
    if version > 4 {
        return Err(rusqlite::Error::InvalidQuery);
    }
    // Preserve formats referencing stable clip IDs while replacing the parent table.
    db.pragma_update(None, "foreign_keys", false)?;
    let result: Result<bool> = (|| {
        let tx = db.transaction()?;
        if version < 2 {
            tx.execute_batch("DROP TRIGGER IF EXISTS clips_ai; DROP TRIGGER IF EXISTS clips_ad;
                DROP TRIGGER IF EXISTS clips_au; DROP TABLE IF EXISTS clips_fts;
                CREATE TABLE clips_v2(
                id INTEGER PRIMARY KEY, kind TEXT NOT NULL, text TEXT NOT NULL, search TEXT NOT NULL,
                payload BLOB, hash BLOB NOT NULL, thumbnail BLOB NOT NULL, source TEXT NOT NULL,
                created INTEGER NOT NULL, updated INTEGER NOT NULL, rank INTEGER NOT NULL,
                pinned INTEGER NOT NULL DEFAULT 0, UNIQUE(kind,hash));")?;
            if version == 1 {
                let mut stmt = tx.prepare("SELECT id,kind,text,payload,thumbnail,source,created,updated,rank,pinned FROM clips")?;
                let mut rows = stmt.query([])?;
                while let Some(row) = rows.next()? {
                    let kind: String = row.get(1)?;
                    let text: String = row.get(2)?;
                    let payload: Vec<u8> = row.get(3)?;
                    tx.execute(
                        "INSERT INTO clips_v2 VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12)",
                        params![
                            row.get::<_, i64>(0)?,
                            kind,
                            text,
                            search_text(&text),
                            (payload != text.as_bytes()).then_some(&payload),
                            content_hash(&kind, &payload),
                            row.get::<_, Vec<u8>>(4)?,
                            row.get::<_, String>(5)?,
                            row.get::<_, i64>(6)?,
                            row.get::<_, i64>(7)?,
                            row.get::<_, i64>(8)?,
                            row.get::<_, bool>(9)?
                        ],
                    )?;
                }
                tx.execute_batch("DROP TABLE clips;")?;
            }
            tx.execute_batch("ALTER TABLE clips_v2 RENAME TO clips;
                CREATE TABLE IF NOT EXISTS formats(clip_id INTEGER NOT NULL REFERENCES clips(id) ON DELETE CASCADE,
                name TEXT NOT NULL,data BLOB NOT NULL, PRIMARY KEY(clip_id,name));
                CREATE INDEX clips_order ON clips(pinned DESC,rank DESC);
                PRAGMA user_version=2;")?;
        }
        if version < 3 {
            tx.execute_batch(
                "ALTER TABLE clips ADD COLUMN copies INTEGER NOT NULL DEFAULT 1;
                ALTER TABLE clips ADD COLUMN file_path TEXT NOT NULL DEFAULT '';
                ALTER TABLE clips ADD COLUMN screenshot_title TEXT NOT NULL DEFAULT '';
                ALTER TABLE clips ADD COLUMN source_exe TEXT NOT NULL DEFAULT '';
                DELETE FROM formats WHERE clip_id IN(SELECT id FROM clips WHERE kind='image');
                PRAGMA user_version=3;",
            )?;
        }
        if version < 4 {
            tx.execute_batch("ALTER TABLE clips ADD COLUMN pastes INTEGER NOT NULL DEFAULT 0; PRAGMA user_version=4;")?;
        }
        let had_fts: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE name='clips_fts')",
            [],
            |r| r.get(0),
        )?;
        let fts = tx.execute_batch("CREATE VIRTUAL TABLE IF NOT EXISTS clips_fts USING fts5(search,content='clips',content_rowid='id',tokenize='trigram');
            CREATE TRIGGER IF NOT EXISTS clips_ai AFTER INSERT ON clips BEGIN INSERT INTO clips_fts(rowid,search) VALUES(new.id,new.search); END;
            CREATE TRIGGER IF NOT EXISTS clips_ad AFTER DELETE ON clips BEGIN INSERT INTO clips_fts(clips_fts,rowid,search) VALUES('delete',old.id,old.search); END;
            CREATE TRIGGER IF NOT EXISTS clips_au AFTER UPDATE OF search ON clips BEGIN INSERT INTO clips_fts(clips_fts,rowid,search) VALUES('delete',old.id,old.search); INSERT INTO clips_fts(rowid,search) VALUES(new.id,new.search); END;").is_ok();
        if fts && !had_fts {
            tx.execute_batch("INSERT INTO clips_fts(clips_fts) VALUES('rebuild');")?;
        }
        tx.commit()?;
        Ok(fts)
    })();
    db.pragma_update(None, "foreign_keys", true)?;
    let fts = result?;
    if version > 0 && version < 3 {
        db.execute_batch("VACUUM;")?;
    }
    Ok(fts)
}

impl Store {
    pub fn open(path: &Path, limit: usize, max_clip_mb: usize) -> Result<Self> {
        Self::from_connection(Connection::open(path)?, limit, max_clip_mb)
    }
    fn from_connection(mut db: Connection, limit: usize, max_clip_mb: usize) -> Result<Self> {
        db.execute_batch("PRAGMA foreign_keys=ON; PRAGMA journal_mode=WAL;")?;
        db.busy_timeout(std::time::Duration::from_secs(2))?;
        let fts = migrate(&mut db)?;
        let mut store = Self {
            db,
            limit,
            fts,
            skipped: 0,
            max_clip_bytes: max_clip_mb.saturating_mul(1024 * 1024),
        };
        store.retain()?;
        Ok(store)
    }
    // Only admitted captures and explicit edits/seeds reach this writer. Clipboard exclusions are checked before capture construction.
    pub fn record(&mut self, clip: &Clip, own_write: bool, excluded: bool) -> Result<Option<i64>> {
        if own_write || excluded || clip.payload.is_empty() {
            return Ok(None);
        }
        let size = if clip.kind == "image" {
            clip.payload.len().saturating_add(clip.thumbnail.len())
        } else {
            clip.text
                .len()
                .saturating_add(if clip.payload == clip.text.as_bytes() {
                    0
                } else {
                    clip.payload.len()
                })
                .saturating_add(clip.thumbnail.len())
                .saturating_add(clip.formats.iter().map(|f| f.data.len()).sum::<usize>())
        };
        if size > self.max_clip_bytes {
            self.skipped += 1;
            eprintln!(
                "Skipping clip: {size} bytes exceeds max_clip_mb ({} bytes)",
                self.max_clip_bytes
            );
            return Ok(None);
        }
        let hash = content_hash(&clip.kind, &clip.payload);
        let tx = self.db.transaction()?;
        tx.execute("INSERT INTO clips(kind,text,search,payload,hash,thumbnail,source,created,updated,rank,pinned,file_path,screenshot_title,source_exe)
            VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,(SELECT COALESCE(MAX(rank),0)+1 FROM clips),0,?10,?11,?12)
            ON CONFLICT(kind,hash) DO UPDATE SET text=excluded.text,payload=excluded.payload,search=excluded.search,thumbnail=excluded.thumbnail,source=excluded.source,updated=excluded.updated,copies=clips.copies+1,file_path=CASE WHEN excluded.file_path<>'' THEN excluded.file_path ELSE clips.file_path END,screenshot_title=CASE WHEN excluded.screenshot_title<>'' THEN excluded.screenshot_title ELSE clips.screenshot_title END,source_exe=excluded.source_exe,rank=(SELECT COALESCE(MAX(rank),0)+1 FROM clips)",
            params![clip.kind,clip.text,search_text(&clip.text),(clip.payload != clip.text.as_bytes()).then_some(&clip.payload),hash,clip.thumbnail,clip.source,clip.created,clip.updated,clip.file_path,clip.screenshot_title,clip.source_exe])?;
        let id: i64 = tx.query_row(
            "SELECT id FROM clips WHERE kind=?1 AND hash=?2",
            params![clip.kind, hash],
            |r| r.get(0),
        )?;
        tx.execute("DELETE FROM formats WHERE clip_id=?1", [id])?;
        for f in clip.formats.iter().filter(|_| clip.kind != "image") {
            tx.execute(
                "INSERT INTO formats VALUES(?1,?2,?3)",
                params![id, f.name, f.data],
            )?;
        }
        tx.execute("DELETE FROM clips WHERE pinned=0 AND id NOT IN(SELECT id FROM clips WHERE pinned=0 ORDER BY rank DESC LIMIT ?1)", [self.limit as i64])?;
        tx.commit()?;
        Ok(Some(id))
    }
    pub fn list(&self, query: &str) -> Result<Vec<Clip>> {
        let q = query.to_lowercase();
        // LIKE preserves literal substring behavior, including punctuation and short queries.
        let pattern = format!(
            "%{}%",
            q.replace('\\', "\\\\")
                .replace('%', "\\%")
                .replace('_', "\\_")
        );
        let sql = if self.fts && !q.contains(['%', '_', '\\']) {
            "SELECT id,kind,substr(text,1,512),X'',thumbnail,source,created,updated,pinned,CASE WHEN kind='files' AND text<>'' THEN length(text)-length(replace(text,char(10),''))+1 ELSE 0 END,copies,file_path,screenshot_title,source_exe,pastes FROM clips WHERE id IN (SELECT rowid FROM clips_fts WHERE search LIKE ?1) ORDER BY pinned DESC,rank DESC"
        } else if self.fts {
            "SELECT id,kind,substr(text,1,512),X'',thumbnail,source,created,updated,pinned,CASE WHEN kind='files' AND text<>'' THEN length(text)-length(replace(text,char(10),''))+1 ELSE 0 END,copies,file_path,screenshot_title,source_exe,pastes FROM clips WHERE id IN (SELECT rowid FROM clips_fts WHERE search LIKE ?1 ESCAPE '\\') ORDER BY pinned DESC,rank DESC"
        } else {
            "SELECT id,kind,substr(text,1,512),X'',thumbnail,source,created,updated,pinned,CASE WHEN kind='files' AND text<>'' THEN length(text)-length(replace(text,char(10),''))+1 ELSE 0 END,copies,file_path,screenshot_title,source_exe,pastes FROM clips WHERE search LIKE ?1 ESCAPE '\\' ORDER BY pinned DESC,rank DESC"
        };
        self.db
            .prepare(sql)?
            .query_map([pattern], |r| {
                Ok(Clip {
                    copies: r.get(10)?,
                    pastes: r.get(14)?,
                    file_path: r.get(11)?,
                    screenshot_title: r.get(12)?,
                    source_exe: r.get(13)?,
                    sequence: 0,
                    id: r.get(0)?,
                    kind: r.get(1)?,
                    text: r.get(2)?,
                    payload: r.get(3)?,
                    thumbnail: r.get(4)?,
                    source: r.get(5)?,
                    created: r.get(6)?,
                    updated: r.get(7)?,
                    pinned: r.get(8)?,
                    formats: vec![],
                    file_count: r.get::<_, i64>(9)? as usize,
                })
            })?
            .collect()
    }
    pub fn hydrate(&self, clip: &mut Clip) -> Result<()> {
        (clip.text, clip.payload) = self.db.query_row(
            "SELECT text,COALESCE(payload,CAST(text AS BLOB)) FROM clips WHERE id=?1",
            [clip.id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )?;
        clip.formats = self
            .db
            .prepare("SELECT name,data FROM formats WHERE clip_id=?1")?
            .query_map([clip.id], |r| {
                Ok(Format {
                    name: r.get(0)?,
                    data: r.get(1)?,
                })
            })?
            .collect::<Result<_>>()?;
        Ok(())
    }
    pub fn delete(&mut self, id: i64) -> Result<()> {
        self.db.execute("DELETE FROM clips WHERE id=?1", [id])?;
        Ok(())
    }
    pub fn mark_pasted(&mut self, id: i64, hash: &[u8]) -> Result<()> {
        // A delayed completion must not mark a different clip after edit/deletion/ID reuse.
        self.db.execute(
            "UPDATE clips SET pastes=pastes+1 WHERE id=?1 AND hash=?2",
            params![id, hash],
        )?;
        Ok(())
    }
    pub fn pin(&mut self, id: i64) -> Result<()> {
        self.db
            .execute("UPDATE clips SET pinned=NOT pinned WHERE id=?1", [id])?;
        self.retain()
    }
    fn retain(&mut self) -> Result<()> {
        self.db.execute("DELETE FROM clips WHERE pinned=0 AND id NOT IN(SELECT id FROM clips WHERE pinned=0 ORDER BY rank DESC LIMIT ?1)",[self.limit as i64])?;
        Ok(())
    }
    pub fn edit(&mut self, id: i64, text: &str) -> Result<()> {
        if text.len() > self.max_clip_bytes {
            return Err(rusqlite::Error::InvalidParameterName(
                "Edit exceeds max_clip_mb".into(),
            ));
        }
        let tx = self.db.transaction()?;
        tx.execute("UPDATE clips SET kind=?1,text=?2,search=?3,payload=NULL,hash=?4,pastes=CASE WHEN hash=?4 THEN pastes ELSE 0 END WHERE id=?5 AND kind IN('text','link','email')",params![crate::clip::detect(text),text,search_text(text),content_hash(crate::clip::detect(text),text.as_bytes()),id])?;
        tx.execute("DELETE FROM formats WHERE clip_id=?1", [id])?;
        tx.commit()
    }
}
