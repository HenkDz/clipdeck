use super::{Clip, Store, content_hash, search_text};
use rusqlite::{OptionalExtension, Result, Transaction, params};

fn size(clip: &Clip) -> usize {
    if clip.kind == "image" {
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
    }
}

fn upsert(tx: &Transaction<'_>, clip: &Clip) -> Result<i64> {
    let hash = content_hash(&clip.kind, &clip.payload);
    tx.execute("INSERT INTO clips(kind,text,search,payload,hash,thumbnail,source,created,updated,rank,pinned,file_path,screenshot_title,source_exe,file_kinds)
        VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,(SELECT COALESCE(MAX(rank),0)+1 FROM clips),0,?10,?11,?12,?13)
        ON CONFLICT(kind,hash) DO UPDATE SET text=excluded.text,payload=excluded.payload,search=excluded.search,thumbnail=excluded.thumbnail,source=excluded.source,updated=excluded.updated,copies=clips.copies+1,file_path=CASE WHEN excluded.file_path<>'' THEN excluded.file_path ELSE clips.file_path END,screenshot_title=CASE WHEN excluded.screenshot_title<>'' THEN excluded.screenshot_title ELSE clips.screenshot_title END,source_exe=excluded.source_exe,file_kinds=excluded.file_kinds,rank=(SELECT COALESCE(MAX(rank),0)+1 FROM clips)",
        params![clip.kind,clip.text,search_text(&clip.text),(clip.payload != clip.text.as_bytes()).then_some(&clip.payload),hash,clip.thumbnail,clip.source,clip.created,clip.updated,clip.file_path,clip.screenshot_title,clip.source_exe,clip.file_kinds])?;
    let id = tx.query_row(
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
    Ok(id)
}

fn retain(tx: &Transaction<'_>, limit: usize) -> Result<()> {
    tx.execute("DELETE FROM clips WHERE pinned=0 AND id NOT IN(SELECT id FROM clips WHERE pinned=0 ORDER BY rank DESC LIMIT ?1)", [limit as i64])?;
    Ok(())
}

impl Store {
    // Capture exclusions are checked before this writer; explicit copy completion is separate.
    pub fn record(&mut self, clip: &Clip, own_write: bool, excluded: bool) -> Result<Option<i64>> {
        if own_write || excluded || clip.payload.is_empty() {
            return Ok(None);
        }
        let size = size(clip);
        if size > self.max_clip_bytes {
            self.skipped += 1;
            eprintln!(
                "Skipping clip: {size} bytes exceeds max_clip_mb ({} bytes)",
                self.max_clip_bytes
            );
            return Ok(None);
        }
        let tx = self.db.transaction()?;
        let id = upsert(&tx, clip)?;
        retain(&tx, self.limit)?;
        tx.commit()?;
        Ok(Some(id))
    }

    /// Records a successful explicit copy without re-capturing our clipboard marker.
    pub fn copied(&mut self, clip: &Clip, members: &[(i64, Vec<u8>)]) -> Result<Vec<i64>> {
        let batch = members.len() > 1;
        if batch && (clip.payload.is_empty() || size(clip) > self.max_clip_bytes) {
            return Err(rusqlite::Error::InvalidParameterName(
                "Copied batch exceeds max_clip_mb; raise it in Settings to save this group".into(),
            ));
        }
        let tx = self.db.transaction()?;
        let group: Option<i64> = if batch {
            tx.query_row(
                "SELECT id FROM clips WHERE kind=?1 AND hash=?2",
                params![clip.kind, content_hash(&clip.kind, &clip.payload)],
                |r| r.get(0),
            )
            .optional()?
        } else {
            None
        };
        let mut current = vec![];
        // Promote the original rows in their existing order, then put the batch above them.
        for (id, hash) in members.iter().rev() {
            if Some(*id) != group && !current.contains(id)
                && tx.execute("UPDATE clips SET copies=copies+1,updated=?1,rank=(SELECT COALESCE(MAX(rank),0)+1 FROM clips) WHERE id=?2 AND hash=?3", params![crate::clip::now(),id,hash])? > 0
            {
                current.push(*id);
            }
        }
        if batch {
            current.push(upsert(&tx, clip)?);
        }
        retain(&tx, self.limit)?;
        tx.commit()?;
        Ok(current)
    }
}
