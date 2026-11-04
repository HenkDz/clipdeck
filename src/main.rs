#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod clip;
mod config;
mod platform;
mod store;
mod ui;
use global_hotkey::{GlobalHotKeyEvent, GlobalHotKeyManager, HotKeyState};
use gpui::*;
use std::{
    cell::RefCell,
    rc::Rc,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};
fn main() {
    if let Err(error) = run() {
        eprintln!("Clipdeck: {error}");
        std::process::exit(1);
    }
}
fn run() -> Result<(), Box<dyn std::error::Error>> {
    let (sender, receiver) = async_channel::unbounded();
    let Some(instance) = platform::Instance::acquire(sender.clone())? else {
        return Ok(());
    };
    let config = config::Config::load()?;
    let mut store = store::Store::open(
        &config::directory()?.join("history.db"),
        config.history_size,
        config.max_clip_mb,
    )?;
    if std::env::args().any(|arg| arg == "--debug-seed") {
        // Explicit debug fixture bypasses clipboard capture; it contains only generated text.
        for i in 0..5000 {
            store.record(
                &clip::Clip::text(
                    format!("Clipdeck scroll fixture {i:04}"),
                    "Debug fixture".into(),
                ),
                false,
                false,
            )?;
        }
        eprintln!("Seeded fixtures (retention still applies; set history_size >= 5000)");
    }
    eprintln!("SQLite FTS5 trigram search: {}", store.fts);
    let paused = Arc::new(AtomicBool::new(false));
    let clipboard = platform::start_watcher(sender.clone(), paused.clone())?;
    let manager = GlobalHotKeyManager::new()?;
    manager.register(config.hotkey.parse()?)?;
    let hotkey_down = AtomicBool::new(false);
    GlobalHotKeyEvent::set_event_handler(Some(move |event: GlobalHotKeyEvent| {
        if opens_popup(event.state, &hotkey_down) {
            let _ = sender.send_blocking(platform::Event::Open(platform::Target::capture()));
        }
    }));
    Application::with_platform(Rc::new(gpui_windows::WindowsPlatform::new(false)?))
        .with_assets(gpui_kit_assets::AllAssets)
        .run(move |cx: &mut App| {
            gpui_component::init(cx);
            ui::sync_theme(None, cx);
            let store = Rc::new(RefCell::new(store));
            cx.spawn(async move |cx| {
                let (_manager, _instance) = (manager, instance);
                let mut popup: Option<WindowHandle<gpui_component::Root>> = None;
                let mut view: Option<Entity<ui::Popup>> = None;
                while let Ok(event) = receiver.recv().await {
                    cx.update(|cx| match event {
                        platform::Event::Clip { clip, own_write } => {
                            if !paused.load(Ordering::SeqCst) {
                                match store.borrow_mut().record(&clip, own_write, false) {
                                    Ok(Some(id)) => {
                                        platform::set_current(id, clip.sequence);
                                    }
                                    Ok(None) => {}
                                    Err(error) => eprintln!("Store capture: {error}"),
                                }
                                if let Some(view) = &view {
                                    view.update(cx, |v, cx| v.reload(cx));
                                }
                            }
                        }
                        platform::Event::Pause => {
                            cx.refresh_windows();
                        }
                        platform::Event::ClipboardChanged => cx.refresh_windows(),
                        platform::Event::Quit => cx.quit(),
                        platform::Event::Settings => {
                            if let Ok(dir) = config::directory()
                                && let Err(error) =
                                    platform::open_settings(&dir.join("config.toml"))
                            {
                                eprintln!("Settings: {error}");
                            }
                        }
                        platform::Event::Error(error) => eprintln!("{error}"),
                        platform::Event::Open(target) | platform::Event::TrayOpen(target) => {
                            let mouse = matches!(event, platform::Event::TrayOpen(_));
                            if let (Some(handle), Some(view)) = (popup, view.as_ref()) {
                                let _ = handle.update(cx, |_, window, cx| {
                                    view.update(cx, |view, cx| {
                                        view.reopen(target, mouse, window, cx)
                                    })
                                });
                                return;
                            }
                            match cx.open_window(
                                WindowOptions {
                                    titlebar: None,
                                    focus: false,
                                    show: false,
                                    kind: WindowKind::Normal,
                                    window_bounds: Some(WindowBounds::Windowed(Bounds::new(
                                        point(px(0.), px(0.)),
                                        size(px(480.), px(480.)),
                                    ))),
                                    is_resizable: false,
                                    is_minimizable: false,
                                    ..Default::default()
                                },
                                |window, cx| {
                                    window.set_window_title("Clipdeck");
                                    platform::configure_popup(window);
                                    let entity = cx.new(|cx| {
                                        ui::Popup::new(
                                            store.clone(),
                                            clipboard,
                                            target,
                                            paused.clone(),
                                            config.paste_delay_ms,
                                            window,
                                            cx,
                                        )
                                    });
                                    view = Some(entity.clone());
                                    entity.update(cx, |v, cx| v.reopen(target, mouse, window, cx));
                                    cx.new(|cx| gpui_component::Root::new(entity, window, cx))
                                },
                            ) {
                                Ok(handle) => popup = Some(handle),
                                Err(error) => eprintln!("Popup: {error}"),
                            }
                        }
                    });
                }
            })
            .detach();
        });
    Ok(())
}

fn opens_popup(state: HotKeyState, down: &AtomicBool) -> bool {
    if state == HotKeyState::Released {
        down.store(false, Ordering::SeqCst);
        false
    } else {
        !down.swap(true, Ordering::SeqCst)
    }
}
#[cfg(test)]
mod hotkey_tests {
    use super::{AtomicBool, HotKeyState, opens_popup};
    #[test]
    fn one_open_per_held_chord() {
        let down = AtomicBool::new(false);
        let states = [
            HotKeyState::Pressed,
            HotKeyState::Pressed,
            HotKeyState::Pressed,
            HotKeyState::Released,
            HotKeyState::Released,
            HotKeyState::Pressed,
        ];
        assert_eq!(
            states.map(|state| opens_popup(state, &down)),
            [true, false, false, false, false, true]
        );
    }
}
