//! [`ParsingState`] and its stored [`StateData`]; [`DeriveError`], the failure carrier
//! of the fallible transition choke point.

use alloc::sync::Arc;
use alloc::vec::Vec;
use core::fmt;

use crate::scopes::{ScopeOpError, ScopeStack};
use crate::token::{PrefixTable, TokenRules, TriggerChars};

use super::delta::ParsingStateDelta;
use super::lang::Lang;

/// The plain stored settings of a parsing state — the data that deltas override and
/// [`Lang::finalize_transition`] may rewrite. Fields are public *here* (the customizer
/// needs full access); the outer [`ParsingState`] exposes them read-only.
pub struct StateData<L: Lang> {
    /// Tokenization rules — plain stored data (defined in the token topic).
    pub rules: TokenRules<L>,
    /// The definitions visible in this state: the provider stack (Phase 7.3),
    /// modified mid-parse via [`scope_ops`](super::ParsingStateDelta::scope_ops)
    /// (`\newcommand`-style definitions, package loads).
    pub scopes: ScopeStack<L>,
    /// The parsing mode this state is in ([`Lang::ModeId`]) — first-class core data:
    /// deltas *initiate* mode changes ([`mode`](super::ParsingStateDelta::mode)
    /// override channel) and [`Lang::finalize_transition`] *interprets* them
    /// (DESIGN_RATIONALE.md [§dd-dr:parsing-state]).
    pub mode: L::ModeId,
    /// Language-specific state (e.g. feature-toggle flags; modal state lives in
    /// [`mode`](StateData::mode) instead).
    pub ext: L::StateExt,
}

/// An immutable parsing state: [`StateData`] behind a getter-only surface, plus derived
/// caches valid for this instance's lifetime.
///
/// The **only** way a non-initial state comes into existence is
/// [`derived()`](ParsingState::derived) — the transition choke point. States are cheaply
/// shareable; the engine wraps them in `Arc` and creates a new one only at transitions,
/// so nodes can record their parse-time state (ARCHITECTURE.md [§dd-arch:state]).
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
    /// Applies the delta's overrides to a copy of this state's data, strips temporary
    /// group rules when the delta ends their scope (below), runs
    /// [`Lang::finalize_transition`] exactly once, and freezes the result. Derived
    /// caches are rebuilt — except the [`PrefixTable`], which is reused from `self`
    /// (an `Arc` clone) when its inputs are unchanged: same `enable_groups`, same
    /// `groups` and `temporary_groups` rules by elementwise `Arc` identity. The
    /// dominant transition — a group interior overriding only `expecting_group_close`,
    /// which is deliberately not a table input — takes the reuse path. Functional
    /// contract: `self` is never observably mutated.
    ///
    /// # Fallibility (Phase 7.3, DESIGN_RATIONALE.md [§dd-dr:specs])
    ///
    /// A delta's [`scope_ops`](ParsingStateDelta::scope_ops) can fail (op targets an
    /// absent provider name; a definition op routed to an immutable provider). A delta
    /// without scope ops **cannot fail**. On failure, every failing op is skipped —
    /// the rest of the delta still applies — and the result is returned as a
    /// [`DeriveError`]: the mechanical failure records plus the fully derived
    /// **recovered state** (finalized and frozen like any other), so a tolerant caller
    /// can diagnose and continue while a strict caller aborts. Classification is the
    /// caller's: the in-parse seam routes failures through the recover funnel
    /// ([`ScopeOpFailed`](crate::constructs::ScopeOpFailed)); an embedder deriving out
    /// of parse treats an `Err` as its own input error.
    ///
    /// # Temporary group rules (July 2026, DESIGN_RATIONALE.md [§dd-dr:parsers-engine])
    ///
    /// [`TokenRules::temporary_groups`] is scoped in state data, and this choke point
    /// enforces the scope — every group descent passes through here (installing the
    /// entered rule as `expecting_group_close`), including hand-built deltas that never
    /// touch the session helpers. A delta that overrides `expecting_group_close` ends
    /// the temporaries' scope unless the installed close **is** one of the base's
    /// temporary rules (by `Arc` identity — the same-delimiter descent that keeps
    /// nested minted brackets balancing); installing any other rule, or clearing the
    /// expectation, yields a derived state with `temporary_groups` emptied, and
    /// interior inheritance then keeps it empty for the whole subtree (brace protection
    /// at any depth). A delta that explicitly overrides `temporary_groups` itself is
    /// exempt — the delta author spoke. The rule is a pure function of `(base, delta)`,
    /// so identity-keyed derivation memos stay sound.
    #[allow(clippy::result_large_err)] // large `Err` by design — see `DeriveError`
    pub fn derived(&self, delta: &ParsingStateDelta<L>) -> Result<ParsingState<L>, DeriveError<L>> {
        let mut data = self.data.clone();
        let failures = delta.apply_overrides(&mut data);
        let ends_temporary_scope = match &delta.rules.expecting_group_close {
            None => false,
            Some(installed) => !matches!(
                installed,
                Some(rule) if self
                    .data
                    .rules
                    .temporary_groups
                    .iter()
                    .any(|temporary| Arc::ptr_eq(temporary, rule))
            ),
        };
        if ends_temporary_scope && delta.rules.temporary_groups.is_none() {
            data.rules.temporary_groups.clear();
        }
        L::finalize_transition(&mut data, self, &delta.events);
        // Checked *after* finalize_transition: the customizer may rewrite `groups` too.
        let table_inputs_unchanged = data.rules.enable_groups == self.data.rules.enable_groups
            && data.rules.groups.len() == self.data.rules.groups.len()
            && data
                .rules
                .groups
                .iter()
                .zip(&self.data.rules.groups)
                .all(|(new, old)| Arc::ptr_eq(new, old))
            && data.rules.temporary_groups.len() == self.data.rules.temporary_groups.len()
            && data
                .rules
                .temporary_groups
                .iter()
                .zip(&self.data.rules.temporary_groups)
                .all(|(new, old)| Arc::ptr_eq(new, old));
        let state = if table_inputs_unchanged {
            ParsingState::freeze_with_table(data, Arc::clone(&self.prefix_table))
        } else {
            ParsingState::freeze(data)
        };
        if failures.is_empty() {
            Ok(state)
        } else {
            Err(DeriveError { failures, recovered: state, delta: delta.clone() })
        }
    }

    /// The tokenization rules in effect.
    pub fn rules(&self) -> &TokenRules<L> {
        &self.data.rules
    }

    /// The definitions visible in this state: the provider stack.
    pub fn scopes(&self) -> &ScopeStack<L> {
        &self.data.scopes
    }

    /// The parsing mode this state is in ([`Lang::ModeId`]; by value — modes are
    /// `Copy`).
    pub fn mode(&self) -> L::ModeId {
        self.data.mode
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
//
// `ParsingState` itself deliberately has **no** `Clone` (July 2026, Action 06): states
// are identity-bearing (`Arc` pointer identity keys the engine's memoization and links
// nodes to the state that parsed them), so duplicating one would fork that identity —
// and a `Clone` impl would make `Arc::make_mut` on a "frozen" state expressible. The
// only constructors are `initial()` and `derived()`.

impl<L: Lang> Clone for StateData<L> {
    fn clone(&self) -> Self {
        StateData {
            rules: self.rules.clone(),
            scopes: self.scopes.clone(),
            mode: self.mode,
            ext: self.ext.clone(),
        }
    }
}

impl<L: Lang> fmt::Debug for StateData<L> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("StateData")
            .field("rules", &self.rules)
            .field("scopes", &self.scopes)
            .field("mode", &self.mode)
            .field("ext", &self.ext)
            .finish()
    }
}

impl<L: Lang> fmt::Debug for ParsingState<L> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // The caches are derived data; `dbg!(state)` shows the settings that determine
        // behavior (the Option C debuggability promise).
        f.debug_struct("ParsingState")
            .field("rules", &self.data.rules)
            .field("scopes", &self.data.scopes)
            .field("mode", &self.data.mode)
            .field("ext", &self.data.ext)
            .finish_non_exhaustive()
    }
}

/// A [`derived()`](ParsingState::derived) transition whose delta carried failing
/// [`scope ops`](ParsingStateDelta::scope_ops) (Phase 7.3, DESIGN_RATIONALE.md [§dd-dr:specs]).
///
/// Mechanical, deliberately unclassified — whether a failure is an extension bug or an
/// embedder input error is the *caller's* context. The error carries everything a
/// tolerant caller needs to continue (the `String::from_utf8` pattern — recovery
/// material rides in the error):
///
/// - [`failures`](DeriveError::failures): one record per failing op, in delta order;
/// - [`recovered`](DeriveError::recovered): the fully derived state with exactly the
///   failing ops skipped — finalized and frozen like every state, ready to continue
///   under;
/// - [`delta`](DeriveError::delta): the delta as applied, so a recovering seam can
///   still feed
///   [`ParseDriver::observe_transition`](crate::engine::ParseDriver::observe_transition)
///   the true transition (needed because the group-interior seam derives with a
///   *merged* delta its caller never sees).
///
/// Not `Clone`: states are identity-bearing (deliberately non-`Clone`), and the
/// recovered state is a state.
///
/// The `Err` variant is large *by design* — it owns a full state plus the applied
/// delta, because the recovery payload is the point of the type. The functions
/// returning it `#[allow(clippy::result_large_err)]` rather than box: `Box`-free
/// signatures were judged worth the bigger `Result` return slot
/// (DESIGN_RATIONALE.md [§dd-dr:specs]).
pub struct DeriveError<L: Lang> {
    /// One record per failing op, in delta order (never empty).
    pub failures: Vec<ScopeOpError>,
    /// The derived state with the failing ops skipped (everything else applied).
    pub recovered: ParsingState<L>,
    /// The delta the derivation applied (cloned into the error).
    pub delta: ParsingStateDelta<L>,
}

impl<L: Lang> fmt::Display for DeriveError<L> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "scope op(s) failed while deriving a parsing state: ")?;
        for (i, failure) in self.failures.iter().enumerate() {
            if i > 0 {
                write!(f, "; ")?;
            }
            write!(f, "{failure}")?;
        }
        Ok(())
    }
}

impl<L: Lang> fmt::Debug for DeriveError<L> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DeriveError")
            .field("failures", &self.failures)
            .field("recovered", &self.recovered)
            .field("delta", &self.delta)
            .finish()
    }
}

impl<L: Lang> core::error::Error for DeriveError<L> {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scopes::ScopeStack;
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
            temporary_groups: Vec::new(),
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
            ParsingState::new(StateData { rules: base_rules(), scopes: ScopeStack::new(), mode: (), ext: () });

        let delta = ParsingStateDelta::new().rules(TokenRulesOverrides {
            enable_multi_newline_paragraphs: Some(false),
            forbidden_chars: Some("$".into()),
            ..TokenRulesOverrides::default()
        });
        let derived = state.derived(&delta).unwrap();

        assert!(!derived.rules().enable_multi_newline_paragraphs);
        assert_eq!(&*derived.rules().forbidden_chars, "$");
        // Unchanged fields kept; the original state is untouched.
        assert_eq!(derived.rules().commands, state.rules().commands);
        assert!(state.rules().enable_multi_newline_paragraphs);
    }

    #[test]
    fn derived_rebuilds_prefix_table() {
        let state: ParsingState<PlainLang> =
            ParsingState::new(StateData { rules: base_rules(), scopes: ScopeStack::new(), mode: (), ext: () });
        assert!(state.prefix_table().match_at("[x").is_none());

        let delta = ParsingStateDelta::new().rules(TokenRulesOverrides {
            groups: Some(vec![Arc::new(GroupRule {
                group_type: 1,
                open: "[".into(),
                close: "]".into(),
            })]),
            ..TokenRulesOverrides::default()
        });
        let derived = state.derived(&delta).unwrap();
        assert!(derived.prefix_table().match_at("[x").is_some());
        assert!(derived.prefix_table().match_at("{x").is_none()); // whole-value override
    }

    #[test]
    fn derived_reuses_prefix_table_when_inputs_unchanged() {
        let state: ParsingState<PlainLang> =
            ParsingState::new(StateData { rules: base_rules(), scopes: ScopeStack::new(), mode: (), ext: () });

        // A transition that touches neither enable_groups nor groups (by Arc identity)
        // shares the parent's table instance…
        let same = state.derived(&ParsingStateDelta::new().rules(TokenRulesOverrides {
            enable_comments: Some(false),
            ..TokenRulesOverrides::default()
        })).unwrap();
        assert!(core::ptr::eq(state.prefix_table(), same.prefix_table()));

        // …and so does the dominant group-interior transition (expecting_group_close is
        // deliberately not a table input)…
        let interior = state.derived(&ParsingStateDelta::new().rules(TokenRulesOverrides {
            expecting_group_close: Some(Some(Arc::clone(&state.rules().groups[0]))),
            ..TokenRulesOverrides::default()
        })).unwrap();
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
        })).unwrap();
        assert!(!core::ptr::eq(state.prefix_table(), rebuilt.prefix_table()));
        assert_eq!(state.prefix_table(), rebuilt.prefix_table()); // same contents
    }

    #[test]
    fn derived_scopes_temporary_group_rules() {
        let state: ParsingState<PlainLang> =
            ParsingState::new(StateData { rules: base_rules(), scopes: ScopeStack::new(), mode: (), ext: () });
        let temporary = Arc::new(GroupRule { group_type: 9, open: "[".into(), close: "]".into() });
        let with_temp = state.derived(&ParsingStateDelta::new().rules(TokenRulesOverrides {
            temporary_groups: Some(vec![Arc::clone(&temporary)]),
            ..TokenRulesOverrides::default()
        })).unwrap();
        assert!(with_temp.prefix_table().match_at("[x").is_some());

        // A delta that says nothing about the expected close carries them over…
        let untouched = with_temp.derived(&ParsingStateDelta::new().rules(TokenRulesOverrides {
            enable_comments: Some(false),
            ..TokenRulesOverrides::default()
        })).unwrap();
        assert_eq!(untouched.rules().temporary_groups.len(), 1);

        // …and so does entering the temporary rule's own group (nested delimiters
        // keep balancing).
        let same_rule = with_temp.derived(&ParsingStateDelta::new().rules(TokenRulesOverrides {
            expecting_group_close: Some(Some(Arc::clone(&temporary))),
            ..TokenRulesOverrides::default()
        })).unwrap();
        assert_eq!(same_rule.rules().temporary_groups.len(), 1);

        // Entering any other group ends the scope: the derived interior has no
        // temporaries (and inheritance keeps it that way for the whole subtree).
        let brace = Arc::clone(&state.rules().groups[0]);
        let stripped = with_temp.derived(&ParsingStateDelta::new().rules(TokenRulesOverrides {
            expecting_group_close: Some(Some(Arc::clone(&brace))),
            ..TokenRulesOverrides::default()
        })).unwrap();
        assert!(stripped.rules().temporary_groups.is_empty());
        assert!(stripped.prefix_table().match_at("[x").is_none());

        // Clearing the expectation strips too.
        let cleared = with_temp.derived(&ParsingStateDelta::new().rules(TokenRulesOverrides {
            expecting_group_close: Some(None),
            ..TokenRulesOverrides::default()
        })).unwrap();
        assert!(cleared.rules().temporary_groups.is_empty());

        // An explicit temporaries override wins over stripping: the delta author spoke.
        let other = Arc::new(GroupRule { group_type: 8, open: "<".into(), close: ">".into() });
        let overridden = with_temp.derived(&ParsingStateDelta::new().rules(TokenRulesOverrides {
            expecting_group_close: Some(Some(brace)),
            temporary_groups: Some(vec![Arc::clone(&other)]),
            ..TokenRulesOverrides::default()
        })).unwrap();
        assert_eq!(overridden.rules().temporary_groups, vec![other]);
    }

    #[test]
    fn temporary_groups_are_prefix_table_inputs() {
        let state: ParsingState<PlainLang> =
            ParsingState::new(StateData { rules: base_rules(), scopes: ScopeStack::new(), mode: (), ext: () });
        let temporary = Arc::new(GroupRule { group_type: 9, open: "[".into(), close: "]".into() });
        let with_temp = state.derived(&ParsingStateDelta::new().rules(TokenRulesOverrides {
            temporary_groups: Some(vec![Arc::clone(&temporary)]),
            ..TokenRulesOverrides::default()
        })).unwrap();
        // Installing the temporaries rebuilt the table…
        assert!(!core::ptr::eq(state.prefix_table(), with_temp.prefix_table()));

        // …the keep-path descent (into the temporary rule itself) reuses it…
        let same_rule = with_temp.derived(&ParsingStateDelta::new().rules(TokenRulesOverrides {
            expecting_group_close: Some(Some(Arc::clone(&temporary))),
            ..TokenRulesOverrides::default()
        })).unwrap();
        assert!(core::ptr::eq(with_temp.prefix_table(), same_rule.prefix_table()));

        // …and the strip-path descent rebuilds: a stale reuse here would keep
        // tokenizing the stripped delimiters.
        let stripped = with_temp.derived(&ParsingStateDelta::new().rules(TokenRulesOverrides {
            expecting_group_close: Some(Some(Arc::clone(&state.rules().groups[0]))),
            ..TokenRulesOverrides::default()
        })).unwrap();
        assert!(!core::ptr::eq(with_temp.prefix_table(), stripped.prefix_table()));
    }

    #[test]
    fn empty_delta_is_a_clean_copy() {
        let state: ParsingState<PlainLang> =
            ParsingState::new(StateData { rules: base_rules(), scopes: ScopeStack::new(), mode: (), ext: () });
        let derived = state.derived(&ParsingStateDelta::new()).unwrap();
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
        assert!(state.scopes().is_empty());
        assert!(state.prefix_table().match_at("{x").is_none());
    }

    // --- a lang with a canonical seed: initial() is the crate-owned freeze of its data --

    #[derive(Debug, Clone, Copy)]
    struct SeededLang;
    impl Lang for SeededLang {
        type GroupTypeId = u32;
        type CallableTypeId = u32;
        type ModeId = ();
        type StateExt = ();
        type Event = ();
        type SessionExt = ();
        type SourceOrigin = Option<String>;
        type NodeExts = ();
        type Driver = crate::engine::StdParseDriver;

        fn initial_state_data() -> StateData<Self> {
            StateData { rules: base_rules(), scopes: ScopeStack::new(), mode: (), ext: () }
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
        let derived = state.derived(&ParsingStateDelta::new()).unwrap();
        assert_eq!(derived.rules(), state.rules());
    }

    #[test]
    fn enable_flag_disables_and_reenables_without_carrying_the_data() {
        // The restore problem the enable_* gates exist for (DESIGN_RATIONALE [§dd-dr:tokens]): the
        // re-enabling delta names no CommandRules — the data survived the disabled scope.
        let state: ParsingState<PlainLang> =
            ParsingState::new(StateData { rules: base_rules(), scopes: ScopeStack::new(), mode: (), ext: () });

        let disabled = state.derived(&ParsingStateDelta::new().rules(TokenRulesOverrides {
            enable_commands: Some(false),
            ..TokenRulesOverrides::default()
        })).unwrap();
        assert!(!disabled.rules().enable_commands);
        assert_eq!(disabled.rules().commands, state.rules().commands);

        let reenabled = disabled.derived(&ParsingStateDelta::new().rules(TokenRulesOverrides {
            enable_commands: Some(true),
            ..TokenRulesOverrides::default()
        })).unwrap();
        assert!(reenabled.rules().enable_commands);
        assert_eq!(reenabled.rules().commands, state.rules().commands);
    }

    #[test]
    fn enable_groups_flag_rebakes_the_prefix_table() {
        // The gate is baked into the per-state table at freeze time; toggling it through
        // deltas empties and rebuilds the table with the (untouched) group rules.
        let state: ParsingState<PlainLang> =
            ParsingState::new(StateData { rules: base_rules(), scopes: ScopeStack::new(), mode: (), ext: () });
        assert!(state.prefix_table().match_at("{x").is_some());

        let disabled = state.derived(&ParsingStateDelta::new().rules(TokenRulesOverrides {
            enable_groups: Some(false),
            ..TokenRulesOverrides::default()
        })).unwrap();
        assert!(disabled.prefix_table().match_at("{x").is_none());
        assert_eq!(disabled.rules().groups, state.rules().groups);

        let reenabled = disabled.derived(&ParsingStateDelta::new().rules(TokenRulesOverrides {
            enable_groups: Some(true),
            ..TokenRulesOverrides::default()
        })).unwrap();
        assert!(reenabled.prefix_table().match_at("{x").is_some());
    }

    // --- a lang exercising events + the finalize customizer ---------------------------
    //
    // The FLM-style example from ARCHITECTURE.md [§dd-arch:state]: "in math mode the escape char
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
        type ModeId = ();
        type StateExt = MathState;
        type Event = MathEvent;
        type SessionExt = ();
        type SourceOrigin = Option<String>;
        type NodeExts = ();
        type Driver = crate::engine::StdParseDriver;

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
            ParsingState::new(StateData { rules: base_rules(), scopes: ScopeStack::new(), mode: (), ext: MathState::default() });

        let in_math = state.derived(&ParsingStateDelta::new().event(MathEvent::EnterMath)).unwrap();
        assert!(in_math.ext().in_math);
        assert_eq!(in_math.rules().commands[0].escape_char, '#');

        let out_again = in_math.derived(&ParsingStateDelta::new().event(MathEvent::LeaveMath)).unwrap();
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
        // the customizer author's documented choice (ARCHITECTURE.md [§dd-arch:state]).
        let state: ParsingState<MathLang> =
            ParsingState::new(StateData { rules: base_rules(), scopes: ScopeStack::new(), mode: (), ext: MathState::default() });

        let mut custom = base_rules::<MathLang>().commands;
        Arc::make_mut(&mut custom[0]).escape_char = '@';
        let delta = ParsingStateDelta::new().rules(TokenRulesOverrides {
            commands: Some(custom),
            ..TokenRulesOverrides::default()
        });
        let derived = state.derived(&delta).unwrap();
        assert_eq!(derived.rules().commands[0].escape_char, '\\');
    }

    #[test]
    fn ext_replacement_via_delta() {
        let state: ParsingState<MathLang> =
            ParsingState::new(StateData { rules: base_rules(), scopes: ScopeStack::new(), mode: (), ext: MathState::default() });
        let derived = state.derived(&ParsingStateDelta::new().ext(MathState { in_math: true })).unwrap();
        assert!(derived.ext().in_math);
        // finalize also ran on the replaced ext:
        assert_eq!(derived.rules().commands[0].escape_char, '#');
    }

    // --- a lang with a first-class parsing mode (Phase 7.1, DESIGN_RATIONALE [§dd-dr:parsing-state]) -----
    //
    // Mode-shaped transitions need no `L::Event`: the delta's mode override *is* the
    // signal. `finalize_transition` interprets it — level normalization recomputed from
    // the (already overridden) `new.mode`, with `prev.mode()` available for
    // edge-sensitive reactions.

    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
    enum Mode {
        #[default]
        Text,
        Math,
    }

    /// What finalize saw of the transition edge (proves prev/new mode visibility).
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
    struct SeenEdge {
        entered_math: bool,
    }

    #[derive(Debug, Clone, Copy)]
    struct ModedLang;
    impl Lang for ModedLang {
        type GroupTypeId = u32;
        type CallableTypeId = u32;
        type ModeId = Mode;
        type StateExt = SeenEdge;
        type Event = ();
        type SessionExt = ();
        type SourceOrigin = Option<String>;
        type NodeExts = ();
        type Driver = crate::engine::StdParseDriver;

        fn initial_state_data() -> StateData<Self> {
            // Coherent seed: text mode with comments enabled — exactly what
            // finalize_transition would normalize to (the hook's coherence contract).
            StateData {
                rules: base_rules(),
                scopes: ScopeStack::new(),
                mode: Mode::Text,
                ext: SeenEdge::default(),
            }
        }

        fn finalize_transition(
            new: &mut StateData<Self>,
            prev: &ParsingState<Self>,
            _events: &[()],
        ) {
            // Level normalization: comments are a text-mode feature in this toy
            // language — a pure function of the incoming mode.
            new.rules.enable_comments = new.mode == Mode::Text;
            // Edge visibility: the hook compares prev.mode() with the applied override.
            new.ext =
                SeenEdge { entered_math: prev.mode() == Mode::Text && new.mode == Mode::Math };
        }
    }

    #[test]
    fn default_seed_mode_is_the_mode_ids_default() {
        // A lang with an enum mode that keeps the *default* initial_state_data: the
        // seed's mode is `ModeId::default()` (the `Default` bound exists for this).
        #[derive(Debug, Clone, Copy)]
        struct DefaultSeedLang;
        impl Lang for DefaultSeedLang {
            type GroupTypeId = u32;
            type CallableTypeId = u32;
            type ModeId = Mode;
            type StateExt = ();
            type Event = ();
            type SessionExt = ();
            type SourceOrigin = Option<String>;
            type NodeExts = ();
            type Driver = crate::engine::StdParseDriver;
        }
        let state: ParsingState<DefaultSeedLang> = ParsingState::initial();
        assert_eq!(state.mode(), Mode::Text);
        // SimpleLang languages are modeless: `ModeId = ()`.
        let plain: ParsingState<PlainLang> = ParsingState::initial();
        assert_eq!(plain.mode(), ());
    }

    #[test]
    fn moded_seed_is_coherent_under_the_empty_delta() {
        // The mechanical pin of the coherence contract, mode included.
        let seed: ParsingState<ModedLang> = ParsingState::initial();
        assert_eq!(seed.mode(), Mode::Text);
        assert!(seed.rules().enable_comments);
        let derived = seed.derived(&ParsingStateDelta::new()).unwrap();
        assert_eq!(derived.rules(), seed.rules());
        assert_eq!(derived.mode(), seed.mode());
        assert_eq!(derived.ext(), seed.ext());
    }

    #[test]
    fn mode_overrides_via_delta_and_is_inherited_otherwise() {
        let state: ParsingState<ModedLang> = ParsingState::initial();

        let math = state.derived(&ParsingStateDelta::new().mode(Mode::Math)).unwrap();
        assert_eq!(math.mode(), Mode::Math);
        assert_eq!(state.mode(), Mode::Text); // functional contract: base untouched

        // A delta that says nothing about the mode carries it over…
        let inherited = math.derived(&ParsingStateDelta::new().rules(TokenRulesOverrides {
            enable_whitespace: Some(false),
            ..TokenRulesOverrides::default()
        })).unwrap();
        assert_eq!(inherited.mode(), Mode::Math);

        // …and a mode override travels with rules overrides in one delta — one
        // transition, one finalize run.
        let combo =
            state.derived(&ParsingStateDelta::new().mode(Mode::Math).rules(TokenRulesOverrides {
                enable_whitespace: Some(false),
                ..TokenRulesOverrides::default()
            })).unwrap();
        assert_eq!(combo.mode(), Mode::Math);
        assert!(!combo.rules().enable_whitespace);
        assert!(!combo.rules().enable_comments); // finalize interpreted the mode too
    }

    #[test]
    fn finalize_interprets_mode_changes_seeing_prev_and_new() {
        let state: ParsingState<ModedLang> = ParsingState::initial();

        // Entering math: level normalization applies (comments off), and the hook saw
        // the Text→Math edge.
        let math = state.derived(&ParsingStateDelta::new().mode(Mode::Math)).unwrap();
        assert!(!math.rules().enable_comments);
        assert!(math.ext().entered_math);

        // A mode-silent transition inside math: still normalized (level, not edge)…
        let still_math = math.derived(&ParsingStateDelta::new().rules(TokenRulesOverrides {
            enable_whitespace: Some(false),
            ..TokenRulesOverrides::default()
        })).unwrap();
        assert!(!still_math.rules().enable_comments);
        // …and no Text→Math edge was seen (prev is already Math).
        assert!(!still_math.ext().entered_math);

        // Leaving math re-enables the text-mode feature (recomputed, not restored).
        let text_again = still_math.derived(&ParsingStateDelta::new().mode(Mode::Text)).unwrap();
        assert!(text_again.rules().enable_comments);
        assert!(!text_again.ext().entered_math);
    }

    // --- scope ops through the choke point (Phase 7.3): fallibility, CoW reversion -----

    #[test]
    fn derived_applies_scope_ops_and_reverts_structurally() {
        use crate::scopes::{CallableQuery, CallableSyntax, Package};
        use crate::spec::{CallableSpec, StdCallableSpec};

        let spec: Arc<dyn CallableSpec<PlainLang>> = Arc::new(StdCallableSpec::default());
        let mut package: Package<PlainLang> = Package::new("newcommands");
        package.insert(0u32, "mycmd", Arc::clone(&spec));

        let base: ParsingState<PlainLang> =
            ParsingState::new(StateData { rules: base_rules(), scopes: ScopeStack::new(), mode: (), ext: () });
        let derived =
            base.derived(&ParsingStateDelta::new().push_provider(Arc::new(package))).unwrap();

        let query = CallableQuery::new(0u32, "mycmd", CallableSyntax::Command { escape_char: '\\' });
        // The derived state resolves the new name; the base never does — "popping" is
        // just the caller keeping the previous state (structural reversion).
        assert!(base.scopes().retrieve_spec(&query, &base).unwrap().is_none());
        let resolved = derived.scopes().retrieve_spec(&query, &derived).unwrap().unwrap();
        assert!(Arc::ptr_eq(&resolved, &spec));
        assert_eq!(base.scopes().len(), 0);
        assert_eq!(derived.scopes().len(), 1);
    }

    #[test]
    fn derived_define_into_an_outer_scope_stays_group_local_via_cow() {
        use crate::scopes::{CallableQuery, CallableSyntax, Scope, ScopeOp};
        use crate::spec::{CallableSpec, StdCallableSpec};

        // The lazy-scope semantics: no per-group scope pushes — a Define inside a
        // group routes to the *outer* "user" scope, and copy-on-write plus structural
        // reversion still keeps the definition group-local.
        let mut user: Scope<PlainLang> = Scope::new("user");
        user.insert(0u32, "outer", Arc::new(StdCallableSpec::default()));
        let mut scopes = ScopeStack::new();
        scopes.push(Arc::new(user));
        let base: ParsingState<PlainLang> =
            ParsingState::new(StateData { rules: base_rules(), scopes, mode: (), ext: () });

        let added: Arc<dyn CallableSpec<PlainLang>> = Arc::new(StdCallableSpec::default());
        let interior = base
            .derived(&ParsingStateDelta::new().scope_op(ScopeOp::Define {
                scope: "user".into(),
                callable_type: 0u32,
                name: "inner".into(),
                spec: Arc::clone(&added),
            }))
            .unwrap();

        let query = CallableQuery::new(0u32, "inner", CallableSyntax::Command { escape_char: '\\' });
        let resolved = interior.scopes().retrieve_spec(&query, &interior).unwrap().unwrap();
        assert!(Arc::ptr_eq(&resolved, &added));
        // The base still holds the pre-CoW provider: nothing leaked outward.
        assert!(base.scopes().retrieve_spec(&query, &base).unwrap().is_none());
        assert!(!Arc::ptr_eq(&base.scopes().providers()[0], &interior.scopes().providers()[0]));
    }

    #[test]
    fn derived_define_lazily_creates_the_named_scope() {
        use crate::scopes::{CallableQuery, CallableSyntax, ScopeOp};
        use crate::spec::StdCallableSpec;

        let base: ParsingState<PlainLang> =
            ParsingState::new(StateData { rules: base_rules(), scopes: ScopeStack::new(), mode: (), ext: () });
        let derived = base
            .derived(&ParsingStateDelta::new().scope_op(ScopeOp::Define {
                scope: "local".into(),
                callable_type: 0u32,
                name: "fresh".into(),
                spec: Arc::new(StdCallableSpec::default()),
            }))
            .unwrap();
        assert_eq!(derived.scopes().provider_names().collect::<Vec<_>>(), vec!["local"]);
        let query = CallableQuery::new(0u32, "fresh", CallableSyntax::Other);
        assert!(derived.scopes().retrieve_spec(&query, &derived).unwrap().is_some());
    }

    #[test]
    fn derived_collects_op_failures_and_carries_the_recovered_state() {
        use crate::scopes::{Package, ScopeOp, ScopeOpError};

        let base: ParsingState<PlainLang> =
            ParsingState::new(StateData { rules: base_rules(), scopes: ScopeStack::new(), mode: (), ext: () });

        // Three ops: a failing Unload sandwiched between valid ones — plus a rules
        // override riding along. The failing op is skipped; everything else applies.
        let delta = ParsingStateDelta::new()
            .push_provider(Arc::new(Package::new("a")))
            .scope_op(ScopeOp::Unload { name: "nope".into() })
            .push_provider(Arc::new(Package::new("b")))
            .rules(TokenRulesOverrides {
                enable_comments: Some(false),
                ..TokenRulesOverrides::default()
            });
        let error = base.derived(&delta).unwrap_err();

        assert_eq!(error.failures, vec![ScopeOpError::UnknownProvider { name: "nope".into() }]);
        assert!(error.to_string().contains("nope"));
        // The recovered state is the full derivation minus the failing op…
        assert_eq!(
            error.recovered.scopes().provider_names().collect::<Vec<_>>(),
            vec!["b", "a"]
        );
        assert!(!error.recovered.rules().enable_comments);
        // …the carried delta is the one that was applied…
        assert_eq!(error.delta.scope_ops.len(), 3);
        // …and the base is untouched (functional contract holds on the Err path too).
        assert_eq!(base.scopes().len(), 0);
        assert!(base.rules().enable_comments);
    }
}
