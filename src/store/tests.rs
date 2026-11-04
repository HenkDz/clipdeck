#[cfg(test)]
mod cases {
    use super::super::*;
    fn store(n: usize) -> Store {
        Store::from_connection(Connection::open_in_memory().unwrap(), n, 16).unwrap()
    }
    #[test]
    fn v2_migration_failure_rolls_back_columns_and_image_formats() {
        let mut db = Connection::open_in_memory().unwrap();
        db.execute_batch(
            "CREATE TABLE clips(id INTEGER PRIMARY KEY,kind TEXT,file_path TEXT);
            CREATE TABLE formats(clip_id INTEGER,name TEXT,data BLOB);
            INSERT INTO clips VALUES(42,'image','original.png');
            INSERT INTO formats VALUES(42,'CF_DIB',X'010203');
            PRAGMA user_version=2;",
        )
        .unwrap();
        assert!(migrate(&mut db).is_err());
        assert_eq!(
            db.pragma_query_value(None, "user_version", |r| r.get::<_, i64>(0))
                .unwrap(),
            2
        );
        assert!(db.prepare("SELECT copies FROM clips").is_err());
        assert_eq!(
            db.query_row("SELECT data FROM formats", [], |r| r.get::<_, Vec<u8>>(0))
                .unwrap(),
            [1, 2, 3]
        );
    }
    #[test]
    fn v2_images_migrate_without_bitmap_blobs_and_copy_counts_persist() {
        let path =
            std::env::temp_dir().join(format!("clipdeck-v2-migration-{}.db", std::process::id()));
        let db = Connection::open(&path).unwrap();
        db.execute_batch("CREATE TABLE clips(id INTEGER PRIMARY KEY,kind TEXT NOT NULL,text TEXT NOT NULL,search TEXT NOT NULL,payload BLOB,hash BLOB NOT NULL,thumbnail BLOB NOT NULL,source TEXT NOT NULL,created INTEGER NOT NULL,updated INTEGER NOT NULL,rank INTEGER NOT NULL,pinned INTEGER NOT NULL DEFAULT 0,UNIQUE(kind,hash));
            CREATE TABLE formats(clip_id INTEGER NOT NULL REFERENCES clips(id) ON DELETE CASCADE,name TEXT NOT NULL,data BLOB NOT NULL,PRIMARY KEY(clip_id,name));
            PRAGMA user_version=2;").unwrap();
        let png = b"existing canonical PNG";
        db.execute(
            "INSERT INTO clips VALUES(42,'image','2×2','2×2',?1,?2,X'0102','ShareX',123,456,7,1)",
            params![png, content_hash("image", png)],
        )
        .unwrap();
        db.execute(
            "INSERT INTO formats VALUES(42,'CF_DIBV5',zeroblob(19000000))",
            [],
        )
        .unwrap();
        let before = std::fs::metadata(&path).unwrap().len();
        let mut s = Store::from_connection(db, 10, 16).unwrap();
        s.db.execute_batch("PRAGMA wal_checkpoint(TRUNCATE)")
            .unwrap();
        assert!(std::fs::metadata(&path).unwrap().len() < before / 10);
        let mut c = s.list("").unwrap().remove(0);
        s.hydrate(&mut c).unwrap();
        assert_eq!(
            (c.id, c.created, c.updated, c.pinned, c.copies),
            (42, 123, 456, true, 1)
        );
        assert_eq!(c.payload, png);
        assert!(c.formats.is_empty());
        s.record(&c, false, false).unwrap();
        assert_eq!(s.list("").unwrap()[0].copies, 2);
        drop(s);
        let s = Store::open(&path, 10, 16).unwrap();
        assert_eq!(s.list("").unwrap()[0].copies, 2);
        drop(s);
        std::fs::remove_file(path).unwrap();
    }
    fn clip(s: &str) -> Clip {
        Clip::text(s.into(), "Test".into())
    }
    #[test]
    fn v1_migration_preserves_content_and_reclaims_space() {
        let path =
            std::env::temp_dir().join(format!("clipdeck-v1-migration-{}.db", std::process::id()));
        let db = Connection::open(&path).unwrap();
        db.execute_batch(include_str!("v1.sql")).unwrap();
        let text = "x".repeat(5 * 1024 * 1024);
        db.execute(
            "INSERT INTO clips VALUES(42,'text',?1,?1,?2,X'0102','Test',123,456,7,1)",
            params![text, text.as_bytes()],
        )
        .unwrap();
        db.execute(
            "INSERT INTO formats VALUES(42,'Rich Text Format',X'010203')",
            [],
        )
        .unwrap();
        let before = std::fs::metadata(&path).unwrap().len();
        let mut s = Store::from_connection(db, 10, 16).unwrap();
        s.db.execute_batch("PRAGMA wal_checkpoint(TRUNCATE);")
            .unwrap();
        let after = std::fs::metadata(&path).unwrap().len();
        println!("5 MiB text database: v1={before} bytes; v2={after} bytes");
        assert!(after < before / 3);
        assert_eq!(
            s.db.pragma_query_value(None, "user_version", |r| r.get::<_, i64>(0))
                .unwrap(),
            3
        );
        assert!(
            s.db.query_row(
                "SELECT payload IS NULL AND length(hash)=32 AND length(search)=65536 FROM clips",
                [],
                |r| r.get::<_, bool>(0)
            )
            .unwrap()
        );
        let mut c = s.list("xxx").unwrap().remove(0);
        assert_eq!(c.text.len(), 512);
        s.hydrate(&mut c).unwrap();
        assert_eq!(c.text, text);
        assert_eq!(c.payload, text.as_bytes());
        assert_eq!((c.id, c.created, c.updated, c.pinned), (42, 123, 456, true));
        assert_eq!(c.formats[0].data, [1, 2, 3]);
        assert!(
            s.db.prepare("PRAGMA foreign_key_check")
                .unwrap()
                .query([])
                .unwrap()
                .next()
                .unwrap()
                .is_none()
        );
        s.record(&clip(&text), false, false).unwrap();
        assert_eq!(s.list("").unwrap().len(), 1);
        s.delete(42).unwrap();
        assert_eq!(
            s.db.query_row("SELECT count(*) FROM formats", [], |r| r.get::<_, i64>(0))
                .unwrap(),
            0
        );
        drop(s);
        std::fs::remove_file(path).unwrap();
    }
    #[test]
    fn hash_dedupe_preview_search_and_hydration() {
        let mut s = store(10);
        let text = format!("{}tailneedle", "é".repeat(40_000));
        let c = clip(&text);
        s.record(&c, false, false).unwrap();
        let id = s.list("").unwrap()[0].id;
        s.pin(id).unwrap();
        s.record(&c, false, false).unwrap();
        assert_eq!(s.list("").unwrap().len(), 1);
        assert!(s.list("tailneedle").unwrap().is_empty());
        s.fts = false;
        assert!(s.list("tailneedle").unwrap().is_empty());
        let mut loaded = s.list("ééé").unwrap().remove(0);
        assert_eq!(loaded.text.chars().count(), 512);
        assert!(loaded.payload.is_empty() && loaded.formats.is_empty());
        assert!(loaded.pinned);
        s.hydrate(&mut loaded).unwrap();
        assert_eq!(loaded.text, text);
        assert_eq!(loaded.payload, c.payload);
        let mut other_kind = c.clone();
        other_kind.kind = "files".into();
        s.record(&other_kind, false, false).unwrap();
        assert_eq!(s.list("").unwrap().len(), 2);
        s.edit(id, "edited").unwrap();
        s.record(&clip("edited"), false, false).unwrap();
        assert_eq!(s.list("edited").unwrap().len(), 1);
        assert!(search_text(&"İ".repeat(40_000)).len() <= SEARCH_BYTES);
    }
    #[test]
    fn migration_preserves_non_text_payload_and_rolls_back_on_failure() {
        let mut db = Connection::open_in_memory().unwrap();
        db.execute_batch(include_str!("v1.sql")).unwrap();
        db.execute("INSERT INTO clips VALUES(1,'image','dimensions','dimensions',X'010203',X'04','Test',1,2,3,0)",[]).unwrap();
        // A failed conversion leaves the v1 schema and rows intact.
        db.execute_batch("CREATE TABLE clips_v2(sentinel);")
            .unwrap();
        assert!(migrate(&mut db).is_err());
        assert_eq!(
            db.pragma_query_value(None, "user_version", |r| r.get::<_, i64>(0))
                .unwrap(),
            1
        );
        db.execute_batch("DROP TABLE clips_v2;").unwrap();
        let s = Store::from_connection(db, 10, 16).unwrap();
        let mut c = s.list("").unwrap().remove(0);
        s.hydrate(&mut c).unwrap();
        assert_eq!(c.payload, [1, 2, 3]);
        assert_eq!(c.thumbnail, [4]);
        assert_eq!(c.text, "dimensions");
    }
    #[test]
    fn oversize_skip_includes_formats_and_preserves_existing_clip() {
        let mut s = Store::from_connection(Connection::open_in_memory().unwrap(), 10, 1).unwrap();
        let c = clip(&"x".repeat(1024 * 1024));
        s.record(&c, false, false).unwrap();
        s.record(&clip(&"x".repeat(1024 * 1024 + 1)), false, false)
            .unwrap();
        let mut rich = c.clone();
        rich.formats.push(Format {
            name: "RTF".into(),
            data: vec![1],
        });
        s.record(&rich, false, false).unwrap();
        let mut loaded = s.list("").unwrap().remove(0);
        s.hydrate(&mut loaded).unwrap();
        assert!(loaded.formats.is_empty());
        assert_eq!(loaded.payload, c.payload);
        assert_eq!(s.list("").unwrap().len(), 1);
        assert!(s.edit(loaded.id, &"z".repeat(1024 * 1024 + 1)).is_err());
    }
    #[test]
    fn file_count_uses_full_content() {
        let mut s = store(10);
        let mut c = clip(&format!("{}\nsecond", "x".repeat(600)));
        c.kind = "files".into();
        s.record(&c, false, false).unwrap();
        let loaded = s.list("").unwrap().remove(0);
        assert_eq!(loaded.text.len(), 512);
        assert!(loaded.meta().starts_with("2 files"));
        s.delete(loaded.id).unwrap();
        c.text = "one".into();
        c.payload = b"one".to_vec();
        s.record(&c, false, false).unwrap();
        assert!(s.list("").unwrap()[0].meta().starts_with("1 file ·"));
    }
    #[test]
    fn migrations_and_fts() {
        let mut s = store(3);
        assert!(s.fts);
        assert!(migrate(&mut s.db).unwrap());
        assert_eq!(
            s.db.pragma_query_value(None, "user_version", |r| r.get::<_, i64>(0))
                .unwrap(),
            3
        );
        s.db.pragma_update(None, "user_version", 99).unwrap();
        assert!(migrate(&mut s.db).is_err());
    }
    #[test]
    fn retention_keeps_pinned_and_promotes() {
        let mut s = store(2);
        s.record(&clip("pin"), false, false).unwrap();
        let id = s.list("").unwrap()[0].id;
        s.pin(id).unwrap();
        for t in ["one", "two", "one", "three"] {
            s.record(&clip(t), false, false).unwrap();
        }
        assert_eq!(
            s.list("")
                .unwrap()
                .iter()
                .map(|c| c.text.as_str())
                .collect::<Vec<_>>(),
            ["pin", "three", "one"]
        );
        s.pin(id).unwrap();
        assert_eq!(s.list("").unwrap().len(), 2);
    }
    #[test]
    fn privacy_and_search() {
        let mut s = store(10);
        s.record(&clip("excluded"), false, true).unwrap();
        s.record(&clip("own"), true, false).unwrap();
        s.record(&clip("Hello ÉTÉ 100%"), false, false).unwrap();
        assert_eq!(s.list("été").unwrap().len(), 1);
        assert_eq!(s.list("%").unwrap().len(), 1);
        assert!(s.list("_").unwrap().is_empty());
        assert_eq!(s.list("").unwrap().len(), 1);
    }
    #[test]
    fn all_types_round_trip() {
        let mut s = store(10);
        for kind in ["text", "email", "link", "image", "files"] {
            let mut c = clip(kind);
            c.kind = kind.into();
            c.thumbnail = vec![1, 2];
            c.formats = vec![Format {
                name: "HTML Format".into(),
                data: vec![0, 1, 255],
            }];
            s.record(&c, false, false).unwrap();
            let mut loaded = s.list(kind).unwrap().remove(0);
            s.hydrate(&mut loaded).unwrap();
            assert_eq!(loaded.kind, kind);
            if kind == "image" {
                assert!(loaded.formats.is_empty());
            } else {
                assert_eq!(loaded.formats, c.formats);
            }
            assert_eq!(loaded.payload, c.payload);
            assert_eq!(loaded.thumbnail, c.thumbnail);
        }
    }
    #[test]
    fn edit_delete_and_reopen() {
        let path =
            std::env::temp_dir().join(format!("clipdeck-store-test-{}.db", std::process::id()));
        {
            let mut s = Store::open(&path, 10, 16).unwrap();
            s.record(&clip("before"), false, false).unwrap();
            let id = s.list("").unwrap()[0].id;
            s.edit(id, "https://example.com").unwrap();
        }
        {
            let mut s = Store::open(&path, 10, 16).unwrap();
            let c = s.list("example").unwrap().remove(0);
            assert_eq!(c.kind, "link");
            s.delete(c.id).unwrap();
            assert!(s.list("").unwrap().is_empty());
        }
        std::fs::remove_file(path).unwrap();
    }
}
