//! The delimiter prefix table derived from [`TokenRules`].
//!
//! Rebuilt only at state transitions (the parsing state caches it per instance, Phase 3),
//! so the hot token-reading path scans a small pre-sorted table.

use alloc::string::String;
use alloc::vec::Vec;

use super::rules::{GroupTypeId, TokenRules};

/// One delimiter string and the group types it may open and/or close.
///
/// A single string may be both an opener and a closer — of the same group type (`$…$`) or
/// of different ones. The table merges those into one entry (the WIP's "open-or-close"
/// ambiguity merging); [`StdTokenReader`](super::StdTokenReader) resolves the direction:
/// an expected close (per [`TokenRules::expecting_group_close`]) wins, otherwise the open
/// interpretation does.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrefixEntry {
    delim: String,
    open: Option<GroupTypeId>,
    close: Option<GroupTypeId>,
}

impl PrefixEntry {
    /// The delimiter string.
    pub fn delim(&self) -> &str {
        &self.delim
    }

    /// The group type this string opens, if any.
    pub fn open(&self) -> Option<GroupTypeId> {
        self.open
    }

    /// The group type this string closes, if any.
    pub fn close(&self) -> Option<GroupTypeId> {
        self.close
    }
}

/// Sorted delimiter-matching table derived from a [`TokenRules`] value.
///
/// Entries are sorted longest-first so matching is greedy (`$$` before `$`); entries of
/// equal length keep the [`TokenRules::group_types`] order. When two group types claim the
/// same delimiter string in the same direction, the earlier entry wins.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrefixTable {
    entries: Vec<PrefixEntry>,
    first_chars: String,
}

impl PrefixTable {
    /// Build the table for the group types of `rules`. Empty delimiter strings are ignored.
    pub fn for_rules(rules: &TokenRules) -> PrefixTable {
        let mut entries: Vec<PrefixEntry> = Vec::new();

        let mut add = |delim: &str, id: GroupTypeId, is_open: bool| {
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
    pub fn match_at(&self, rest: &str) -> Option<&PrefixEntry> {
        self.entries.iter().find(|e| rest.starts_with(e.delim.as_str()))
    }

    /// The distinct first characters of all delimiters (used to bound content-character
    /// runs: a run stops at any character that might start a delimiter).
    pub fn first_chars(&self) -> &str {
        &self.first_chars
    }

    /// The entries, longest delimiter first.
    pub fn entries(&self) -> &[PrefixEntry] {
        &self.entries
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::token::rules::GroupType;

    fn rules_with_groups(group_types: Vec<GroupType>) -> TokenRules {
        TokenRules {
            whitespace: None,
            macros: None,
            group_types,
            comments: None,
            paragraph_breaks: false,
            specials: Vec::new(),
            forbidden_chars: String::new(),
            expecting_group_close: None,
        }
    }

    fn group(id: u32, open: &str, close: &str) -> GroupType {
        GroupType { id: GroupTypeId::new(id), open: open.into(), close: close.into() }
    }

    #[test]
    fn braces_directional_entries() {
        let table = PrefixTable::for_rules(&rules_with_groups(vec![group(0, "{", "}")]));

        let open = table.match_at("{x").unwrap();
        assert_eq!(open.delim(), "{");
        assert_eq!(open.open(), Some(GroupTypeId::new(0)));
        assert_eq!(open.close(), None);

        let close = table.match_at("} y").unwrap();
        assert_eq!(close.delim(), "}");
        assert_eq!(close.open(), None);
        assert_eq!(close.close(), Some(GroupTypeId::new(0)));

        assert!(table.match_at("plain").is_none());
    }

    #[test]
    fn same_string_open_and_close_merges() {
        // `$…$`: the same string opens and closes one group type.
        let table = PrefixTable::for_rules(&rules_with_groups(vec![group(2, "$", "$")]));
        let entry = table.match_at("$x").unwrap();
        assert_eq!(entry.open(), Some(GroupTypeId::new(2)));
        assert_eq!(entry.close(), Some(GroupTypeId::new(2)));
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
        assert_eq!(entry.open(), Some(GroupTypeId::new(0)));
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
