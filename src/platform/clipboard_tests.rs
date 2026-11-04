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
    assert_eq!(current_id(), Some(42));
}
