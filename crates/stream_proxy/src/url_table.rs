//! The addresses one stream's playlists named, numbered so the player can be given short ones.

use std::collections::HashMap;

use crate::entry_kind::EntryKind;

/// Most addresses one stream may register. A two-hour film in two-second pieces, with a few
/// renditions, is about ten thousand; this leaves room and still bounds the memory.
pub(crate) const MAX_ENTRIES: usize = 20_000;

/// One real address and what it is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Entry {
    pub(crate) url: String,
    pub(crate) kind: EntryKind,
}

/// Numbers addresses; the same address (of the same kind) always gets the same number, so a live
/// playlist that is fetched every few seconds does not grow the table.
#[derive(Debug, Default)]
pub(crate) struct UrlTable {
    entries: Vec<Entry>,
    ids: HashMap<(String, EntryKind), usize>,
}

impl UrlTable {
    /// The number of `url`; `None` when the table is full.
    pub(crate) fn register(&mut self, url: &str, kind: EntryKind) -> Option<usize> {
        if let Some(id) = self.ids.get(&(url.to_owned(), kind)) {
            return Some(*id);
        }
        if self.entries.len() >= MAX_ENTRIES {
            return None;
        }
        let id = self.entries.len();
        self.entries.push(Entry {
            url: url.to_owned(),
            kind,
        });
        self.ids.insert((url.to_owned(), kind), id);
        Some(id)
    }

    /// The address with number `id`.
    pub(crate) fn get(&self, id: usize) -> Option<&Entry> {
        self.entries.get(id)
    }

    /// How many addresses are registered.
    #[cfg(test)]
    pub(crate) fn len(&self) -> usize {
        self.entries.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn register_new_addresses_get_consecutive_numbers_and_come_back() {
        let mut table = UrlTable::default();
        assert_eq!(
            table.register("https://a/1.ts", EntryKind::Segment),
            Some(0)
        );
        assert_eq!(
            table.register("https://a/2.ts", EntryKind::Segment),
            Some(1)
        );
        assert_eq!(
            table.get(1),
            Some(&Entry {
                url: "https://a/2.ts".to_owned(),
                kind: EntryKind::Segment
            })
        );
        assert_eq!(table.get(2), None);
    }

    #[test]
    fn register_the_same_address_again_returns_the_same_number() {
        let mut table = UrlTable::default();
        let first = table.register("https://a/1.ts", EntryKind::Segment);
        assert_eq!(table.register("https://a/1.ts", EntryKind::Segment), first);
        assert_eq!(table.len(), 1);
    }

    #[test]
    fn register_the_same_address_as_another_kind_is_a_new_entry() {
        let mut table = UrlTable::default();
        let segment = table.register("https://a/x", EntryKind::Segment);
        let key = table.register("https://a/x", EntryKind::Key);
        assert_ne!(segment, key);
    }

    #[test]
    fn register_a_full_table_refuses_new_addresses_but_knows_the_old_ones() {
        let mut table = UrlTable::default();
        for n in 0..MAX_ENTRIES {
            assert!(table
                .register(&format!("https://a/{n}.ts"), EntryKind::Segment)
                .is_some());
        }
        assert_eq!(
            table.register("https://a/one-too-many.ts", EntryKind::Segment),
            None
        );
        assert_eq!(
            table.register("https://a/5.ts", EntryKind::Segment),
            Some(5)
        );
    }
}
