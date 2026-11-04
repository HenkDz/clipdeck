CREATE TABLE clips(
    id INTEGER PRIMARY KEY, kind TEXT NOT NULL, text TEXT NOT NULL, search TEXT NOT NULL,
    payload BLOB NOT NULL, thumbnail BLOB NOT NULL, source TEXT NOT NULL,
    created INTEGER NOT NULL, updated INTEGER NOT NULL, rank INTEGER NOT NULL,
    pinned INTEGER NOT NULL DEFAULT 0, UNIQUE(kind,payload));
CREATE TABLE formats(clip_id INTEGER NOT NULL REFERENCES clips(id) ON DELETE CASCADE,
    name TEXT NOT NULL,data BLOB NOT NULL, PRIMARY KEY(clip_id,name));
CREATE INDEX clips_order ON clips(pinned DESC,rank DESC);
PRAGMA user_version=1;
CREATE VIRTUAL TABLE clips_fts USING fts5(search,content='clips',content_rowid='id',tokenize='trigram');
CREATE TRIGGER clips_ai AFTER INSERT ON clips BEGIN INSERT INTO clips_fts(rowid,search) VALUES(new.id,new.search); END;
CREATE TRIGGER clips_ad AFTER DELETE ON clips BEGIN INSERT INTO clips_fts(clips_fts,rowid,search) VALUES('delete',old.id,old.search); END;
CREATE TRIGGER clips_au AFTER UPDATE ON clips BEGIN INSERT INTO clips_fts(clips_fts,rowid,search) VALUES('delete',old.id,old.search); INSERT INTO clips_fts(rowid,search) VALUES(new.id,new.search); END;
