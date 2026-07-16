//! The delimiter prefix table derived from [`TokenRules`].
//!
//! Rebuilt only at state transitions (the parsing state caches it per instance, Phase 3),
//! so the hot token-reading path scans a small pre-sorted table.

use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::fmt;

use crate::state::Lang;

use super::rules::{GroupRule, TokenRules};

/// One delimiter string and the group rules it may open and/or close.
///
/// A single string may be both an opener and a closer — of the same rule (`$…$`) or of
/// different ones. The table merges those into one entry (the WIP's "open-or-close"
/// ambiguity merging); [`StdTokenReader`](super::StdTokenReader) resolves the direction:
/// an expected close (per [`TokenRules::expecting_group_close`]) wins, otherwise the open
/// interpretation does.
pub struct PrefixEntry<L: Lang> {
    delim: String,
    open: Option<Arc<GroupRule<L>>>,
    close: Option<Arc<GroupRule<L>>>,
}

impl<L: Lang> PrefixEntry<L> {
    /// The delimiter string.
    pub fn delim(&self) -> &str {
        &self.delim
    }

    /// The group rule this string opens, if any.
    pub fn open(&self) -> Option<&Arc<GroupRule<L>>> {
        self.open.as_ref()
    }

    /// The group rule this string closes, if any.
    pub fn close(&self) -> Option<&Arc<GroupRule<L>>> {
        self.close.as_ref()
    }
}

/// Sorted delimiter-matching table derived from a [`TokenRules`] value.
///
/// Entries are sorted longest-first so matching is greedy (`$$` before `$`); entries of
/// equal length keep the declaration order — [`TokenRules::temporary_groups`] before
/// [`TokenRules::groups`], each in list order. When two rules claim the same delimiter
/// string in the same direction, the earlier entry wins.
pub struct PrefixTable<L: Lang> {
    entries: Vec<PrefixEntry<L>>,
}

impl<L: Lang> PrefixTable<L> {
    /// Build the table for the group rules of `rules` —
    /// [`temporary_groups`](TokenRules::temporary_groups) first, then
    /// [`groups`](TokenRules::groups), so temporary rules win same-spelling ties (the
    /// minted-rule "prepended wins" semantics). Empty delimiter strings are ignored.
    /// With [`TokenRules::enable_groups`] off the table is empty — the gate is baked in
    /// here (per state, at freeze time) so the hot path never branches on it;
    /// `expecting_group_close` is checked separately by the reader and is *not* gated.
    pub fn for_rules(rules: &TokenRules<L>) -> PrefixTable<L> {
        let mut entries: Vec<PrefixEntry<L>> = Vec::new();
        if !rules.enable_groups {
            return PrefixTable { entries };
        }

        let mut add = |delim: &str, rule: &Arc<GroupRule<L>>, is_open: bool| {
            if delim.is_empty() {
                return;
            }
            let index = match entries.iter().position(|e| e.delim == delim) {
                Some(index) => index,
                None => {
                    entries.push(PrefixEntry { delim: String::from(delim), open: None, close: None });
                    entries.len() - 1
                }
            };
            let entry = &mut entries[index];
            let slot = if is_open { &mut entry.open } else { &mut entry.close };
            if slot.is_none() {
                *slot = Some(Arc::clone(rule));
            }
            // An occupied slot is left alone: earlier rules win.
        };

        for rule in rules.temporary_groups.iter().chain(&rules.groups) {
            add(&rule.open, rule, true);
            add(&rule.close, rule, false);
        }

        // Longest first, for greedy matching; stable, so equal lengths keep declaration order.
        entries.sort_by_key(|entry| core::cmp::Reverse(entry.delim.len()));

        PrefixTable { entries }
    }

    /// The longest entry whose delimiter is a prefix of `rest`, if any.
    pub fn match_at(&self, rest: &str) -> Option<&PrefixEntry<L>> {
        self.entries.iter().find(|e| rest.starts_with(e.delim.as_str()))
    }

    /// The entries, longest delimiter first.
    pub fn entries(&self) -> &[PrefixEntry<L>] {
        &self.entries
    }
}

// Manual impls: derives would demand `L: Clone`/`L: Debug`/`L: PartialEq` although only
// `Arc`-shared rules (whose contents are bounded via `Lang`) are stored.

impl<L: Lang> Clone for PrefixEntry<L> {
    fn clone(&self) -> Self {
        PrefixEntry {
            delim: self.delim.clone(),
            open: self.open.clone(),
            close: self.close.clone(),
        }
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
        PrefixTable { entries: self.entries.clone() }
    }
}

impl<L: Lang> fmt::Debug for PrefixTable<L> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PrefixTable")
            .field("entries", &self.entries)
            .finish()
    }
}

impl<L: Lang> PartialEq for PrefixTable<L> {
    fn eq(&self, other: &Self) -> bool {
        self.entries == other.entries
    }
}

impl<L: Lang> Eq for PrefixTable<L> {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::SimpleLang;
    use crate::token::WhitespaceRules;
    use alloc::vec;
    use alloc::vec::Vec;

    #[derive(Debug, Clone, Copy)]
    struct PlainLang;
    impl SimpleLang for PlainLang {} // GroupTypeId = u32

    fn rules_with_groups(groups: Vec<Arc<GroupRule<PlainLang>>>) -> TokenRules<PlainLang> {
        TokenRules {
            enable_whitespace: false,
            whitespace: WhitespaceRules::default(),
            enable_multi_newline_paragraphs: false,
            enable_groups: true,
            groups,
            temporary_groups: Vec::new(),
            enable_commands: true,
            commands: Vec::new(),
            enable_comments: true,
            comments: Vec::new(),
            enable_specials: true,
            forbidden_chars: "".into(),
            expecting_group_close: None,
        }
    }

    fn group(group_type: u32, open: &str, close: &str) -> Arc<GroupRule<PlainLang>> {
        Arc::new(GroupRule { group_type, open: open.into(), close: close.into() })
    }

    #[test]
    fn braces_directional_entries() {
        let rule = group(0, "{", "}");
        let table = PrefixTable::for_rules(&rules_with_groups(vec![rule.clone()]));

        let open = table.match_at("{x").unwrap();
        assert_eq!(open.delim(), "{");
        assert_eq!(open.open(), Some(&rule));
        assert_eq!(open.close(), None);

        let close = table.match_at("} y").unwrap();
        assert_eq!(close.delim(), "}");
        assert_eq!(close.open(), None);
        assert_eq!(close.close(), Some(&rule));

        assert!(table.match_at("plain").is_none());
    }

    #[test]
    fn same_string_open_and_close_merges() {
        // `$…$`: the same string opens and closes one rule.
        let rule = group(2, "$", "$");
        let table = PrefixTable::for_rules(&rules_with_groups(vec![rule.clone()]));
        let entry = table.match_at("$x").unwrap();
        assert_eq!(entry.open(), Some(&rule));
        assert_eq!(entry.close(), Some(&rule));
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
    fn conflicting_claims_earlier_rule_wins() {
        let first = group(0, "{", "}");
        let table = PrefixTable::for_rules(&rules_with_groups(vec![
            first.clone(),
            group(7, "{", "}"),
        ]));
        let entry = table.match_at("{").unwrap();
        assert_eq!(entry.open(), Some(&first));
    }

    #[test]
    fn temporary_rules_win_same_spelling_ties() {
        // Temporaries are added ahead of `groups` — the minted-rule "prepended wins"
        // semantics (an optional-argument `[` beats a language's own `[` pairing).
        let permanent = group(0, "[", "]");
        let temporary = group(9, "[", "]");
        let mut rules = rules_with_groups(vec![permanent]);
        rules.temporary_groups = vec![temporary.clone()];
        let table = PrefixTable::for_rules(&rules);
        assert_eq!(table.match_at("[x").unwrap().open(), Some(&temporary));
        assert_eq!(table.match_at("]x").unwrap().close(), Some(&temporary));
    }

    #[test]
    fn empty_delimiters_ignored() {
        let table = PrefixTable::for_rules(&rules_with_groups(vec![group(0, "{", "")]));
        assert_eq!(table.entries().len(), 1);
        assert_eq!(table.entries()[0].delim(), "{");
    }
}
