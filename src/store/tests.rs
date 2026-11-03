#[cfg(test)]
mod tests {
    use super::super::*;
    fn store(n: usize) -> Store {
        Store::from_connection(Connection::open_in_memory().unwrap(), n).unwrap()
    }
    fn clip(s: &str) -> Clip {
        Clip::text(s.into(), "Test".into())
    }
    #[test]
    fn migrations_and_fts() {
        let mut s = store(3);
        assert!(s.fts);
        assert!(migrate(&mut s.db).unwrap());
        assert_eq!(
            s.db.pragma_query_value(None, "user_version", |r| r.get::<_, i64>(0))
                .unwrap(),
            1
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
            assert_eq!(loaded.formats, c.formats);
            assert_eq!(loaded.payload, c.payload);
            assert_eq!(loaded.thumbnail, c.thumbnail);
        }
    }
    #[test]
    fn edit_delete_and_reopen() {
        let path =
            std::env::temp_dir().join(format!("clipdeck-store-test-{}.db", std::process::id()));
        {
            let mut s = Store::open(&path, 10).unwrap();
            s.record(&clip("before"), false, false).unwrap();
            let id = s.list("").unwrap()[0].id;
            s.edit(id, "https://example.com").unwrap();
        }
        {
            let mut s = Store::open(&path, 10).unwrap();
            let c = s.list("example").unwrap().remove(0);
            assert_eq!(c.kind, "link");
            s.delete(c.id).unwrap();
            assert!(s.list("").unwrap().is_empty());
        }
        std::fs::remove_file(path).unwrap();
    }
}
