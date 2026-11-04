#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod clip;
mod config;
mod files;
mod platform;
mod store;
mod ui;
mod update;
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
    match update::bootstrap() {
        Ok(true) => return,
        Err(error) => {
            update::error_dialog(&error);
            return;
        }
        Ok(false) => {}
    }
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
    let mut updates = update::Controller::new();
    let update_sender = sender.clone();
    update::schedule(sender.clone());
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
    platform::set_geometry(platform::Geometry {
        size: (config.width, config.height),
        position: config.position,
    });
    platform::set_keep(config.keep_open);
    let paused = Arc::new(AtomicBool::new(false));
    let clipboard = platform::start_watcher(sender.clone(), paused.clone())?;
    let manager = GlobalHotKeyManager::new()?;
    let current: global_hotkey::hotkey::HotKey = config.hotkey.parse()?;
    manager.register(current)?;
    // Settings changes the shortcut live: swap registrations, keeping the old one on failure.
    let hotkeys = Rc::new(RefCell::new((manager, current)));
    let set_hotkey: ui::SetHotkey = Rc::new(move |text: &str| {
        let new: global_hotkey::hotkey::HotKey = text.parse().map_err(|e| format!("{e}"))?;
        let mut keys = hotkeys.borrow_mut();
        let (manager, old) = &mut *keys;
        if new == *old {
            return Ok(());
        }
        manager.unregister(*old).map_err(|e| e.to_string())?;
        if let Err(error) = manager.register(new) {
            let _ = manager.register(*old);
            return Err(error.to_string());
        }
        *old = new;
        Ok(())
    });
    let hotkey_down = AtomicBool::new(false);
    let source_sender = sender.clone();
    let settings_sender = sender.clone();
    GlobalHotKeyEvent::set_event_handler(Some(move |event: GlobalHotKeyEvent| {
        if opens_popup(event.state, &hotkey_down) {
            let _ = sender.send_blocking(platform::Event::Open(platform::Target::capture()));
        }
    }));
    Application::with_platform(Rc::new(gpui_windows::WindowsPlatform::new(false)?))
        .with_assets(gpui_kit_assets::AllAssets)
        .run(move |cx: &mut App| {
            gpui_component::init(cx);
            // No clipboard events are consumed until the update startup handshake succeeds.
            if let Err(error) = update::ready() {
                eprintln!("Update startup: {error}");
                cx.quit();
                return;
            }
            if update::incoming() || std::env::args().any(|arg| arg == "--update-recovered") {
                let _ = update_sender.send_blocking(platform::Event::Open(platform::last_app()));
                let _ = update_sender.send_blocking(platform::Event::ShowUpdates);
            }
            ui::sync_theme(None, cx);
            let store = Rc::new(RefCell::new(store));
            cx.spawn(async move |cx| {
                let (_set_hotkey, _instance) = (set_hotkey.clone(), instance);
                let mut popup: Option<WindowHandle<gpui_component::Root>> = None;
                let mut view: Option<Entity<ui::Popup>> = None;
                while let Ok(event) = receiver.recv().await {
                    cx.update(|cx| match event {
                        platform::Event::Update(action) => {
                            updates.handle(action, &update_sender);
                            if matches!(action, update::Action::Check(true)) {
                                let _ = update_sender
                                    .send_blocking(platform::Event::Open(platform::last_app()));
                                let _ = update_sender.send_blocking(platform::Event::ShowUpdates);
                            }
                            cx.refresh_windows();
                        }
                        platform::Event::UpdateState(state) => {
                            if matches!(state, update::State::Restarting) {
                                cx.quit();
                            }
                            *updates.state.borrow_mut() = state;
                            cx.refresh_windows();
                        }
                        platform::Event::Clip { clip, own_write } => {
                            if !paused.load(Ordering::SeqCst) {
                                match store.borrow_mut().record(&clip, own_write, false) {
                                    Ok(Some(id)) => {
                                        platform::set_current(id, clip.sequence);
                                        if clip.kind == "image" {
                                            let mut clip = clip.clone();
                                            clip.id = id;
                                            let sender = source_sender.clone();
                                            std::thread::spawn(move || {
                                                std::thread::sleep(std::time::Duration::from_secs(
                                                    3,
                                                ));
                                                if let Err(error) =
                                                    platform::locations::resolve_image(&clip, false)
                                                {
                                                    let _ = sender.send_blocking(
                                                        platform::Event::Error(format!(
                                                            "Screenshot location: {error}"
                                                        )),
                                                    );
                                                }
                                                let _ = sender.send_blocking(
                                                    platform::Event::ImageLocationChanged,
                                                );
                                            });
                                        }
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
                        // A drop works like a paste: count it, and close unless kept open.
                        platform::Event::Dropped { id, hash } => {
                            if let Err(error) = store.borrow_mut().mark_pasted(id, &hash, false) {
                                eprintln!("Store drop: {error}");
                            }
                            if let (Some(handle), Some(view)) = (popup, view.as_ref()) {
                                let _ = handle.update(cx, |_, window, cx| {
                                    if !platform::keep() {
                                        platform::hide_popup(window);
                                    }
                                    view.update(cx, |v, cx| v.reload(cx));
                                });
                            }
                        }
                        platform::Event::PasteSent { id, hash, plain } => {
                            if let Err(error) = store.borrow_mut().mark_pasted(id, &hash, plain) {
                                eprintln!("Store paste: {error}");
                            }
                            if let Some(view) = &view {
                                view.update(cx, |v, cx| v.reload(cx));
                            }
                        }
                        platform::Event::PasteBatch { clips, plain } => {
                            if let Err(error) = store.borrow_mut().mark_pasted_many(&clips, plain) {
                                eprintln!("Store paste: {error}");
                            }
                            if let Some(view) = &view {
                                view.update(cx, |v, cx| v.reload(cx));
                            }
                        }
                        platform::Event::Geometry(geometry) => {
                            let (x, y) = geometry.position.unwrap_or_default();
                            if let Err(error) = config::Config::save(&[
                                ("width", geometry.size.0.to_string()),
                                ("height", geometry.size.1.to_string()),
                                ("x", x.to_string()),
                                ("y", y.to_string()),
                            ]) {
                                eprintln!("Save window size: {error}");
                            }
                        }
                        platform::Event::ImageLocationChanged => {
                            if let Some(view) = &view {
                                view.update(cx, |v, cx| v.reload(cx));
                            }
                        }
                        platform::Event::Quit => cx.quit(),
                        // Tray Settings: open the popup for keyboard use, then show Settings.
                        platform::Event::Settings => {
                            let _ = settings_sender
                                .send_blocking(platform::Event::Open(platform::last_app()));
                            let _ = settings_sender.send_blocking(platform::Event::ShowSettings);
                        }
                        platform::Event::ShowSettings | platform::Event::ShowUpdates => {
                            if let (Some(handle), Some(view)) = (popup, view.as_ref()) {
                                let _ = handle.update(cx, |_, window, cx| {
                                    view.update(cx, |view, cx| {
                                        if matches!(event, platform::Event::ShowUpdates) {
                                            view.open_updates(window, cx)
                                        } else {
                                            view.open_settings(window, cx)
                                        }
                                    })
                                });
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
                                        size(px(config.width as f32), px(config.height as f32)),
                                    ))),
                                    // Resizing comes from the platform layer's own frame handling;
                                    // GPUI's resizable frame would offset the content by its borders.
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
                                            ui::Hooks {
                                                paused: paused.clone(),
                                                set_hotkey: set_hotkey.clone(),
                                                updates: updates.state.clone(),
                                                update_action: {
                                                    let sender = update_sender.clone();
                                                    Rc::new(move |action| {
                                                        let _ = sender.send_blocking(
                                                            platform::Event::Update(action),
                                                        );
                                                    })
                                                },
                                            },
                                            config.clone(),
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
