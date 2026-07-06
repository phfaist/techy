//! The delimiter prefix table derived from [`TokenRules`].
//!
//! Rebuilt only at state transitions (the parsing state caches it per instance, Phase 3),
//! so the hot token-reading path scans a small pre-sorted table.

use alloc::string::String;
use alloc::vec::Vec;
use core::fmt;

use crate::state::Lang;

use super::rules::TokenRules;

/// One delimiter string and the group types it may open and/or close.
///
/// A single string may be both an opener and a closer — of the same group type (`$…$`) or
/// of different ones. The table merges those into one entry (the WIP's "open-or-close"
/// ambiguity merging); [`StdTokenReader`](super::StdTokenReader) resolves the direction:
/// an expected close (per [`TokenRules::expecting_group_close`]) wins, otherwise the open
/// interpretation does.
pub struct PrefixEntry<L: Lang> {
    delim: String,
    open: Option<L::GroupTypeId>,
    close: Option<L::GroupTypeId>,
}

impl<L: Lang> PrefixEntry<L> {
    /// The delimiter string.
    pub fn delim(&self) -> &str {
        &self.delim
    }

    /// The group type this string opens, if any.
    pub fn open(&self) -> Option<L::GroupTypeId> {
        self.open
    }

    /// The group type this string closes, if any.
    pub fn close(&self) -> Option<L::GroupTypeId> {
        self.close
    }
}

/// Sorted delimiter-matching table derived from a [`TokenRules`] value.
///
/// Entries are sorted longest-first so matching is greedy (`$$` before `$`); entries of
/// equal length keep the [`TokenRules::group_types`] order. When two group types claim the
/// same delimiter string in the same direction, the earlier entry wins.
pub struct PrefixTable<L: Lang> {
    entries: Vec<PrefixEntry<L>>,
    first_chars: String,
}

impl<L: Lang> PrefixTable<L> {
    /// Build the table for the group types of `rules`. Empty delimiter strings are ignored.
    pub fn for_rules(rules: &TokenRules<L>) -> PrefixTable<L> {
        let mut entries: Vec<PrefixEntry<L>> = Vec::new();

        let mut add = |delim: &str, id: L::GroupTypeId, is_open: bool| {
            if delim.is_empty() {
                return;
            }
            let entry = match entries.iter_mut().find(|e| e.delim == delim) {
                Some(entry) => entry,
                None => {
                    entries.push(PrefixEntry { delim: String::from(delim), open: None, close: None });
                    entries.last_mut().expect("just pushed")
                }
            };
            let slot = if is_open { &mut entry.open } else { &mut entry.close };
            if slot.is_none() {
                *slot = Some(id);
            }
            // An occupied slot is left alone: earlier group types win.
        };

        for group_type in &rules.group_types {
            add(&group_type.open, group_type.id, true);
            add(&group_type.close, group_type.id, false);
        }

        // Longest first, for greedy matching; stable, so equal lengths keep declaration order.
        entries.sort_by(|a, b| b.delim.len().cmp(&a.delim.len()));

        let mut first_chars = String::new();
        for entry in &entries {
            let c = entry.delim.chars().next().expect("empty delimiters were skipped");
            if !first_chars.contains(c) {
                first_chars.push(c);
            }
        }

        PrefixTable { entries, first_chars }
    }

    /// The longest entry whose delimiter is a prefix of `rest`, if any.
    pub fn match_at(&self, rest: &str) -> Option<&PrefixEntry<L>> {
        self.entries.iter().find(|e| rest.starts_with(e.delim.as_str()))
    }

    /// The distinct first characters of all delimiters (used to bound content-character
    /// runs: a run stops at any character that might start a delimiter).
    pub fn first_chars(&self) -> &str {
        &self.first_chars
    }

    /// The entries, longest delimiter first.
    pub fn entries(&self) -> &[PrefixEntry<L>] {
        &self.entries
    }
}

// Manual impls: derives would demand `L: Clone`/`L: Debug`/`L: PartialEq` although only
// the `Lang::GroupTypeId` associated type (already bounded) is stored.

impl<L: Lang> Clone for PrefixEntry<L> {
    fn clone(&self) -> Self {
        PrefixEntry { delim: self.delim.clone(), open: self.open, close: self.close }
    }
}

impl<L: Lang> fmt::Debug for PrefixEntry<L> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PrefixEntry")
            .field("delim", &self.delim)
            .field("open", &self.open)
            .field("close", &self.close)
            .finish()
    }
}

impl<L: Lang> PartialEq for PrefixEntry<L> {
    fn eq(&self, other: &Self) -> bool {
        self.delim == other.delim && self.open == other.open && self.close == other.close
    }
}

impl<L: Lang> Eq for PrefixEntry<L> {}

impl<L: Lang> Clone for PrefixTable<L> {
    fn clone(&self) -> Self {
        PrefixTable { entries: self.entries.clone(), first_chars: self.first_chars.clone() }
    }
}

impl<L: Lang> fmt::Debug for PrefixTable<L> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PrefixTable")
            .field("entries", &self.entries)
            .field("first_chars", &self.first_chars)
            .finish()
    }
}

impl<L: Lang> PartialEq for PrefixTable<L> {
    fn eq(&self, other: &Self) -> bool {
        self.entries == other.entries && self.first_chars == other.first_chars
    }
}

impl<L: Lang> Eq for PrefixTable<L> {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::SimpleLang;
    use crate::token::rules::GroupType;
    use alloc::vec;
    use alloc::vec::Vec;

    #[derive(Debug, Clone, Copy)]
    struct PlainLang;
    impl SimpleLang for PlainLang {} // GroupTypeId = u32

    fn rules_with_groups(group_types: Vec<GroupType<PlainLang>>) -> TokenRules<PlainLang> {
        TokenRules {
            whitespace: None,
            double_newline_paragraphs: false,
            group_types,
            commands: Vec::new(),
            comments: Vec::new(),
            forbidden_chars: String::new(),
            expecting_group_close: None,
        }
    }

    fn group(id: u32, open: &str, close: &str) -> GroupType<PlainLang> {
        GroupType { id, open: open.into(), close: close.into() }
    }

    #[test]
    fn braces_directional_entries() {
        let table = PrefixTable::for_rules(&rules_with_groups(vec![group(0, "{", "}")]));

        let open = table.match_at("{x").unwrap();
        assert_eq!(open.delim(), "{");
        assert_eq!(open.open(), Some(0));
        assert_eq!(open.close(), None);

        let close = table.match_at("} y").unwrap();
        assert_eq!(close.delim(), "}");
        assert_eq!(close.open(), None);
        assert_eq!(close.close(), Some(0));

        assert!(table.match_at("plain").is_none());
    }

    #[test]
    fn same_string_open_and_close_merges() {
        // `$…$`: the same string opens and closes one group type.
        let table = PrefixTable::for_rules(&rules_with_groups(vec![group(2, "$", "$")]));
        let entry = table.match_at("$x").unwrap();
        assert_eq!(entry.open(), Some(2));
        assert_eq!(entry.close(), Some(2));
    }

    #[test]
    fn longest_delimiter_matches_first() {
        let table = PrefixTable::for_rules(&rules_with_groups(vec![
            group(2, "$", "$"),
            group(3, "$$", "$$"),
        ]));
        assert_eq!(table.match_at("$$x").unwrap().delim(), "$$");
        assert_eq!(table.match_at("$x").unwrap().delim(), "$");
    }

    #[test]
    fn conflicting_claims_earlier_group_type_wins() {
        let table = PrefixTable::for_rules(&rules_with_groups(vec![
            group(0, "{", "}"),
            group(7, "{", "}"),
        ]));
        let entry = table.match_at("{").unwrap();
        assert_eq!(entry.open(), Some(0));
    }

    #[test]
    fn first_chars_deduplicated() {
        let table = PrefixTable::for_rules(&rules_with_groups(vec![
            group(0, "{", "}"),
            group(2, "$", "$"),
            group(3, "$$", "$$"),
            group(4, r"\(", r"\)"),
        ]));
        let mut chars: Vec<char> = table.first_chars().chars().collect();
        chars.sort_unstable();
        assert_eq!(chars, vec!['$', '\\', '{', '}']);
    }

    #[test]
    fn empty_delimiters_ignored() {
        let table = PrefixTable::for_rules(&rules_with_groups(vec![group(0, "{", "")]));
        assert_eq!(table.entries().len(), 1);
        assert_eq!(table.entries()[0].delim(), "{");
    }
}
