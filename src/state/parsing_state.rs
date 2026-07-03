//! [`ParsingState`] and its stored [`StateData`].

use core::fmt;

use crate::token::{PrefixTable, TokenRules, TriggerChars};

use super::delta::ParsingStateDelta;
use super::lang::Lang;

/// The plain stored settings of a parsing state — the data that deltas override and
/// [`Lang::finalize_transition`] may rewrite. Fields are public *here* (the customizer
/// needs full access); the outer [`ParsingState`] exposes them read-only.
///
/// `libraries` (the definitions visible in this state) arrives with Phase 4.
pub struct StateData<L: Lang> {
    /// Tokenization rules — plain stored data (defined in the token topic).
    pub rules: TokenRules,
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
/// states non-`Sync`), and both derivations are cheap relative to a transition. Revisit
/// only if profiling shows transition cost matters.
pub struct ParsingState<L: Lang> {
    data: StateData<L>,
    prefix_table: PrefixTable,
    trigger_chars: TriggerChars,
}

impl<L: Lang> ParsingState<L> {
    /// Create an initial parsing state. (Non-initial states only ever come from
    /// [`derived()`](ParsingState::derived); at the engine level, `Language<L>` seeds
    /// the initial state from its defaults, Phase 6.)
    pub fn new(data: StateData<L>) -> ParsingState<L> {
        ParsingState::freeze(data)
    }

    /// The sole constructor of non-initial states — the transition choke point.
    ///
    /// Applies the delta's overrides to a copy of this state's data, runs
    /// [`Lang::finalize_transition`] exactly once, and freezes the result (derived
    /// caches rebuilt). Functional contract: `self` is never observably mutated.
    pub fn derived(&self, delta: &ParsingStateDelta<L>) -> ParsingState<L> {
        let mut data = self.data.clone();
        delta.apply_overrides(&mut data);
        L::finalize_transition(&mut data, self, &delta.events);
        ParsingState::freeze(data)
    }

    /// The tokenization rules in effect.
    pub fn rules(&self) -> &TokenRules {
        &self.data.rules
    }

    /// The language-specific state extension.
    pub fn ext(&self) -> &L::StateExt {
        &self.data.ext
    }

    /// The delimiter-matching table derived from [`rules().group_types`](TokenRules).
    pub fn prefix_table(&self) -> &PrefixTable {
        &self.prefix_table
    }

    /// The specials trigger-character filter derived via
    /// [`Lang::specials_trigger_chars`].
    pub fn trigger_chars(&self) -> &TriggerChars {
        &self.trigger_chars
    }

    fn freeze(data: StateData<L>) -> ParsingState<L> {
        let prefix_table = PrefixTable::for_rules(&data.rules);
        let trigger_chars = L::specials_trigger_chars(&data);
        ParsingState { data, prefix_table, trigger_chars }
    }
}

// Manual impls: derives would demand `L: Clone`/`L: Debug` although only the associated
// types (already bounded in `Lang`) are stored.

impl<L: Lang> Clone for StateData<L> {
    fn clone(&self) -> Self {
        StateData { rules: self.rules.clone(), ext: self.ext.clone() }
    }
}

impl<L: Lang> fmt::Debug for StateData<L> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("StateData").field("rules", &self.rules).field("ext", &self.ext).finish()
    }
}

impl<L: Lang> Clone for ParsingState<L> {
    fn clone(&self) -> Self {
        ParsingState {
            data: self.data.clone(),
            prefix_table: self.prefix_table.clone(),
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
            .field("ext", &self.data.ext)
            .finish_non_exhaustive()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::TokenRulesOverrides;
    use crate::token::{CommandRule, GroupType, GroupTypeId, WhitespaceRules};
    use alloc::string::String;
    use alloc::vec;
    use alloc::vec::Vec;

    fn base_rules() -> TokenRules {
        TokenRules {
            whitespace: Some(WhitespaceRules { chars: " \t\n".into() }),
            double_newline_paragraphs: true,
            group_types: vec![GroupType {
                id: GroupTypeId::new(0),
                open: "{".into(),
                close: "}".into(),
            }],
            commands: vec![CommandRule {
                escape_char: '\\',
                name_chars: "abcdefghijklmnopqrstuvwxyz".into(),
            }],
            comments: Vec::new(),
            forbidden_chars: String::new(),
            expecting_group_close: None,
        }
    }

    // --- a minimal lang: no customization at all -------------------------------------

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    struct PlainLang;
    impl Lang for PlainLang {
        type StateExt = ();
        type Event = ();
        type SourceOrigin = Option<String>;
    }

    #[test]
    fn derived_applies_overrides_and_keeps_the_rest() {
        let state: ParsingState<PlainLang> =
            ParsingState::new(StateData { rules: base_rules(), ext: () });

        let delta = ParsingStateDelta::new().rules(TokenRulesOverrides {
            double_newline_paragraphs: Some(false),
            forbidden_chars: Some("$".into()),
            ..TokenRulesOverrides::default()
        });
        let derived = state.derived(&delta);

        assert!(!derived.rules().double_newline_paragraphs);
        assert_eq!(derived.rules().forbidden_chars, "$");
        // Unchanged fields kept; the original state is untouched.
        assert_eq!(derived.rules().commands, state.rules().commands);
        assert!(state.rules().double_newline_paragraphs);
    }

    #[test]
    fn derived_rebuilds_prefix_table() {
        let state: ParsingState<PlainLang> =
            ParsingState::new(StateData { rules: base_rules(), ext: () });
        assert!(state.prefix_table().match_at("[x").is_none());

        let delta = ParsingStateDelta::new().rules(TokenRulesOverrides {
            group_types: Some(vec![GroupType {
                id: GroupTypeId::new(1),
                open: "[".into(),
                close: "]".into(),
            }]),
            ..TokenRulesOverrides::default()
        });
        let derived = state.derived(&delta);
        assert!(derived.prefix_table().match_at("[x").is_some());
        assert!(derived.prefix_table().match_at("{x").is_none()); // whole-value override
    }

    #[test]
    fn empty_delta_is_a_clean_copy() {
        let state: ParsingState<PlainLang> =
            ParsingState::new(StateData { rules: base_rules(), ext: () });
        let derived = state.derived(&ParsingStateDelta::new());
        assert_eq!(derived.rules(), state.rules());
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
        type StateExt = MathState;
        type Event = MathEvent;
        type SourceOrigin = Option<String>;

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
            // Pure normalization: the escape char is a function of the mode.
            for rule in &mut new.rules.commands {
                rule.escape_char = if new.ext.in_math { '#' } else { '\\' };
            }
        }
    }

    #[test]
    fn finalize_centralizes_cross_cutting_rules() {
        let state: ParsingState<MathLang> =
            ParsingState::new(StateData { rules: base_rules(), ext: MathState::default() });

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
            ParsingState::new(StateData { rules: base_rules(), ext: MathState::default() });

        let mut custom = base_rules().commands;
        custom[0].escape_char = '@';
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
            ParsingState::new(StateData { rules: base_rules(), ext: MathState::default() });
        let derived = state.derived(&ParsingStateDelta::new().ext(MathState { in_math: true }));
        assert!(derived.ext().in_math);
        // finalize also ran on the replaced ext:
        assert_eq!(derived.rules().commands[0].escape_char, '#');
    }
}
