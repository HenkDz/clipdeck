use super::*;

#[test]
#[ignore = "writes the real Windows clipboard; run with the app stopped"]
fn live_listener_privacy_formats_and_own_marker() {
    let (sender, receiver) = async_channel::unbounded();
    let clipboard = start_watcher(sender, Arc::new(AtomicBool::new(false))).unwrap();
    let owner = HWND(clipboard.0 as *mut _);
    let cases = [
        (None, None, true),
        (
            Some("ExcludeClipboardContentFromMonitorProcessing"),
            Some(1u32),
            false,
        ),
        (Some("Clipboard Viewer Ignore"), Some(1u32), false),
        (Some("CanIncludeInClipboardHistory"), Some(0u32), false),
        (Some("CanIncludeInClipboardHistory"), Some(1u32), true),
    ];
    for (name, flag, recorded) in cases {
        while receiver.try_recv().is_ok() {}
        {
            let _guard = open(owner).unwrap();
            unsafe {
                EmptyClipboard().unwrap();
                if let Some(name) = name {
                    put(format(name).unwrap(), &flag.unwrap().to_le_bytes()).unwrap();
                }
                let text: Vec<u8> = "Clipdeck privacy fixture"
                    .encode_utf16()
                    .chain(Some(0))
                    .flat_map(u16::to_le_bytes)
                    .collect();
                put(TEXT, &text).unwrap();
            }
        }
        thread::sleep(Duration::from_millis(250));
        let event = std::iter::from_fn(|| receiver.try_recv().ok())
            .find(|e| matches!(e, Event::Clip { .. }));
        if recorded {
            assert!(
                matches!(
                    event,
                    Some(Event::Clip {
                        own_write: false,
                        ..
                    })
                ),
                "{name:?}"
            );
        } else {
            assert!(event.is_none(), "excluded format {name:?} emitted an event");
        }
    }
    clipboard.write("Clipdeck own write fixture").unwrap();
    thread::sleep(Duration::from_millis(250));
    assert!(
        !std::iter::from_fn(|| receiver.try_recv().ok()).any(|e| matches!(e, Event::Clip { .. })),
        "own writes must never reach a store writer"
    );
    let mut png = std::io::Cursor::new(vec![]);
    image::RgbaImage::from_pixel(3, 2, image::Rgba([30, 60, 90, 120]))
        .write_to(&mut png, image::ImageFormat::Png)
        .unwrap();
    let png = png.into_inner();
    {
        let _guard = open(owner).unwrap();
        unsafe {
            EmptyClipboard().unwrap();
            put(format("PNG").unwrap(), &png).unwrap();
            // PNG must take precedence even if a legacy representation is malformed.
            put(8, b"invalid DIB").unwrap();
        }
    }
    thread::sleep(Duration::from_millis(300));
    let mut clip = std::iter::from_fn(|| receiver.try_recv().ok())
        .find_map(|event| match event {
            Event::Clip { clip, .. } => Some(clip),
            _ => None,
        })
        .expect("PNG capture");
    assert_eq!(clip.payload, png);
    assert!(clip.formats.is_empty());
    assert_eq!(clip.text, "3×2");
    clip.id = 42;
    clipboard.write_clip(&clip, false).unwrap();
    {
        let _guard = open(owner).unwrap();
        unsafe {
            assert_eq!(bytes(format("PNG").unwrap()).unwrap(), png);
            assert_eq!(
                super::super::image::png_from_dib(&bytes(8).unwrap())
                    .unwrap()
                    .2,
                "3×2"
            );
        }
    }
    assert!(is_current(42));
    let mut first = crate::clip::Clip::text("first".into(), "test".into());
    first.id = 43;
    let mut second = crate::clip::Clip::text("second".into(), "test".into());
    second.id = 44;
    clipboard
        .write_clips(&[first.clone(), second], false)
        .unwrap();
    {
        let _guard = open(owner).unwrap();
        let wide = unsafe { bytes(13).unwrap() }
            .as_chunks::<2>()
            .0
            .iter()
            .map(|b| u16::from_le_bytes(*b))
            .take_while(|c| *c != 0)
            .collect::<Vec<_>>();
        assert_eq!(String::from_utf16_lossy(&wide), "first\nsecond");
    }
    assert!(is_current(43) && is_current(44) && !is_current(42));
    let dir = std::env::temp_dir().join(format!("clipdeck-batch-live-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let paths = [dir.join("one, été.txt"), dir.join("two.txt")];
    let mut files = paths
        .iter()
        .enumerate()
        .map(|(ix, path)| {
            std::fs::write(path, b"test").unwrap();
            let mut c = crate::clip::Clip::text(path.to_string_lossy().into(), "test".into());
            c.kind = "files".into();
            c.id = 45 + ix as i64;
            c
        })
        .collect::<Vec<_>>();
    clipboard.write_clips(&files, false).unwrap();
    assert!(
        clipboard
            .write_clips(&[first, files[0].clone()], false)
            .is_err()
    );
    {
        use windows::Win32::UI::Shell::*;
        let _guard = open(owner).unwrap();
        let drop = HDROP(unsafe { GetClipboardData(15).unwrap() }.0);
        assert_eq!(unsafe { DragQueryFileW(drop, u32::MAX, None) }, 2);
        for (ix, path) in paths.iter().enumerate() {
            let mut name = [0u16; 32768];
            let len = unsafe { DragQueryFileW(drop, ix as u32, Some(&mut name)) } as usize;
            assert_eq!(
                std::path::PathBuf::from(String::from_utf16_lossy(&name[..len])),
                *path
            );
        }
    }
    assert!(is_current(45) && is_current(46));
    let db = dir.join("history.db");
    let mut store = crate::store::Store::open(&db, 20, 16).unwrap();
    for clip in &mut files {
        clip.id = store.record(clip, false, false).unwrap().unwrap();
    }
    let members = files
        .iter()
        .map(|c| (c.id, crate::store::content_hash(&c.kind, &c.payload)))
        .collect::<Vec<_>>();
    let group = clipboard.write_clips(&files, false).unwrap();
    let current = store.copied(&group, &members).unwrap();
    set_current_ids(current.clone(), group.sequence);
    let mut saved = store.list("").unwrap().remove(0);
    assert_eq!(saved.file_count, 2);
    assert!(files.iter().all(|c| is_current(c.id)) && is_current(saved.id));
    assert_eq!(store.list("").unwrap().len(), 3);
    assert!(
        store.list("").unwrap()[1..]
            .iter()
            .all(|c| c.copies == 2 && c.pastes == 0)
    );
    store.hydrate(&mut saved).unwrap();
    clipboard.write_clip(&saved, false).unwrap();
    {
        use windows::Win32::UI::Shell::*;
        let _guard = open(owner).unwrap();
        let drop = HDROP(unsafe { GetClipboardData(15).unwrap() }.0);
        assert_eq!(unsafe { DragQueryFileW(drop, u32::MAX, None) }, 2);
        for (ix, path) in paths.iter().enumerate() {
            let mut name = [0u16; 32768];
            let len = unsafe { DragQueryFileW(drop, ix as u32, Some(&mut name)) } as usize;
            assert_eq!(
                std::path::PathBuf::from(String::from_utf16_lossy(&name[..len])),
                *path
            );
        }
    }
    // An older completion must not attach the group's current marker to a later copy.
    clipboard
        .write("Clipdeck batch clipboard verification")
        .unwrap();
    set_current_ids(current, group.sequence);
    assert!(!is_current(saved.id));
    thread::sleep(Duration::from_millis(250));
    assert!(
        !std::iter::from_fn(|| receiver.try_recv().ok()).any(|e| matches!(e, Event::Clip { .. }))
    );
    drop(store);
    std::fs::remove_file(db).unwrap();
    for path in paths {
        std::fs::remove_file(path).unwrap();
    }
    std::fs::remove_dir(dir).unwrap();
}
