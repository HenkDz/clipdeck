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
    let updates = MenuItem::with_id("updates", "Check for updates", true, None);
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
            let _ = click_sender.send_blocking(Event::TrayOpen(foreground::last_app()));
        }
    }));
    menu.append_items(&[
        &open,
        &pause,
        &settings,
        &updates,
        &PredefinedMenuItem::separator(),
        &quit,
    ])?;
    MenuEvent::set_event_handler(Some(move |event: MenuEvent| {
        let event = match event.id.0.as_str() {
            "open" => Event::TrayOpen(foreground::last_app()),
            "pause" => {
                paused.fetch_xor(true, Ordering::SeqCst);
                Event::Pause
            }
            "settings" => Event::Settings,
            "updates" => Event::Update(crate::update::Action::Check(true)),
            "quit" => Event::Quit,
            _ => return,
        };
        let _ = sender.send_blocking(event);
    }));
    let icon = image::load_from_memory(include_bytes!("../../assets/tray.png"))?.into_rgba8();
    let (width, height) = icon.dimensions();
    Ok(TrayIconBuilder::new()
        .with_tooltip("Clipdeck")
        .with_menu(Box::new(menu))
        .with_menu_on_left_click(false)
        .with_icon(Icon::from_rgba(icon.into_raw(), width, height)?)
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
