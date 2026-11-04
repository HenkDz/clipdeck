use super::{Event, foreground};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use tray_icon::{
    Icon, MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent,
    menu::{CheckMenuItem, Menu, MenuEvent, MenuItem, PredefinedMenuItem},
};

pub fn create(
    sender: async_channel::Sender<Event>,
    paused: Arc<AtomicBool>,
) -> Result<TrayIcon, Box<dyn std::error::Error>> {
    let menu = Menu::new();
    let open = MenuItem::with_id("open", "Open", true, None);
    let pause = CheckMenuItem::with_id("pause", "Pause recording", true, false, None);
    let settings = MenuItem::with_id("settings", "Settings", true, None);
    let quit = MenuItem::with_id("quit", "Quit", true, None);
    // Left click opens the popup; right click shows the menu.
    let click_sender = sender.clone();
    TrayIconEvent::set_event_handler(Some(move |event: TrayIconEvent| {
        if matches!(
            event,
            TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            }
        ) {
            let _ = click_sender.send_blocking(Event::Open(foreground::last_app()));
        }
    }));
    menu.append_items(&[
        &open,
        &pause,
        &settings,
        &PredefinedMenuItem::separator(),
        &quit,
    ])?;
    MenuEvent::set_event_handler(Some(move |event: MenuEvent| {
        let event = match event.id.0.as_str() {
            "open" => Event::Open(foreground::last_app()),
            "pause" => {
                paused.fetch_xor(true, Ordering::SeqCst);
                Event::Pause
            }
            "settings" => Event::Settings,
            "quit" => Event::Quit,
            _ => return,
        };
        let _ = sender.send_blocking(event);
    }));
    let mut rgba = vec![0u8; 32 * 32 * 4];
    for y in 0..32 {
        for x in 0..32 {
            let i = (y * 32 + x) * 4;
            let ink = (7..25).contains(&x) && (5..27).contains(&y);
            rgba[i..i + 4].copy_from_slice(if ink {
                &[237, 235, 228, 255]
            } else {
                &[38, 38, 36, 255]
            });
        }
    }
    Ok(TrayIconBuilder::new()
        .with_tooltip("Clipdeck")
        .with_menu(Box::new(menu))
        .with_menu_on_left_click(false)
        .with_icon(Icon::from_rgba(rgba, 32, 32)?)
        .build()?)
}
pub fn open_external(path: &str) -> Result<(), String> {
    use windows::{
        Win32::UI::{Shell::ShellExecuteW, WindowsAndMessaging::SW_SHOWNORMAL},
        core::{HSTRING, PCWSTR, w},
    };
    let path = HSTRING::from(path);
    let result = unsafe {
        ShellExecuteW(
            None,
            w!("open"),
            PCWSTR(path.as_ptr()),
            None,
            None,
            SW_SHOWNORMAL,
        )
    };
    if result.0 as isize <= 32 {
        Err("Windows could not open this item".into())
    } else {
        Ok(())
    }
}

pub fn open_settings(path: &std::path::Path) -> Result<(), String> {
    use windows::{
        Win32::UI::{Shell::*, WindowsAndMessaging::SW_SHOWNORMAL},
        core::{HSTRING, PCWSTR, w},
    };
    let file = HSTRING::from(path.to_string_lossy().as_ref());
    let mut request = SHELLEXECUTEINFOW {
        cbSize: std::mem::size_of::<SHELLEXECUTEINFOW>() as u32,
        fMask: SEE_MASK_FLAG_NO_UI,
        lpVerb: w!("open"),
        lpFile: PCWSTR(file.as_ptr()),
        nShow: SW_SHOWNORMAL.0,
        ..Default::default()
    };
    if unsafe { ShellExecuteExW(&mut request) }.is_ok() {
        return Ok(());
    }
    std::process::Command::new("notepad.exe")
        .arg(path)
        .spawn()
        .map(|_| ())
        .map_err(|e| e.to_string())
}
