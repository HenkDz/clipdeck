mod crypto;
mod database;
mod http;
mod install;
mod job;
mod manifest;
mod native;
mod stage;

pub use install::{bootstrap, incoming, ready};
pub use native::error_dialog;
use std::{
    cell::RefCell,
    rc::Rc,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

#[derive(Clone)]
pub struct Release {
    pub manifest: manifest::Manifest,
    bytes: Vec<u8>,
    signature: Vec<u8>,
}
#[derive(Clone)]
pub enum State {
    Idle,
    Checking,
    Current,
    Available(Release),
    Downloading(u64, u64),
    Ready(stage::Stage),
    Installing,
    Restarting,
    Message(String),
    Error(String),
}
impl State {
    pub fn busy(&self) -> bool {
        matches!(
            self,
            Self::Checking | Self::Downloading(..) | Self::Installing | Self::Restarting
        )
    }
    pub fn offered(&self) -> bool {
        matches!(self, Self::Available(_) | Self::Ready(_))
    }
}
#[derive(Clone, Copy)]
pub enum Action {
    Check(bool),
    Download,
    Install,
}
pub type Shared = Rc<RefCell<State>>;
pub type SendAction = Rc<dyn Fn(Action)>;
type Work =
    Box<dyn FnOnce(async_channel::Sender<crate::platform::Event>) -> Result<State, String> + Send>;
pub struct Controller {
    pub state: Shared,
}
fn epoch() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}
pub fn save_result(message: &str) {
    if let Ok(dir) = stage::root() {
        let _ = std::fs::create_dir_all(&dir);
        let _ = std::fs::write(dir.join("last-result.txt"), message);
    }
}
impl Controller {
    pub fn new() -> Self {
        let state = if incoming() {
            State::Message(format!("Updated to Clipdeck {}", env!("CARGO_PKG_VERSION")))
        } else {
            stage::root()
                .ok()
                .and_then(|p| std::fs::read_to_string(p.join("last-result.txt")).ok())
                .filter(|s| s.len() < 8192)
                .map(|message| {
                    if message.starts_with("Update failed") {
                        State::Error(message)
                    } else {
                        State::Message(message)
                    }
                })
                .unwrap_or(State::Idle)
        };
        Self {
            state: Rc::new(RefCell::new(state)),
        }
    }
    pub fn handle(
        &mut self,
        action: Action,
        sender: &async_channel::Sender<crate::platform::Event>,
    ) {
        if self.state.borrow().busy() {
            return;
        }
        let (next, work): (State, Work) = match action {
            Action::Check(manual) => {
                if !manual {
                    if !crate::config::Config::load().is_ok_and(|c| c.check_updates) {
                        return;
                    }
                    let last = stage::root()
                        .ok()
                        .and_then(|p| std::fs::read_to_string(p.join("last-check")).ok())
                        .and_then(|s| s.parse::<u64>().ok())
                        .unwrap_or(0);
                    if epoch().saturating_sub(last) < 86_400 {
                        return;
                    }
                }
                (
                    State::Checking,
                    Box::new(|_| {
                        let root = stage::root()?;
                        std::fs::create_dir_all(&root).map_err(|e| e.to_string())?;
                        std::fs::write(root.join("last-check"), epoch().to_string())
                            .map_err(|e| e.to_string())?;
                        let base = "https://github.com/HenkDz/clipdeck/releases/latest/download/clipdeck-update.toml";
                        let bytes = http::bytes(base, 16_384)?;
                        let signature = http::bytes(&format!("{base}.sig"), 64)?;
                        let manifest = manifest::Manifest::verified(&bytes, &signature)?;
                        let current = env!("CARGO_PKG_VERSION").parse::<manifest::Version>()?;
                        Ok(if manifest.version > current {
                            State::Available(Release {
                                manifest,
                                bytes,
                                signature,
                            })
                        } else {
                            State::Current
                        })
                    }),
                )
            }
            Action::Download => {
                let State::Available(release) = &*self.state.borrow() else {
                    return;
                };
                let release = release.clone();
                let size = release.manifest.size;
                (
                    State::Downloading(0, size),
                    Box::new(move |sender| {
                        let mut last = std::time::Instant::now();
                        stage::Stage::download(release, |done| {
                            if last.elapsed() > Duration::from_millis(100) || done == size {
                                let _ = sender.send_blocking(crate::platform::Event::UpdateState(
                                    State::Downloading(done, size),
                                ));
                                last = std::time::Instant::now();
                            }
                        })
                        .map(State::Ready)
                    }),
                )
            }
            Action::Install => {
                let State::Ready(stage) = &*self.state.borrow() else {
                    return;
                };
                let stage = stage.clone();
                (
                    State::Installing,
                    Box::new(move |_| {
                        install::start(stage)?;
                        Ok(State::Restarting)
                    }),
                )
            }
        };
        *self.state.borrow_mut() = next;
        let sender = sender.clone();
        std::thread::spawn(move || {
            let state = work(sender.clone()).unwrap_or_else(State::Error);
            let _ = sender.send_blocking(crate::platform::Event::UpdateState(state));
        });
    }
}
pub fn schedule(sender: async_channel::Sender<crate::platform::Event>) {
    std::thread::spawn(move || {
        loop {
            if sender
                .send_blocking(crate::platform::Event::Update(Action::Check(false)))
                .is_err()
            {
                break;
            }
            std::thread::sleep(Duration::from_secs(3600));
        }
    });
}
