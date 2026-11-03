mod tests;
use crate::clip::{Clip, Format};
use rusqlite::{Connection, Result, params};
use std::path::Path;

pub struct Store {
    db: Connection,
    limit: usize,
    pub fts: bool,
}

pub fn migrate(db: &mut Connection) -> Result<bool> {
    let version: i64 = db.pragma_query_value(None, "user_version", |r| r.get(0))?;
    if version > 1 {
        return Err(rusqlite::Error::InvalidQuery);
    }
    let tx = db.transaction()?;
    tx.execute_batch("CREATE TABLE IF NOT EXISTS clips(
        id INTEGER PRIMARY KEY, kind TEXT NOT NULL, text TEXT NOT NULL, search TEXT NOT NULL,
        payload BLOB NOT NULL, thumbnail BLOB NOT NULL, source TEXT NOT NULL,
        created INTEGER NOT NULL, updated INTEGER NOT NULL, rank INTEGER NOT NULL,
        pinned INTEGER NOT NULL DEFAULT 0, UNIQUE(kind,payload));
        CREATE TABLE IF NOT EXISTS formats(clip_id INTEGER NOT NULL REFERENCES clips(id) ON DELETE CASCADE,
        name TEXT NOT NULL,data BLOB NOT NULL, PRIMARY KEY(clip_id,name));
        CREATE INDEX IF NOT EXISTS clips_order ON clips(pinned DESC,rank DESC);
        PRAGMA user_version=1;")?;
    let fts = tx.execute_batch("CREATE VIRTUAL TABLE IF NOT EXISTS clips_fts USING fts5(search,content='clips',content_rowid='id',tokenize='trigram');
        CREATE TRIGGER IF NOT EXISTS clips_ai AFTER INSERT ON clips BEGIN INSERT INTO clips_fts(rowid,search) VALUES(new.id,new.search); END;
        CREATE TRIGGER IF NOT EXISTS clips_ad AFTER DELETE ON clips BEGIN INSERT INTO clips_fts(clips_fts,rowid,search) VALUES('delete',old.id,old.search); END;
        CREATE TRIGGER IF NOT EXISTS clips_au AFTER UPDATE ON clips BEGIN INSERT INTO clips_fts(clips_fts,rowid,search) VALUES('delete',old.id,old.search); INSERT INTO clips_fts(rowid,search) VALUES(new.id,new.search); END;").is_ok();
    if fts {
        tx.execute_batch("INSERT INTO clips_fts(clips_fts) VALUES('rebuild');")?;
    }
    tx.commit()?;
    Ok(fts)
}

impl Store {
    pub fn open(path: &Path, limit: usize) -> Result<Self> {
        Self::from_connection(Connection::open(path)?, limit)
    }
    fn from_connection(mut db: Connection, limit: usize) -> Result<Self> {
        db.execute_batch("PRAGMA foreign_keys=ON; PRAGMA journal_mode=WAL;")?;
        db.busy_timeout(std::time::Duration::from_secs(2))?;
        let fts = migrate(&mut db)?;
        let mut store = Self { db, limit, fts };
        store.retain()?;
        Ok(store)
    }
    // Only admitted captures and explicit edits/seeds reach this writer. Clipboard exclusions are checked before capture construction.
    pub fn record(&mut self, clip: &Clip, own_write: bool, excluded: bool) -> Result<()> {
        if own_write || excluded || clip.payload.is_empty() {
            return Ok(());
        }
        let tx = self.db.transaction()?;
        tx.execute("INSERT INTO clips(kind,text,search,payload,thumbnail,source,created,updated,rank,pinned)
            VALUES(?1,?2,?3,?4,?5,?6,?7,?8,(SELECT COALESCE(MAX(rank),0)+1 FROM clips),0)
            ON CONFLICT(kind,payload) DO UPDATE SET text=excluded.text,search=excluded.search,thumbnail=excluded.thumbnail,source=excluded.source,updated=excluded.updated,rank=(SELECT COALESCE(MAX(rank),0)+1 FROM clips)",
            params![clip.kind,clip.text,clip.text.to_lowercase(),clip.payload,clip.thumbnail,clip.source,clip.created,clip.updated])?;
        let id: i64 = tx.query_row(
            "SELECT id FROM clips WHERE kind=?1 AND payload=?2",
            params![clip.kind, clip.payload],
            |r| r.get(0),
        )?;
        tx.execute("DELETE FROM formats WHERE clip_id=?1", [id])?;
        for f in &clip.formats {
            tx.execute(
                "INSERT INTO formats VALUES(?1,?2,?3)",
                params![id, f.name, f.data],
            )?;
        }
        tx.execute("DELETE FROM clips WHERE pinned=0 AND id NOT IN(SELECT id FROM clips WHERE pinned=0 ORDER BY rank DESC LIMIT ?1)", [self.limit as i64])?;
        tx.commit()
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
            "SELECT id,kind,text,X'',thumbnail,source,created,updated,pinned FROM clips WHERE id IN (SELECT rowid FROM clips_fts WHERE search LIKE ?1) ORDER BY pinned DESC,rank DESC"
        } else if self.fts {
            "SELECT id,kind,text,X'',thumbnail,source,created,updated,pinned FROM clips WHERE id IN (SELECT rowid FROM clips_fts WHERE search LIKE ?1 ESCAPE '\\') ORDER BY pinned DESC,rank DESC"
        } else {
            "SELECT id,kind,text,X'',thumbnail,source,created,updated,pinned FROM clips WHERE search LIKE ?1 ESCAPE '\\' ORDER BY pinned DESC,rank DESC"
        };
        self.db
            .prepare(sql)?
            .query_map([pattern], |r| {
                Ok(Clip {
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
                })
            })?
            .collect()
    }
    pub fn hydrate(&self, clip: &mut Clip) -> Result<()> {
        clip.payload =
            self.db
                .query_row("SELECT payload FROM clips WHERE id=?1", [clip.id], |r| {
                    r.get(0)
                })?;
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
        let tx = self.db.transaction()?;
        tx.execute("UPDATE clips SET kind=?1,text=?2,search=?3,payload=?4 WHERE id=?5 AND kind IN('text','link','email')",params![crate::clip::detect(text),text,text.to_lowercase(),text.as_bytes(),id])?;
        tx.execute("DELETE FROM formats WHERE clip_id=?1", [id])?;
        tx.commit()
    }
}
