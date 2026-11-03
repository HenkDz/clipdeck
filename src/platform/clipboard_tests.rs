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
        let event = receiver.try_recv();
        if recorded {
            assert!(
                matches!(
                    event,
                    Ok(Event::Clip {
                        own_write: false,
                        ..
                    })
                ),
                "{name:?}"
            );
        } else {
            assert!(event.is_err(), "excluded format {name:?} emitted an event");
        }
    }
    clipboard.write("Clipdeck own write fixture").unwrap();
    thread::sleep(Duration::from_millis(250));
    assert!(
        receiver.try_recv().is_err(),
        "own writes must never reach a store writer"
    );
}
