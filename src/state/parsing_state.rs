//! [`ParsingState`] and its stored [`StateData`].

use alloc::sync::Arc;
use core::fmt;

use crate::library::LibraryStack;
use crate::token::{PrefixTable, TokenRules, TriggerChars};

use super::delta::ParsingStateDelta;
use super::lang::Lang;

/// The plain stored settings of a parsing state — the data that deltas override and
/// [`Lang::finalize_transition`] may rewrite. Fields are public *here* (the customizer
/// needs full access); the outer [`ParsingState`] exposes them read-only.
pub struct StateData<L: Lang> {
    /// Tokenization rules — plain stored data (defined in the token topic).
    pub rules: TokenRules<L>,
    /// The definitions visible in this state (extendable mid-parse via
    /// [`push_library`](super::ParsingStateDelta::push_library) — `\newcommand`).
    pub libraries: LibraryStack<L>,
    /// Language-specific state (e.g. a math-mode flag).
    pub ext: L::StateExt,
}

/// An immutable parsing state: [`StateData`] behind a getter-only surface, plus derived
/// caches valid for this instance's lifetime.
///
/// The **only** way a non-initial state comes into existence is
/// [`derived()`](ParsingState::derived) — the transition choke point. States are cheaply
/// shareable; the engine wraps them in `Arc` and creates a new one only at transitions,
/// so nodes can record their parse-time state (ARCHITECTURE.md §state).
///
/// # Derived caches
///
/// The delimiter [`PrefixTable`] and the specials [`TriggerChars`] filter are computed
/// eagerly when the state is frozen (constructor / end of `derived()`), not lazily on
/// first use: the crate is `no_std` (`core` has no `OnceLock`, and `OnceCell` would make
/// states non-`Sync`). Eager rebuilds are a real fraction of a transition's cost, so
/// [`derived()`](ParsingState::derived) reuses the parent's `PrefixTable` (held behind
/// `Arc`) whenever its inputs — `enable_groups` and the `groups` rules, by `Arc`
/// identity — are unchanged (July 2026). No analogous generic reuse rule exists for
/// `TriggerChars`: its inputs include `L::StateExt`, which carries no `Eq` bound (see
/// [`Lang::specials_trigger_chars`]).
pub struct ParsingState<L: Lang> {
    data: StateData<L>,
    prefix_table: Arc<PrefixTable<L>>,
    trigger_chars: TriggerChars,
}

impl<L: Lang> ParsingState<L> {
    /// The seed state: [`Lang::initial_state_data`] frozen — the one public path from
    /// data to state, so every state a parse sees is either this seed or a
    /// [`derived()`](ParsingState::derived) descendant that passed through
    /// [`Lang::finalize_transition`]. Callers customize the starting point by deriving
    /// from the seed with a delta. The seed itself does *not* run `finalize_transition`
    /// (it has no predecessor); its coherence is the language author's contract (the
    /// hook's docs).
    pub fn initial() -> ParsingState<L> {
        ParsingState::freeze(L::initial_state_data())
    }

    /// Create a state directly from raw data, bypassing [`Lang::finalize_transition`].
    /// Test-internal: tests assemble ad-hoc states; the public paths are
    /// [`initial()`](ParsingState::initial) and [`derived()`](ParsingState::derived),
    /// which keep the choke point airtight.
    #[cfg(test)]
    pub(crate) fn new(data: StateData<L>) -> ParsingState<L> {
        ParsingState::freeze(data)
    }

    /// The sole constructor of non-initial states — the transition choke point.
    ///
    /// Applies the delta's overrides to a copy of this state's data, runs
    /// [`Lang::finalize_transition`] exactly once, and freezes the result. Derived
    /// caches are rebuilt — except the [`PrefixTable`], which is reused from `self`
    /// (an `Arc` clone) when its inputs are unchanged: same `enable_groups`, same
    /// `groups` rules by elementwise `Arc` identity. The dominant transition — a group
    /// interior overriding only `expecting_group_close`, which is deliberately not a
    /// table input — takes the reuse path. Functional contract: `self` is never
    /// observably mutated.
    pub fn derived(&self, delta: &ParsingStateDelta<L>) -> ParsingState<L> {
        let mut data = self.data.clone();
        delta.apply_overrides(&mut data);
        L::finalize_transition(&mut data, self, &delta.events);
        // Checked *after* finalize_transition: the customizer may rewrite `groups` too.
        let table_inputs_unchanged = data.rules.enable_groups == self.data.rules.enable_groups
            && data.rules.groups.len() == self.data.rules.groups.len()
            && data
                .rules
                .groups
                .iter()
                .zip(&self.data.rules.groups)
                .all(|(new, old)| Arc::ptr_eq(new, old));
        if table_inputs_unchanged {
            ParsingState::freeze_with_table(data, Arc::clone(&self.prefix_table))
        } else {
            ParsingState::freeze(data)
        }
    }

    /// The tokenization rules in effect.
    pub fn rules(&self) -> &TokenRules<L> {
        &self.data.rules
    }

    /// The definitions visible in this state.
    pub fn libraries(&self) -> &LibraryStack<L> {
        &self.data.libraries
    }

    /// The language-specific state extension.
    pub fn ext(&self) -> &L::StateExt {
        &self.data.ext
    }

    /// The delimiter-matching table derived from [`rules().groups`](TokenRules) — empty
    /// when [`TokenRules::enable_groups`] is off (the gate is baked in at freeze time).
    pub fn prefix_table(&self) -> &PrefixTable<L> {
        &self.prefix_table
    }

    /// The specials trigger-character filter derived via
    /// [`Lang::specials_trigger_chars`] — the empty filter when
    /// [`TokenRules::enable_specials`] is off (the gate is baked in at freeze time).
    pub fn trigger_chars(&self) -> &TriggerChars {
        &self.trigger_chars
    }

    fn freeze(data: StateData<L>) -> ParsingState<L> {
        let prefix_table = Arc::new(PrefixTable::for_rules(&data.rules));
        ParsingState::freeze_with_table(data, prefix_table)
    }

    fn freeze_with_table(data: StateData<L>, prefix_table: Arc<PrefixTable<L>>) -> ParsingState<L> {
        // The enable_specials gate is baked in here: a disabled state stores the empty
        // filter, so `Lang::scan_specials` is unreachable and the hot path never
        // branches on the flag (same treatment as enable_groups in the prefix table).
        let trigger_chars = if data.rules.enable_specials {
            L::specials_trigger_chars(&data)
        } else {
            TriggerChars::default()
        };
        ParsingState { data, prefix_table, trigger_chars }
    }
}

// Manual impls: derives would demand `L: Clone`/`L: Debug` although only the associated
// types (already bounded in `Lang`) are stored.

impl<L: Lang> Clone for StateData<L> {
    fn clone(&self) -> Self {
        StateData {
            rules: self.rules.clone(),
            libraries: self.libraries.clone(),
            ext: self.ext.clone(),
        }
    }
}

impl<L: Lang> fmt::Debug for StateData<L> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("StateData")
            .field("rules", &self.rules)
            .field("libraries", &self.libraries)
            .field("ext", &self.ext)
            .finish()
    }
}

impl<L: Lang> Clone for ParsingState<L> {
    fn clone(&self) -> Self {
        ParsingState {
            data: self.data.clone(),
            prefix_table: Arc::clone(&self.prefix_table),
            trigger_chars: self.trigger_chars.clone(),
        }
    }
}

impl<L: Lang> fmt::Debug for ParsingState<L> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // The caches are derived data; `dbg!(state)` shows the settings that determine
        // behavior (the Option C debuggability promise).
        f.debug_struct("ParsingState")
            .field("rules", &self.data.rules)
            .field("libraries", &self.data.libraries)
            .field("ext", &self.data.ext)
            .finish_non_exhaustive()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::library::LibraryStack;
    use crate::state::TokenRulesOverrides;
    use crate::token::{CommandRule, GroupRule, WhitespaceRules};
    use alloc::string::String;
    use alloc::sync::Arc;
    use alloc::vec;
    use alloc::vec::Vec;

    fn base_rules<L: Lang<GroupTypeId = u32>>() -> TokenRules<L> {
        TokenRules {
            enable_whitespace: true,
            whitespace: WhitespaceRules { chars: " \t\n".into() },
            enable_multi_newline_paragraphs: true,
            enable_groups: true,
            groups: vec![Arc::new(GroupRule {
                group_type: 0,
                open: "{".into(),
                close: "}".into(),
            })],
            enable_commands: true,
            commands: vec![Arc::new(CommandRule {
                escape_char: '\\',
                name_chars: "abcdefghijklmnopqrstuvwxyz".into(),
            })],
            enable_comments: true,
            comments: Vec::new(),
            enable_specials: true,
            forbidden_chars: "".into(),
            expecting_group_close: None,
        }
    }

    // --- a minimal lang: no customization at all (the SimpleLang shortcut) -----------

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    struct PlainLang;
    impl crate::state::SimpleLang for PlainLang {}

    #[test]
    fn derived_applies_overrides_and_keeps_the_rest() {
        let state: ParsingState<PlainLang> =
            ParsingState::new(StateData { rules: base_rules(), libraries: LibraryStack::new(), ext: () });

        let delta = ParsingStateDelta::new().rules(TokenRulesOverrides {
            enable_multi_newline_paragraphs: Some(false),
            forbidden_chars: Some("$".into()),
            ..TokenRulesOverrides::default()
        });
        let derived = state.derived(&delta);

        assert!(!derived.rules().enable_multi_newline_paragraphs);
        assert_eq!(&*derived.rules().forbidden_chars, "$");
        // Unchanged fields kept; the original state is untouched.
        assert_eq!(derived.rules().commands, state.rules().commands);
        assert!(state.rules().enable_multi_newline_paragraphs);
    }

    #[test]
    fn derived_rebuilds_prefix_table() {
        let state: ParsingState<PlainLang> =
            ParsingState::new(StateData { rules: base_rules(), libraries: LibraryStack::new(), ext: () });
        assert!(state.prefix_table().match_at("[x").is_none());

        let delta = ParsingStateDelta::new().rules(TokenRulesOverrides {
            groups: Some(vec![Arc::new(GroupRule {
                group_type: 1,
                open: "[".into(),
                close: "]".into(),
            })]),
            ..TokenRulesOverrides::default()
        });
        let derived = state.derived(&delta);
        assert!(derived.prefix_table().match_at("[x").is_some());
        assert!(derived.prefix_table().match_at("{x").is_none()); // whole-value override
    }

    #[test]
    fn derived_reuses_prefix_table_when_inputs_unchanged() {
        let state: ParsingState<PlainLang> =
            ParsingState::new(StateData { rules: base_rules(), libraries: LibraryStack::new(), ext: () });

        // A transition that touches neither enable_groups nor groups (by Arc identity)
        // shares the parent's table instance…
        let same = state.derived(&ParsingStateDelta::new().rules(TokenRulesOverrides {
            enable_comments: Some(false),
            ..TokenRulesOverrides::default()
        }));
        assert!(core::ptr::eq(state.prefix_table(), same.prefix_table()));

        // …and so does the dominant group-interior transition (expecting_group_close is
        // deliberately not a table input)…
        let interior = state.derived(&ParsingStateDelta::new().rules(TokenRulesOverrides {
            expecting_group_close: Some(Some(Arc::clone(&state.rules().groups[0]))),
            ..TokenRulesOverrides::default()
        }));
        assert!(core::ptr::eq(state.prefix_table(), interior.prefix_table()));

        // …while a groups override — even one value-equal to the current rules — builds
        // a fresh table (reuse is keyed on Arc identity, not content).
        let equal_groups: Vec<Arc<GroupRule<PlainLang>>> = state
            .rules()
            .groups
            .iter()
            .map(|rule| Arc::new(GroupRule::clone(rule)))
            .collect();
        let rebuilt = state.derived(&ParsingStateDelta::new().rules(TokenRulesOverrides {
            groups: Some(equal_groups),
            ..TokenRulesOverrides::default()
        }));
        assert!(!core::ptr::eq(state.prefix_table(), rebuilt.prefix_table()));
        assert_eq!(state.prefix_table(), rebuilt.prefix_table()); // same contents
    }

    #[test]
    fn empty_delta_is_a_clean_copy() {
        let state: ParsingState<PlainLang> =
            ParsingState::new(StateData { rules: base_rules(), libraries: LibraryStack::new(), ext: () });
        let derived = state.derived(&ParsingStateDelta::new());
        assert_eq!(derived.rules(), state.rules());
    }

    #[test]
    fn default_initial_state_is_neutral() {
        // The default `Lang::initial_state_data`: every syntax gate off, no libraries.
        let state: ParsingState<PlainLang> = ParsingState::initial();
        assert!(!state.rules().enable_whitespace);
        assert!(!state.rules().enable_groups);
        assert!(!state.rules().enable_commands);
        assert!(!state.rules().enable_comments);
        assert!(!state.rules().enable_specials);
        assert!(state.libraries().is_empty());
        assert!(state.prefix_table().match_at("{x").is_none());
    }

    // --- a lang with a canonical seed: initial() is the crate-owned freeze of its data --

    #[derive(Debug, Clone, Copy)]
    struct SeededLang;
    impl Lang for SeededLang {
        type GroupTypeId = u32;
        type CallableTypeId = u32;
        type StateExt = ();
        type Event = ();
        type SessionExt = ();
        type SourceOrigin = Option<String>;
        type NodeExts = ();

        fn initial_state_data() -> StateData<Self> {
            StateData { rules: base_rules(), libraries: LibraryStack::new(), ext: () }
        }
    }

    #[test]
    fn initial_freezes_the_langs_seed_data() {
        let state: ParsingState<SeededLang> = ParsingState::initial();
        assert!(state.rules().enable_groups);
        // The caches are built from the seed data at freeze:
        assert!(state.prefix_table().match_at("{x").is_some());
        // Customizing the starting point goes through derived() — the finalize choke
        // point — and an empty delta reproduces the seed (the coherence contract's
        // mechanical check, trivial here since SeededLang has no normalizer):
        let derived = state.derived(&ParsingStateDelta::new());
        assert_eq!(derived.rules(), state.rules());
    }

    #[test]
    fn enable_flag_disables_and_reenables_without_carrying_the_data() {
        // The restore problem the enable_* gates exist for (DESIGN_RATIONALE §3.2): the
        // re-enabling delta names no CommandRules — the data survived the disabled scope.
        let state: ParsingState<PlainLang> =
            ParsingState::new(StateData { rules: base_rules(), libraries: LibraryStack::new(), ext: () });

        let disabled = state.derived(&ParsingStateDelta::new().rules(TokenRulesOverrides {
            enable_commands: Some(false),
            ..TokenRulesOverrides::default()
        }));
        assert!(!disabled.rules().enable_commands);
        assert_eq!(disabled.rules().commands, state.rules().commands);

        let reenabled = disabled.derived(&ParsingStateDelta::new().rules(TokenRulesOverrides {
            enable_commands: Some(true),
            ..TokenRulesOverrides::default()
        }));
        assert!(reenabled.rules().enable_commands);
        assert_eq!(reenabled.rules().commands, state.rules().commands);
    }

    #[test]
    fn enable_groups_flag_rebakes_the_prefix_table() {
        // The gate is baked into the per-state table at freeze time; toggling it through
        // deltas empties and rebuilds the table with the (untouched) group rules.
        let state: ParsingState<PlainLang> =
            ParsingState::new(StateData { rules: base_rules(), libraries: LibraryStack::new(), ext: () });
        assert!(state.prefix_table().match_at("{x").is_some());

        let disabled = state.derived(&ParsingStateDelta::new().rules(TokenRulesOverrides {
            enable_groups: Some(false),
            ..TokenRulesOverrides::default()
        }));
        assert!(disabled.prefix_table().match_at("{x").is_none());
        assert_eq!(disabled.rules().groups, state.rules().groups);

        let reenabled = disabled.derived(&ParsingStateDelta::new().rules(TokenRulesOverrides {
            enable_groups: Some(true),
            ..TokenRulesOverrides::default()
        }));
        assert!(reenabled.prefix_table().match_at("{x").is_some());
    }

    // --- a lang exercising events + the finalize customizer ---------------------------
    //
    // The FLM-style example from ARCHITECTURE.md §state: "in math mode the escape char
    // is '#'". The math-open parser would only emit `EnterMath`; no delta writer knows
    // the escape-char rule — finalize centralizes it (pure-normalization idiom: dependent
    // settings recomputed from ext at every transition).

    #[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
    struct MathState {
        in_math: bool,
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum MathEvent {
        EnterMath,
        LeaveMath,
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    struct MathLang;
    impl Lang for MathLang {
        type GroupTypeId = u32;
        type CallableTypeId = u32;
        type StateExt = MathState;
        type Event = MathEvent;
        type SessionExt = ();
        type SourceOrigin = Option<String>;
        type NodeExts = ();

        fn finalize_transition(
            new: &mut StateData<Self>,
            _prev: &ParsingState<Self>,
            events: &[MathEvent],
        ) {
            for event in events {
                match event {
                    MathEvent::EnterMath => new.ext.in_math = true,
                    MathEvent::LeaveMath => new.ext.in_math = false,
                }
            }
            // Pure normalization: the escape char is a function of the mode. Rules are
            // Arc-shared; rewriting one is clone-on-write.
            for rule in &mut new.rules.commands {
                Arc::make_mut(rule).escape_char = if new.ext.in_math { '#' } else { '\\' };
            }
        }
    }

    #[test]
    fn finalize_centralizes_cross_cutting_rules() {
        let state: ParsingState<MathLang> =
            ParsingState::new(StateData { rules: base_rules(), libraries: LibraryStack::new(), ext: MathState::default() });

        let in_math = state.derived(&ParsingStateDelta::new().event(MathEvent::EnterMath));
        assert!(in_math.ext().in_math);
        assert_eq!(in_math.rules().commands[0].escape_char, '#');

        let out_again = in_math.derived(&ParsingStateDelta::new().event(MathEvent::LeaveMath));
        assert!(!out_again.ext().in_math);
        assert_eq!(out_again.rules().commands[0].escape_char, '\\');

        // The intermediate states are untouched (functional contract).
        assert_eq!(state.rules().commands[0].escape_char, '\\');
        assert_eq!(in_math.rules().commands[0].escape_char, '#');
    }

    #[test]
    fn normalization_clobbers_in_scope_overrides_by_design() {
        // The pure-normalization idiom recomputes dependent settings at *every*
        // transition — an explicit escape-char override is clobbered. That trade-off is
        // the customizer author's documented choice (ARCHITECTURE.md §state).
        let state: ParsingState<MathLang> =
            ParsingState::new(StateData { rules: base_rules(), libraries: LibraryStack::new(), ext: MathState::default() });

        let mut custom = base_rules::<MathLang>().commands;
        Arc::make_mut(&mut custom[0]).escape_char = '@';
        let delta = ParsingStateDelta::new().rules(TokenRulesOverrides {
            commands: Some(custom),
            ..TokenRulesOverrides::default()
        });
        let derived = state.derived(&delta);
        assert_eq!(derived.rules().commands[0].escape_char, '\\');
    }

    #[test]
    fn ext_replacement_via_delta() {
        let state: ParsingState<MathLang> =
            ParsingState::new(StateData { rules: base_rules(), libraries: LibraryStack::new(), ext: MathState::default() });
        let derived = state.derived(&ParsingStateDelta::new().ext(MathState { in_math: true }));
        assert!(derived.ext().in_math);
        // finalize also ran on the replaced ext:
        assert_eq!(derived.rules().commands[0].escape_char, '#');
    }
}
