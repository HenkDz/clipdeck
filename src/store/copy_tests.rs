use super::*;

type Members = Vec<(i64, Vec<u8>)>;

fn fixture() -> (Store, Vec<Clip>, Members) {
    let mut store = Store::from_connection(Connection::open_in_memory().unwrap(), 20, 16).unwrap();
    let mut clips = ["one", "two"]
        .map(|s| Clip::text(s.into(), "test".into()))
        .to_vec();
    for clip in &mut clips {
        clip.id = store.record(clip, false, false).unwrap().unwrap();
    }
    let members = clips
        .iter()
        .map(|c| (c.id, content_hash(&c.kind, &c.payload)))
        .collect();
    (store, clips, members)
}

#[test]
fn copied_batch_promotes_marks_and_deduplicates_without_pasting() {
    let (mut store, clips, members) = fixture();
    let batch = Clip::text("one\ntwo".into(), "Clipdeck selection".into());
    let active = store.copied(&batch, &members).unwrap();
    let rows = store.list("").unwrap();
    let id = rows[0].id;
    assert_eq!(
        rows.iter().map(|c| c.id).collect::<Vec<_>>(),
        [id, clips[0].id, clips[1].id]
    );
    assert_eq!(rows[0].text, "one\ntwo");
    assert_eq!(rows[0].copies, 1);
    assert!(rows[1..].iter().all(|c| c.copies == 2));
    assert!(
        rows.iter()
            .all(|c| c.pastes == 0 && c.plain_pastes == 0 && active.contains(&c.id))
    );
    store.copied(&batch, &members).unwrap();
    let rows = store.list("").unwrap();
    assert_eq!(rows.len(), 3);
    assert_eq!((rows[0].id, rows[0].copies), (id, 2));
    assert!(rows[1..].iter().all(|c| c.copies == 3));
}

#[test]
fn copy_completion_rolls_back_and_ignores_changed_or_deleted_members() {
    let (mut store, clips, members) = fixture();
    let batch = Clip::text("one\ntwo".into(), "Clipdeck selection".into());
    store.db.execute_batch("CREATE TRIGGER fail_group BEFORE INSERT ON clips WHEN new.source='Clipdeck selection' BEGIN SELECT RAISE(ABORT,'fixture'); END").unwrap();
    assert!(store.copied(&batch, &members).is_err());
    assert!(store.list("").unwrap().iter().all(|c| c.copies == 1));
    assert_eq!(store.list("").unwrap().len(), 2);
    store.db.execute_batch("DROP TRIGGER fail_group").unwrap();
    store.edit(clips[0].id, "changed").unwrap();
    store.delete(clips[1].id).unwrap();
    let active = store.copied(&batch, &members).unwrap();
    let rows = store.list("").unwrap();
    assert_eq!(rows.len(), 2);
    assert_eq!(active, [rows[0].id]);
    assert_eq!(rows[1].text, "changed");
    assert_eq!(rows[1].copies, 1);
    assert!(store.copied(&clips[0], &members[..1]).unwrap().is_empty());
}

#[test]
fn single_copy_marks_existing_row_and_does_not_recreate_deleted_content() {
    let (mut store, clips, members) = fixture();
    store.pin(clips[0].id).unwrap();
    let hash = &members[0].1;
    store.mark_pasted(clips[0].id, hash, false).unwrap();
    store.mark_pasted(clips[0].id, hash, true).unwrap();
    assert_eq!(
        store.copied(&clips[0], &members[..1]).unwrap(),
        [clips[0].id]
    );
    let rows = store.list("").unwrap();
    assert_eq!(rows.len(), 2);
    assert_eq!(
        (rows[0].copies, rows[0].pastes, rows[0].plain_pastes),
        (2, 1, 1)
    );
    assert!(rows[0].pinned);
    store.delete(clips[0].id).unwrap();
    assert!(store.copied(&clips[0], &members[..1]).unwrap().is_empty());
    assert_eq!(store.list("").unwrap().len(), 1);
}

#[test]
fn group_matching_a_member_is_counted_once_and_size_limit_rolls_back() {
    let (mut store, clips, mut members) = fixture();
    // Overlapping file selections can reduce to the same payload as an existing row.
    store.copied(&clips[0], &members).unwrap();
    let rows = store.list("").unwrap();
    assert_eq!(rows.len(), 2);
    assert!(rows.iter().all(|c| c.copies == 2));
    store.set_max_clip_mb(0);
    let batch = Clip::text("one\ntwo".into(), "Clipdeck selection".into());
    assert!(store.copied(&batch, &members).is_err());
    assert_eq!(store.list("").unwrap(), rows);
    // Duplicate completion members must never increment one row twice.
    members = vec![members[0].clone(), members[0].clone()];
    store.set_max_clip_mb(16);
    store.copied(&batch, &members).unwrap();
    assert_eq!(
        store
            .list("one")
            .unwrap()
            .iter()
            .find(|c| c.id == clips[0].id)
            .unwrap()
            .copies,
        3
    );
}
