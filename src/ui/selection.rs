use std::collections::BTreeSet;

#[derive(Default)]
pub(super) struct Selection {
    pub focus: Option<i64>,
    anchor: Option<i64>,
    ids: BTreeSet<i64>,
}
impl Selection {
    pub fn contains(&self, id: i64) -> bool {
        self.ids.contains(&id)
    }
    pub fn len(&self) -> usize {
        self.ids.len()
    }
    pub fn sync(&mut self, visible: &[i64]) {
        let visible_ids = visible.iter().copied().collect::<BTreeSet<_>>();
        self.ids.retain(|id| visible_ids.contains(id));
        if self.focus.is_none_or(|id| !visible_ids.contains(&id)) {
            self.focus = visible
                .iter()
                .find(|id| self.ids.contains(id))
                .copied()
                .or_else(|| visible.first().copied());
            if let Some(id) = self.focus {
                self.ids.insert(id);
            }
        }
        if self.anchor.is_none_or(|id| !visible_ids.contains(&id)) {
            self.anchor = self.focus;
        }
    }
    pub fn click(&mut self, visible: &[i64], id: i64, control: bool, shift: bool) {
        if !visible.contains(&id) {
            return;
        }
        if shift {
            let anchor = self.anchor.or(self.focus).unwrap_or(id);
            let a = visible.iter().position(|x| *x == anchor).unwrap_or(0);
            let b = visible.iter().position(|x| *x == id).unwrap();
            if !control {
                self.ids.clear();
            }
            self.ids
                .extend(visible[a.min(b)..=a.max(b)].iter().copied());
            self.anchor = Some(anchor);
        } else {
            if control {
                if !self.ids.remove(&id) {
                    self.ids.insert(id);
                }
            } else {
                self.ids.clear();
                self.ids.insert(id);
            }
            self.anchor = Some(id);
        }
        self.focus = Some(id);
    }
    pub fn navigate(&mut self, visible: &[i64], down: bool, shift: bool, control: bool) {
        if visible.is_empty() {
            return;
        }
        let ix = self
            .focus
            .and_then(|id| visible.iter().position(|v| *v == id))
            .unwrap_or(0);
        let next = if down {
            (ix + 1).min(visible.len() - 1)
        } else {
            ix.saturating_sub(1)
        };
        if control && !shift {
            self.focus = Some(visible[next]);
        } else {
            self.click(visible, visible[next], control, shift);
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn toggles_ranges_and_keyboard_keep_an_anchor() {
        let ids = [10, 20, 30, 40, 50];
        let mut s = Selection::default();
        s.sync(&ids);
        s.click(&ids, 30, true, false);
        assert!(s.contains(10) && s.contains(30));
        s.click(&ids, 10, true, false);
        assert!(!s.contains(10));
        s.click(&ids, 40, false, true);
        assert_eq!(s.ids, BTreeSet::from([10, 20, 30, 40]));
        s.click(&ids, 30, false, false);
        s.navigate(&ids, true, true, false);
        s.navigate(&ids, true, true, false);
        assert_eq!(s.ids, BTreeSet::from([30, 40, 50]));
        s.navigate(&ids, false, true, false);
        assert_eq!(s.ids, BTreeSet::from([30, 40]));
        s.navigate(&ids, false, false, true);
        assert_eq!(s.focus, Some(30));
        assert_eq!(s.len(), 2);
        s.click(&ids, 10, true, true);
        assert_eq!(s.ids, BTreeSet::from([10, 20, 30, 40]));
    }
    #[test]
    fn refresh_keeps_ids_and_empty_toggle_does_not_select_an_unrelated_row() {
        let mut s = Selection::default();
        s.sync(&[1, 2, 3]);
        s.click(&[1, 2, 3], 3, true, false);
        s.sync(&[9, 3, 2, 1]);
        assert_eq!(s.ids, BTreeSet::from([1, 3]));
        assert_eq!(s.focus, Some(3));
        s.sync(&[9, 2, 1]);
        assert_eq!(s.ids, BTreeSet::from([1]));
        assert_eq!(s.focus, Some(1));
        s.click(&[9, 2, 1], 1, true, false);
        s.sync(&[9, 2, 1]);
        assert_eq!(s.len(), 0);
        s.sync(&[]);
        assert_eq!(s.focus, None);
    }
}
