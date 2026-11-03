#[derive(Default)]
pub struct History {
    clips: Vec<String>,
}

impl History {
    pub fn record(&mut self, text: String, own_write: bool) {
        if own_write || text.is_empty() {
            return;
        }
        self.clips.retain(|clip| clip != &text);
        self.clips.insert(0, text);
    }

    pub fn filter(&self, query: &str) -> Vec<String> {
        let query = query.to_lowercase();
        self.clips
            .iter()
            .filter(|clip| clip.to_lowercase().contains(&query))
            .cloned()
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn newest_first_and_deduplicated() {
        let mut h = History::default();
        for text in ["one", "two", "one"] {
            h.record(text.into(), false);
        }
        assert_eq!(h.filter(""), ["one", "two"]);
    }

    #[test]
    fn own_write_does_not_promote_or_insert() {
        let mut h = History::default();
        h.record("one".into(), false);
        h.record("two".into(), false);
        h.record("one".into(), true);
        h.record("new".into(), true);
        assert_eq!(h.filter(""), ["two", "one"]);
    }

    #[test]
    fn substring_case_insensitive_and_empty() {
        let mut h = History::default();
        h.record("".into(), false);
        h.record("Hello ÉTÉ".into(), false);
        assert_eq!(h.filter("éT"), ["Hello ÉTÉ"]);
        assert!(h.filter("missing").is_empty());
    }
}
