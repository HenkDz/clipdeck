use super::popup::View;

impl super::Popup {
    pub(super) fn views_mut(&mut self) -> impl Iterator<Item = &mut View> {
        std::iter::once(&mut self.view).chain(self.history.iter_mut().rev())
    }
}

impl View {
    pub(super) fn open(&mut self, history: &mut Vec<Self>, next: Self) {
        // Reopening Settings or Updates must not add another copy to the Back path.
        let same_page = |view: &Self| {
            matches!(
                (view, &next),
                (Self::Settings(_), Self::Settings(_)) | (Self::Updates, Self::Updates)
            )
        };
        if same_page(self) {
            return;
        }
        if let Some(index) = history.iter().rposition(same_page) {
            *self = history.remove(index);
            history.truncate(index);
        } else {
            history.push(std::mem::replace(self, next));
        }
    }

    pub(super) fn back(&mut self, history: &mut Vec<Self>) {
        if let Self::Gallery(gallery) = self
            && gallery.open
            && !gallery.single
        {
            gallery.open = false;
        } else {
            *self = history.pop().unwrap_or(Self::List);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::{popup::Gallery, settings::Settings};

    #[test]
    fn updates_returns_to_settings_with_state_and_reopening_does_not_loop() {
        let mut view = View::List;
        let mut history = vec![];
        view.open(
            &mut history,
            View::Settings(Settings {
                confirm: Some(100),
                ..Default::default()
            }),
        );
        if let View::Settings(settings) = &view {
            settings
                .scroll
                .set_offset(gpui::point(gpui::px(0.), gpui::px(-120.)));
        }
        view.open(&mut history, View::Updates);
        view.open(&mut history, View::Updates);
        assert_eq!(history.len(), 2);
        view.back(&mut history);
        assert!(matches!(&view, View::Settings(s) if s.confirm == Some(100)));
        if let View::Settings(settings) = &view {
            assert_eq!(settings.scroll.offset().y, gpui::px(-120.));
        }
        view.open(&mut history, View::Updates);
        view.open(&mut history, View::Settings(Settings::default()));
        assert!(matches!(&view, View::Settings(s) if s.confirm == Some(100)));
        view.back(&mut history);
        assert!(matches!(view, View::List));
        assert!(history.is_empty());
    }

    #[test]
    fn tray_updates_returns_to_list_and_gallery_back_keeps_its_grid() {
        let mut view = View::List;
        let mut history = vec![];
        view.open(&mut history, View::Updates);
        view.back(&mut history);
        assert!(matches!(view, View::List));
        view.open(
            &mut history,
            View::Gallery(Gallery {
                id: 1,
                files: vec![],
                selected: 3,
                open: true,
                single: false,
            }),
        );
        view.back(&mut history);
        assert!(matches!(&view, View::Gallery(g) if !g.open && g.selected == 3));
        view.back(&mut history);
        assert!(matches!(view, View::List));
    }

    #[test]
    fn nested_screens_return_to_the_same_preview() {
        let mut view = View::List;
        let mut history = vec![];
        view.open(
            &mut history,
            View::Text {
                id: 7,
                text: Some("saved preview".into()),
            },
        );
        view.open(&mut history, View::Settings(Settings::default()));
        view.open(&mut history, View::Updates);
        view.back(&mut history);
        view.back(&mut history);
        assert!(matches!(&view, View::Text { id: 7, text: Some(text) } if text == "saved preview"));
        view.back(&mut history);
        assert!(matches!(view, View::List));
    }
}
