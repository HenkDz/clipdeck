mod history;
mod platform;
mod ui;

use global_hotkey::{
    GlobalHotKeyEvent, GlobalHotKeyManager, HotKeyState,
    hotkey::{Code, HotKey, Modifiers},
};
use gpui::*;
use std::{cell::RefCell, rc::Rc};

fn main() {
    let (sender, receiver) = async_channel::unbounded();
    let clipboard = platform::start_watcher(sender.clone()).expect("start clipboard listener");
    let manager = GlobalHotKeyManager::new().expect("create hotkey manager");
    manager
        .register(HotKey::new(
            Some(Modifiers::CONTROL | Modifiers::SHIFT),
            Code::KeyV,
        ))
        .expect("register Ctrl+Shift+V (already in use?)");
    GlobalHotKeyEvent::set_event_handler(Some(move |event: GlobalHotKeyEvent| {
        if event.state == HotKeyState::Released {
            eprintln!("Hotkey released");
            let _ = sender.send_blocking(platform::Event::Open(platform::Target::capture()));
        }
    }));
    Application::with_platform(Rc::new(
        gpui_windows::WindowsPlatform::new(false).expect("Windows GPUI platform"),
    ))
    .run(move |cx: &mut App| {
        gpui_component::init(cx);
        let history = Rc::new(RefCell::new(history::History::default()));
        cx.spawn(async move |cx| {
            let _manager = manager;
            let mut popup: Option<WindowHandle<gpui_component::Root>> = None;
            let mut popup_view: Option<Entity<ui::Popup>> = None;
            while let Ok(event) = receiver.recv().await {
                cx.update(|cx| match event {
                    platform::Event::Clip { text, own_write } => {
                        history.borrow_mut().record(text, own_write);
                        cx.refresh_windows();
                    }
                    platform::Event::Error(error) => eprintln!("{error}"),
                    platform::Event::Open(target) => {
                        eprintln!("Opening popup");
                        if let (Some(handle), Some(view)) = (popup, popup_view.as_ref()) {
                            let _ = handle.update(cx, |_, window, cx| {
                                view.update(cx, |view, cx| view.reopen(target, window, cx))
                            });
                            return;
                        }
                        match cx.open_window(
                            WindowOptions {
                                titlebar: None,
                                kind: WindowKind::Normal,
                                window_bounds: Some(WindowBounds::Windowed(Bounds::new(
                                    point(px(0.), px(0.)),
                                    size(px(480.), px(360.)),
                                ))),
                                is_resizable: false,
                                is_minimizable: false,
                                ..Default::default()
                            },
                            |window, cx| {
                                window.set_window_title("Clipdeck");
                                platform::configure_popup(window);
                                let view = cx.new(|cx| {
                                    ui::Popup::new(history.clone(), clipboard, target, window, cx)
                                });
                                popup_view = Some(view.clone());
                                cx.new(|cx| gpui_component::Root::new(view, window, cx))
                            },
                        ) {
                            Ok(handle) => popup = Some(handle),
                            Err(error) => eprintln!("Open popup: {error}"),
                        }
                    }
                });
            }
        })
        .detach();
    });
}
