#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) enum Mode {
    Mouse,
    Keyboard,
}
impl Mode {
    pub fn opened(mouse: bool) -> Self {
        if mouse { Self::Mouse } else { Self::Keyboard }
    }
    pub fn search(&mut self) {
        *self = Self::Keyboard;
    }
    pub fn is_mouse(self) -> bool {
        self == Self::Mouse
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn tray_search_and_hotkey_transitions() {
        let mut mode = Mode::opened(true);
        assert!(mode.is_mouse());
        mode.search();
        assert!(!mode.is_mouse());
        mode = Mode::opened(true);
        assert!(mode.is_mouse());
        mode = Mode::opened(false);
        assert!(!mode.is_mouse());
    }
}
