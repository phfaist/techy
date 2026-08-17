//! [`ParsingState`] and its stored [`StateData`]; [`DeriveError`], the failure carrier
//! of the fallible transition choke point.

use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::fmt;

use crate::scopes::{IntoSpecsProvider, ScopeOpError, ScopeStack};
use crate::token::{PrefixTable, TokenRules, TriggerChars};

use super::delta::ParsingStateDelta;
use super::features::{FeaturePresence, LangFeatures, LangHasScopes};
use super::lang::Lang;

/// The plain stored settings of a parsing state — the data that deltas override and
/// [`Lang::finalize_transition`] may rewrite. Fields are public *here* (the customizer
/// needs full access); the outer [`ParsingState`] exposes them read-only.
pub struct StateData<L: Lang> {
    /// Tokenization rules — plain stored data (defined in the token topic).
    pub rules: TokenRules<L>,
    /// The definitions visible in this state: the provider stack,
    /// modified mid-parse via [`scope_ops`](super::ParsingStateDelta::scope_ops)
    /// (`\newcommand`-style definitions, package loads).
    pub scopes: ScopeStack<L>,
    /// The parsing mode this state is in ([`Lang::ModeId`]) — first-class core data:
    /// deltas *initiate* mode changes ([`mode`](super::ParsingStateDelta::mode)
    /// override channel) and [`Lang::finalize_transition`] *interprets* them.
    pub mode: L::ModeId,
    /// Language-specific state (e.g. feature-toggle flags; modal state lives in
    /// [`mode`](StateData::mode) instead).
    pub ext: L::StateExt,
}

impl<L: Lang> StateData<L> {
    /// The all-empty state data: [`TokenRules::empty`] rules (every syntax gate off —
    /// character-level content), an empty [`ScopeStack`], the default mode and ext —
    /// the most neutral starting value. The default [`Lang::initial_state_data`]
    /// returns exactly this; a language filling in its canonical seed starts here and
    /// replaces what it defines.
    ///
    /// Deliberately a named constructor, not a `Default` impl: a struct-update
    /// `..Default::default()` would silently zero future fields, where the named
    /// constructor documents the all-empty intent.
    pub fn empty() -> StateData<L> {
        StateData {
            rules: TokenRules::empty(),
            scopes: ScopeStack::new(),
            mode: Default::default(),
            ext: Default::default(),
        }
    }
}

/// An immutable parsing state: [`StateData`] behind a getter-only surface, plus derived
/// caches valid for this instance's lifetime.
///
/// The **only** way a non-initial state comes into existence is
/// [`derived()`](ParsingState::derived) — the single point through which every state
/// transition passes. States are cheaply
/// shareable; the engine wraps them in `Arc` and creates a new one only at transitions,
/// so nodes can record their parse-time state.
///
/// # Derived caches
///
/// The delimiter [`PrefixTable`] and the specials [`TriggerChars`] filter are computed
/// eagerly when the state is frozen (constructor / end of `derived()`), not lazily on
/// first use: the crate is `no_std` (`core` has no `OnceLock`, and `OnceCell` would make
/// states non-`Sync`). Eager rebuilds are a real fraction of a transition's cost, so
/// [`derived()`](ParsingState::derived) reuses the parent's `PrefixTable` (held behind
/// `Arc`) whenever its inputs — the groups block's gate and rule lists, by `Arc`
/// identity — are unchanged. No analogous generic reuse rule exists for
/// `TriggerChars`: its inputs include `L::StateExt`, which carries no `Eq` bound (see
/// [`Lang::specials_trigger_chars`]).
///
/// Each cache is stored through the presence declaration of the feature it derives
/// from ([`Lang::Features`]): the prefix table with groups, the trigger filter with
/// specials. For a language that declares the feature absent, the cache is the
/// zero-sized store and its accessor answers `None`.
pub struct ParsingState<L: Lang> {
    data: StateData<L>,
    prefix_table:
        <<L::Features as LangFeatures>::Groups as FeaturePresence>::Store<Arc<PrefixTable<L>>>,
    trigger_chars:
        <<L::Features as LangFeatures>::Specials as FeaturePresence>::Store<TriggerChars>,
}

impl<L: Lang> ParsingState<L> {
    /// The *Lang's* seed state: [`Lang::initial_state_data`] frozen — the one public
    /// path from data to state, so every state a parse sees is either this seed or a
    /// [`derived()`](ParsingState::derived) descendant that passed through
    /// [`Lang::finalize_transition`]. Callers customize the starting point by deriving
    /// from the seed with a delta (`ParsingState::lang_initial()?.derived(&delta)?`) —
    /// or, for the everyday "seed plus these packages" case,
    /// [`lang_initial_with_packages`](ParsingState::lang_initial_with_packages). The
    /// seed itself does *not* run `finalize_transition` (it has no predecessor); its
    /// coherence is the language author's contract (the hook's docs).
    ///
    /// # Fallibility
    ///
    /// `Err` is [`Lang::initial_state_data`]'s own failure passed through
    /// ([`FinalizeError`] — a seed built from configuration or external definition
    /// data can be invalid or unavailable; see the hook's docs): the failure
    /// surfaces here, at seeding time, so a broken seed is never parsed with. For
    /// a language whose seed data cannot fail, unwrapping with
    /// `.expect("seed state")` states exactly that.
    pub fn lang_initial() -> Result<ParsingState<L>, FinalizeError> {
        Ok(ParsingState::freeze(L::initial_state_data()?))
    }

    /// The *Lang's* seed state with `packages` pushed onto its scope stack (in
    /// iteration order — the last pushed is innermost and shadows the ones below):
    /// the everyday "define a package, add it to the language" construction.
    /// Requires a language whose
    /// features declare the scope stack present ([`LangHasScopes`]) — pushing
    /// providers is scope mutation; a language without the feature seeds via
    /// [`lang_initial`](ParsingState::lang_initial). Packages pass by value through
    /// the sealed [`IntoSpecsProvider`] conversion (pre-shared `Arc`s pass through):
    ///
    /// ```
    /// # use techy::core::{Language, ParsingState, StdParseDriver};
    /// # use techy::core::specs::Package;
    /// # use techy::error::Recovery;
    /// # #[derive(Debug, Clone, Copy)]
    /// # struct MyLang;
    /// # impl techy::core::TrivialLang for MyLang {}
    /// # let my_package: Package<MyLang> = Package::new("mydefs");
    /// let language = Language::new(
    ///     StdParseDriver::new(Recovery::Strict, ()),
    ///     ParsingState::lang_initial_with_packages([my_package]).expect("seed state"),
    /// );
    /// ```
    ///
    /// # Fallibility
    ///
    /// `Err` is [`Lang::initial_state_data`]'s own failure passed through, exactly
    /// as on [`lang_initial`](ParsingState::lang_initial) — **the packages step
    /// adds no failure source of its own**: the seed never runs
    /// [`Lang::finalize_transition`] (it has no predecessor), and pushing providers
    /// directly onto the seed's scope stack involves no by-name scope ops (the only
    /// failing kind). The derivation path is not involved — packages-at-seed is not
    /// a transition, and the freeze rebuilds the derived caches over the augmented
    /// data. Anything beyond packages — rules overrides, a mode, events — goes
    /// through the delta idiom instead:
    /// `ParsingState::lang_initial()?.derived(&delta)?`.
    pub fn lang_initial_with_packages(
        packages: impl IntoIterator<Item: IntoSpecsProvider<L>>,
    ) -> Result<ParsingState<L>, FinalizeError>
    where
        L: LangHasScopes,
    {
        let mut data = L::initial_state_data()?;
        for package in packages {
            data.scopes.push(package.into_specs_provider());
        }
        Ok(ParsingState::freeze(data))
    }

    /// Create a state directly from raw data, bypassing [`Lang::finalize_transition`]:
    /// freezes `data` and rebuilds the derived caches. Crate-internal, for two
    /// callers: the deserialization of a serialized state (whose data already passed
    /// the choke point when the state was first built — the serialized form holds
    /// finalized data, and rebuilding it must not run the customizer again), and tests
    /// assembling ad-hoc states. The public paths are
    /// [`lang_initial()`](ParsingState::lang_initial) (+ the packages form) and
    /// [`derived()`](ParsingState::derived), which keep the choke point airtight.
    pub(crate) fn new(data: StateData<L>) -> ParsingState<L> {
        ParsingState::freeze(data)
    }

    /// The sole constructor of non-initial states: every state transition passes
    /// through this method.
    ///
    /// Applies the delta's overrides to a copy of this state's data, strips temporary
    /// group rules when the delta ends their scope (below), runs
    /// [`Lang::finalize_transition`] exactly once, and freezes the result. Derived
    /// caches are rebuilt — except the [`PrefixTable`], which is reused from `self`
    /// (an `Arc` clone) when its inputs are unchanged: same groups gate, same `rules`
    /// and `temporary` lists by elementwise `Arc` identity. The
    /// dominant transition — a group interior overriding only the expected group
    /// close, which is deliberately not a table input — takes the reuse path.
    /// Functional contract: `self` is never observably mutated.
    ///
    /// # Fallibility
    ///
    /// Two failure sources, both folded into [`DeriveError`]:
    ///
    /// - A delta's [`scope_ops`](ParsingStateDelta::scope_ops) can fail (op targets
    ///   an absent provider name; a definition op routed to an immutable provider).
    ///   Every failing op is skipped — the rest of the delta still applies.
    /// - [`Lang::finalize_transition`] can refuse the transition
    ///   ([`FinalizeError`]) — above all when a *context-dependent* event reaches
    ///   this method un-lowered (the two-class contract on [`Lang::Event`]): the
    ///   enclosing-state context such an event needs exists only inside a driven
    ///   parse, where
    ///   [`ParseContext::derive_state`](crate::constructs::ParseContext::derive_state)
    ///   lowers the event before the delta ever gets here.
    ///
    /// A delta without scope ops and without events **cannot fail** under a `Lang`
    /// whose customizer only refuses events. (Override data for a feature the
    /// language declares absent is not a failure source: it is unrepresentable —
    /// the delta's per-feature fields are stored through [`Lang::Features`], and an
    /// absent feature's field cannot carry data.) The error carries the mechanical
    /// failure records plus the fully derived **recovered state** (frozen like any
    /// other; on a finalize refusal, the data as the hook left it), so a tolerant
    /// caller can diagnose and continue while a strict caller aborts.
    /// Classification is the caller's: the in-parse derivation path
    /// ([`ParseContext::derive_state`](crate::constructs::ParseContext::derive_state))
    /// routes scope-op failures through the recovery entry point
    /// ([`ScopeOpFailed`](crate::constructs::ScopeOpFailed)) and treats a finalize
    /// refusal as an implementation error (the driver failed to lower); an embedder
    /// deriving out of parse treats an `Err` as its own input error.
    ///
    /// # Temporary group rules
    ///
    /// [`GroupRules::temporary`](crate::token::GroupRules::temporary) is scoped in
    /// state data, and this method
    /// enforces the scope — every group descent passes through here (installing the
    /// entered rule as the expected close), including hand-built deltas that never
    /// touch the session helpers. A delta that overrides the expected close
    /// (`expecting_close`) ends
    /// the temporaries' scope unless the installed close **is** one of the base's
    /// temporary rules (by `Arc` identity — the same-delimiter descent that keeps
    /// nested minted brackets balancing); installing any other rule, or clearing the
    /// expectation, yields a derived state with the `temporary` list emptied, and
    /// interior inheritance then keeps it empty for the whole subtree (brace protection
    /// at any depth). A delta that explicitly overrides `temporary` itself is
    /// exempt — the delta author spoke. The rule is a pure function of `(base, delta)`,
    /// so identity-keyed derivation memos stay sound.
    #[allow(clippy::result_large_err)] // large `Err` by design — see `DeriveError`
    pub fn derived(&self, delta: &ParsingStateDelta<L>) -> Result<ParsingState<L>, DeriveError<L>> {
        let mut data = self.data.clone();
        let failures = delta.apply_overrides(&mut data);
        // Temporary group rules exist only with the groups feature: a language that
        // declares groups absent has nothing to strip, so the whole enforcement
        // block is compile-time eliminated for it.
        if <L::Features as LangFeatures>::Groups::PRESENT {
            // The delta's groups block is read through the store projection (the
            // `PRESENT` guard already guarantees `Some`, but the projection is what
            // makes the field reachable at all under an unbounded `L`).
            let delta_groups =
                <L::Features as LangFeatures>::Groups::store_get(&delta.rules.groups);
            let ends_temporary_scope =
                match delta_groups.and_then(|groups| groups.expecting_close.as_ref()) {
                    None => false,
                    Some(installed) => !matches!(
                        installed,
                        Some(rule) if self
                            .data
                            .rules
                            .temporary_group_rules()
                            .iter()
                            .any(|temporary| Arc::ptr_eq(temporary, rule))
                    ),
                };
            if ends_temporary_scope
                && delta_groups.is_none_or(|groups| groups.temporary.is_none())
            {
                // Scope enforcement mutates the derived data in place — one of the few
                // by-field writes into a rules block outside delta application; the
                // same projection story as the read above.
                if let Some(groups) =
                    <L::Features as LangFeatures>::Groups::store_get_mut(&mut data.rules.groups)
                {
                    groups.temporary.clear();
                }
            }
        }
        let finalize_error = L::finalize_transition(&mut data, self, &delta.events).err();
        // Checked *after* finalize_transition: the customizer may rewrite the group
        // rules too.
        let table_inputs_unchanged = data.rules.groups_enabled()
            == self.data.rules.groups_enabled()
            && data.rules.group_rules().len() == self.data.rules.group_rules().len()
            && data
                .rules
                .group_rules()
                .iter()
                .zip(self.data.rules.group_rules())
                .all(|(new, old)| Arc::ptr_eq(new, old))
            && data.rules.temporary_group_rules().len()
                == self.data.rules.temporary_group_rules().len()
            && data
                .rules
                .temporary_group_rules()
                .iter()
                .zip(self.data.rules.temporary_group_rules())
                .all(|(new, old)| Arc::ptr_eq(new, old));
        let state = if table_inputs_unchanged {
            // Cloning the store clones the `Arc` when the groups feature is present
            // (the table-reuse path) and is a no-op zero-sized copy when it is absent.
            ParsingState::freeze_with_table(data, self.prefix_table.clone())
        } else {
            ParsingState::freeze(data)
        };
        if failures.is_empty() && finalize_error.is_none() {
            Ok(state)
        } else {
            Err(DeriveError {
                failures,
                finalize_error,
                recovered: state,
                delta: delta.clone(),
            })
        }
    }

    /// The tokenization rules in effect.
    pub fn rules(&self) -> &TokenRules<L> {
        &self.data.rules
    }

    /// The definitions visible in this state: the provider stack. For a language
    /// that declares the scope stack absent ([`Lang::Features`]), the returned stack
    /// is permanently empty.
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

    /// The delimiter-matching table derived from the rules' groups block. `None`
    /// exactly when the language declares the groups feature absent
    /// ([`Lang::Features`]); a state whose groups are merely disabled at runtime
    /// ([`TokenRules::groups_enabled`] off) still answers `Some` of the **empty**
    /// table (the setting is applied at freeze time).
    pub fn prefix_table(&self) -> Option<&PrefixTable<L>> {
        <L::Features as LangFeatures>::Groups::store_get(&self.prefix_table)
            .map(|table| &**table)
    }

    /// The specials trigger-character filter derived via
    /// [`Lang::specials_trigger_chars`]. `None` exactly when the language declares
    /// the specials feature absent ([`Lang::Features`]); a state whose specials scan
    /// is merely disabled at runtime ([`TokenRules::specials_enabled`] off) still
    /// answers `Some` of the **empty** filter (the setting is applied at freeze
    /// time).
    pub fn trigger_chars(&self) -> Option<&TriggerChars> {
        <L::Features as LangFeatures>::Specials::store_get(&self.trigger_chars)
    }

    fn freeze(data: StateData<L>) -> ParsingState<L> {
        // With the groups feature absent there is no table to build — the store is
        // zero-sized and `PrefixTable::for_rules` is never called.
        let prefix_table = <L::Features as LangFeatures>::Groups::store_with(|| {
            Arc::new(PrefixTable::for_rules(&data.rules))
        });
        ParsingState::freeze_with_table(data, prefix_table)
    }

    fn freeze_with_table(
        data: StateData<L>,
        prefix_table: <<L::Features as LangFeatures>::Groups as FeaturePresence>::Store<
            Arc<PrefixTable<L>>,
        >,
    ) -> ParsingState<L> {
        // The specials gate is baked in here: a disabled state stores the empty
        // filter, so `Lang::scan_specials` is unreachable and the hot path never
        // branches on the gate (same treatment as the groups gate in the prefix
        // table). With the specials feature absent, the whole computation collapses
        // with the store — `Lang::specials_trigger_chars` is never called at freeze
        // time for such a language.
        let trigger_chars = <L::Features as LangFeatures>::Specials::store_with(|| {
            if data.rules.specials_enabled() {
                L::specials_trigger_chars(&data)
            } else {
                TriggerChars::default()
            }
        });
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
// only constructors are `lang_initial()` (with its packages form) and `derived()`.

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

/// A [`derived()`](ParsingState::derived) transition that failed: the delta carried
/// failing [`scope ops`](ParsingStateDelta::scope_ops), and/or
/// [`Lang::finalize_transition`] refused the transition.
///
/// Mechanical, deliberately unclassified — whether a failure is an extension bug or an
/// embedder input error is the *caller's* context. The error carries everything a
/// tolerant caller needs to continue (the `String::from_utf8` pattern — recovery
/// material rides in the error):
///
/// - [`failures`](DeriveError::failures): one record per failing op, in delta order;
/// - [`finalize_error`](DeriveError::finalize_error): the customizer's refusal, when
///   [`Lang::finalize_transition`] returned `Err` (typically a context-dependent
///   event reaching [`ParsingState::derived`] un-lowered — the two-class contract on
///   [`Lang::Event`]);
/// - [`recovered`](DeriveError::recovered): the fully derived state with exactly the
///   failing ops skipped — finalized and frozen like every state, ready to continue
///   under (on a finalize refusal: the data as the hook left it, best-effort);
/// - [`delta`](DeriveError::delta): the delta as applied, so a recovering caller can
///   still feed
///   [`ParseDriver::observe_transition`](crate::engine::ParseDriver::observe_transition)
///   the true transition (needed because the group-interior derivation applies a
///   *merged* delta its caller never sees).
///
/// At least one of [`failures`](DeriveError::failures) (non-empty) and
/// [`finalize_error`](DeriveError::finalize_error) (`Some`) is always present.
///
/// Not `Clone`: states are identity-bearing (deliberately non-`Clone`), and the
/// recovered state is a state.
///
/// The `Err` variant is large *by design* — it owns a full state plus the applied
/// delta, because the recovery payload is the point of the type. The functions
/// returning it `#[allow(clippy::result_large_err)]` rather than box: `Box`-free
/// signatures were judged worth the bigger `Result` return slot.
pub struct DeriveError<L: Lang> {
    /// One record per failing op, in delta order (may be empty only when
    /// [`finalize_error`](DeriveError::finalize_error) is `Some`).
    pub failures: Vec<ScopeOpError>,
    /// [`Lang::finalize_transition`]'s refusal, if the customizer returned `Err`.
    pub finalize_error: Option<FinalizeError>,
    /// The derived state with the failing ops skipped (everything else applied).
    pub recovered: ParsingState<L>,
    /// The delta the derivation applied (cloned into the error).
    pub delta: ParsingStateDelta<L>,
}

impl<L: Lang> fmt::Display for DeriveError<L> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "failed to derive a parsing state: ")?;
        let mut first = true;
        for failure in &self.failures {
            if !first {
                write!(f, "; ")?;
            }
            first = false;
            write!(f, "scope op failed: {failure}")?;
        }
        if let Some(finalize_error) = &self.finalize_error {
            if !first {
                write!(f, "; ")?;
            }
            write!(f, "{finalize_error}")?;
        }
        Ok(())
    }
}

impl<L: Lang> fmt::Debug for DeriveError<L> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DeriveError")
            .field("failures", &self.failures)
            .field("finalize_error", &self.finalize_error)
            .field("recovered", &self.recovered)
            .field("delta", &self.delta)
            .finish()
    }
}

impl<L: Lang> core::error::Error for DeriveError<L> {}

/// A [`Lang`] state hook's refusal to produce a parsing state, from either of the
/// two producers on the data→state path:
///
/// - [`Lang::finalize_transition`] refusing a **transition** — the customizer's
///   loud "this delta cannot be applied here". The canonical case: a
///   **context-dependent** event reaching [`ParsingState::derived`] un-lowered
///   (outside any driven parse, or under a driver that failed to lower it) — see
///   the two-class contract on [`Lang::Event`]. Folded into
///   [`DeriveError::finalize_error`] by [`derived()`](ParsingState::derived).
/// - [`Lang::initial_state_data`] refusing to assemble the **seed** data (a seed
///   built from configuration or external definition data can be invalid or
///   unavailable). Passed through by the
///   [`lang_initial`](ParsingState::lang_initial) family.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FinalizeError {
    message: String,
}

impl FinalizeError {
    /// A refusal with the given human-facing description (say *which* event,
    /// invariant, or seed input, and what the caller should have done — e.g.
    /// "derive through a parse context so the driver can lower the event").
    pub fn new(message: impl Into<String>) -> FinalizeError {
        FinalizeError { message: message.into() }
    }

    /// The refusal's description.
    pub fn message(&self) -> &str {
        &self.message
    }
}

impl fmt::Display for FinalizeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Neutral over the two producers (transition refusal, seed refusal): the
        // message names the specific hook and cause.
        write!(f, "cannot build the parsing state: {}", self.message)
    }
}

impl core::error::Error for FinalizeError {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scopes::ScopeStack;
    use crate::state::{
        CommandOverrides, CommentOverrides, ForbiddenCharsOverrides, GroupOverrides,
        ParagraphOverrides, TokenRulesOverrides, WhitespaceOverrides,
    };
    use crate::token::{
        CommandRule, CommandRules, CommentRules, ForbiddenCharsRules, GroupRule, GroupRules,
        ParagraphRules, SpecialsRules, WhitespaceRules,
    };
    use alloc::string::String;
    use alloc::sync::Arc;
    use alloc::vec;
    use alloc::vec::Vec;

    // `Features = AllLangFeatures` (all test languages here declare it): the plain
    // block literals below only typecheck once the per-feature stores normalize to
    // the blocks themselves.
    fn base_rules<L: Lang<GroupTypeId = u32, Features = crate::state::AllLangFeatures>>(
    ) -> TokenRules<L> {
        TokenRules {
            whitespace: WhitespaceRules { enabled: true, chars: " \t\n".into() },
            paragraphs: ParagraphRules { enabled: true },
            groups: GroupRules {
                enabled: true,
                rules: vec![Arc::new(GroupRule {
                    group_type: 0,
                    open: "{".into(),
                    close: "}".into(),
                })],
                temporary: Vec::new(),
                expecting_close: None,
            },
            commands: CommandRules {
                enabled: true,
                rules: vec![Arc::new(CommandRule {
                    escape_char: '\\',
                    name_chars: "abcdefghijklmnopqrstuvwxyz".into(),
                })],
            },
            comments: CommentRules {
                enabled: true,
                rules: Vec::new(),
            },
            specials: SpecialsRules { enabled: true },
            forbidden_chars: ForbiddenCharsRules { chars: "".into() },
        }
    }

    // --- a minimal lang: no customization at all (the TrivialLang shortcut) -----------

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    struct PlainLang;
    impl crate::state::TrivialLang for PlainLang {}

    #[test]
    fn derived_applies_overrides_and_keeps_the_rest() {
        let state: ParsingState<PlainLang> =
            ParsingState::new(StateData { rules: base_rules(), scopes: ScopeStack::new(), mode: (), ext: () });

        let delta = ParsingStateDelta::new().rules(TokenRulesOverrides {
            paragraphs: ParagraphOverrides { enabled: Some(false) },
            forbidden_chars: ForbiddenCharsOverrides { chars: Some("$".into()) },
            ..TokenRulesOverrides::default()
        });
        let derived = state.derived(&delta).unwrap();

        assert!(!derived.rules().paragraphs_enabled());
        assert_eq!(derived.rules().forbidden_chars(), "$");
        // Unchanged fields kept; the original state is untouched.
        assert_eq!(derived.rules().command_rules(), state.rules().command_rules());
        assert!(state.rules().paragraphs_enabled());
    }

    #[test]
    fn derived_rebuilds_prefix_table() {
        let state: ParsingState<PlainLang> =
            ParsingState::new(StateData { rules: base_rules(), scopes: ScopeStack::new(), mode: (), ext: () });
        assert!(state.prefix_table().unwrap().match_at("[x").is_none());

        let delta = ParsingStateDelta::new().rules(TokenRulesOverrides {
            groups: GroupOverrides {
                rules: Some(vec![Arc::new(GroupRule {
                    group_type: 1,
                    open: "[".into(),
                    close: "]".into(),
                })]),
                ..GroupOverrides::default()
            },
            ..TokenRulesOverrides::default()
        });
        let derived = state.derived(&delta).unwrap();
        assert!(derived.prefix_table().unwrap().match_at("[x").is_some());
        assert!(derived.prefix_table().unwrap().match_at("{x").is_none()); // whole-value override
    }

    #[test]
    fn derived_reuses_prefix_table_when_inputs_unchanged() {
        let state: ParsingState<PlainLang> =
            ParsingState::new(StateData { rules: base_rules(), scopes: ScopeStack::new(), mode: (), ext: () });

        // A transition that touches neither the groups gate nor the group rules (by Arc
        // identity) shares the parent's table instance…
        let same = state.derived(&ParsingStateDelta::new().rules(TokenRulesOverrides {
            comments: CommentOverrides::disable(),
            ..TokenRulesOverrides::default()
        })).unwrap();
        assert!(core::ptr::eq(state.prefix_table().unwrap(), same.prefix_table().unwrap()));

        // …and so does the dominant group-interior transition (the expected close is
        // deliberately not a table input)…
        let interior = state.derived(&ParsingStateDelta::new().rules(TokenRulesOverrides {
            groups: GroupOverrides {
                expecting_close: Some(Some(Arc::clone(&state.rules().group_rules()[0]))),
                ..GroupOverrides::default()
            },
            ..TokenRulesOverrides::default()
        })).unwrap();
        assert!(core::ptr::eq(state.prefix_table().unwrap(), interior.prefix_table().unwrap()));

        // …while a group-rules override — even one value-equal to the current rules —
        // builds a fresh table (reuse is keyed on Arc identity, not content).
        let equal_groups: Vec<Arc<GroupRule<PlainLang>>> = state
            .rules()
            .group_rules()
            .iter()
            .map(|rule| Arc::new(GroupRule::clone(rule)))
            .collect();
        let rebuilt = state.derived(&ParsingStateDelta::new().rules(TokenRulesOverrides {
            groups: GroupOverrides { rules: Some(equal_groups), ..GroupOverrides::default() },
            ..TokenRulesOverrides::default()
        })).unwrap();
        assert!(!core::ptr::eq(state.prefix_table().unwrap(), rebuilt.prefix_table().unwrap()));
        assert_eq!(state.prefix_table().unwrap(), rebuilt.prefix_table().unwrap()); // same contents
    }

    #[test]
    fn derived_scopes_temporary_group_rules() {
        let state: ParsingState<PlainLang> =
            ParsingState::new(StateData { rules: base_rules(), scopes: ScopeStack::new(), mode: (), ext: () });
        let temporary = Arc::new(GroupRule { group_type: 9, open: "[".into(), close: "]".into() });
        let with_temp = state.derived(&ParsingStateDelta::new().rules(TokenRulesOverrides {
            groups: GroupOverrides {
                temporary: Some(vec![Arc::clone(&temporary)]),
                ..GroupOverrides::default()
            },
            ..TokenRulesOverrides::default()
        })).unwrap();
        assert!(with_temp.prefix_table().unwrap().match_at("[x").is_some());

        // A delta that says nothing about the expected close carries them over…
        let untouched = with_temp.derived(&ParsingStateDelta::new().rules(TokenRulesOverrides {
            comments: CommentOverrides::disable(),
            ..TokenRulesOverrides::default()
        })).unwrap();
        assert_eq!(untouched.rules().temporary_group_rules().len(), 1);

        // …and so does entering the temporary rule's own group (nested delimiters
        // keep balancing).
        let same_rule = with_temp.derived(&ParsingStateDelta::new().rules(TokenRulesOverrides {
            groups: GroupOverrides {
                expecting_close: Some(Some(Arc::clone(&temporary))),
                ..GroupOverrides::default()
            },
            ..TokenRulesOverrides::default()
        })).unwrap();
        assert_eq!(same_rule.rules().temporary_group_rules().len(), 1);

        // Entering any other group ends the scope: the derived interior has no
        // temporaries (and inheritance keeps it that way for the whole subtree).
        let brace = Arc::clone(&state.rules().group_rules()[0]);
        let stripped = with_temp.derived(&ParsingStateDelta::new().rules(TokenRulesOverrides {
            groups: GroupOverrides {
                expecting_close: Some(Some(Arc::clone(&brace))),
                ..GroupOverrides::default()
            },
            ..TokenRulesOverrides::default()
        })).unwrap();
        assert!(stripped.rules().temporary_group_rules().is_empty());
        assert!(stripped.prefix_table().unwrap().match_at("[x").is_none());

        // Clearing the expectation strips too.
        let cleared = with_temp.derived(&ParsingStateDelta::new().rules(TokenRulesOverrides {
            groups: GroupOverrides {
                expecting_close: Some(None),
                ..GroupOverrides::default()
            },
            ..TokenRulesOverrides::default()
        })).unwrap();
        assert!(cleared.rules().temporary_group_rules().is_empty());

        // An explicit temporaries override wins over stripping: the delta author spoke.
        let other = Arc::new(GroupRule { group_type: 8, open: "<".into(), close: ">".into() });
        let overridden = with_temp.derived(&ParsingStateDelta::new().rules(TokenRulesOverrides {
            groups: GroupOverrides {
                expecting_close: Some(Some(brace)),
                temporary: Some(vec![Arc::clone(&other)]),
                ..GroupOverrides::default()
            },
            ..TokenRulesOverrides::default()
        })).unwrap();
        assert_eq!(overridden.rules().temporary_group_rules(), vec![other]);
    }

    #[test]
    fn temporary_group_rules_are_prefix_table_inputs() {
        let state: ParsingState<PlainLang> =
            ParsingState::new(StateData { rules: base_rules(), scopes: ScopeStack::new(), mode: (), ext: () });
        let temporary = Arc::new(GroupRule { group_type: 9, open: "[".into(), close: "]".into() });
        let with_temp = state.derived(&ParsingStateDelta::new().rules(TokenRulesOverrides {
            groups: GroupOverrides {
                temporary: Some(vec![Arc::clone(&temporary)]),
                ..GroupOverrides::default()
            },
            ..TokenRulesOverrides::default()
        })).unwrap();
        // Installing the temporaries rebuilt the table…
        assert!(!core::ptr::eq(state.prefix_table().unwrap(), with_temp.prefix_table().unwrap()));

        // …the keep-path descent (into the temporary rule itself) reuses it…
        let same_rule = with_temp.derived(&ParsingStateDelta::new().rules(TokenRulesOverrides {
            groups: GroupOverrides {
                expecting_close: Some(Some(Arc::clone(&temporary))),
                ..GroupOverrides::default()
            },
            ..TokenRulesOverrides::default()
        })).unwrap();
        assert!(core::ptr::eq(with_temp.prefix_table().unwrap(), same_rule.prefix_table().unwrap()));

        // …and the strip-path descent rebuilds: a stale reuse here would keep
        // tokenizing the stripped delimiters.
        let stripped = with_temp.derived(&ParsingStateDelta::new().rules(TokenRulesOverrides {
            groups: GroupOverrides {
                expecting_close: Some(Some(Arc::clone(&state.rules().group_rules()[0]))),
                ..GroupOverrides::default()
            },
            ..TokenRulesOverrides::default()
        })).unwrap();
        assert!(!core::ptr::eq(with_temp.prefix_table().unwrap(), stripped.prefix_table().unwrap()));
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
        let state: ParsingState<PlainLang> = ParsingState::lang_initial().expect("seed state");
        assert!(!state.rules().whitespace_enabled());
        assert!(!state.rules().groups_enabled());
        assert!(!state.rules().commands_enabled());
        assert!(!state.rules().comments_enabled());
        assert!(!state.rules().specials_enabled());
        assert!(state.scopes().is_empty());
        assert!(state.prefix_table().unwrap().match_at("{x").is_none());
    }

    #[test]
    fn lang_initial_with_packages_pushes_in_order_over_the_seed() {
        use crate::scopes::{CallableQuery, CallableSyntax, Package, SpecsProvider};
        use crate::spec::StdCallableSpec;
        use alloc::sync::Arc;

        // Packages by value — no `Arc::new` (the sealed `IntoSpecsProvider`
        // conversion); a pre-shared Arc passes through too.
        let mut outer: Package<PlainLang> = Package::new("outer");
        outer.insert(0u32, "cmd", StdCallableSpec::default());
        let mut inner: Package<PlainLang> = Package::new("inner");
        inner.insert(0u32, "cmd", StdCallableSpec::default());

        let state: ParsingState<PlainLang> =
            ParsingState::lang_initial_with_packages([outer, inner]).expect("seed state");
        // Iteration order: last pushed is innermost (provider_names lists
        // innermost-first) — it shadows the ones below.
        assert_eq!(
            state.scopes().provider_names().collect::<alloc::vec::Vec<_>>(),
            ["inner", "outer"]
        );
        let query =
            CallableQuery::new(0u32, "cmd", CallableSyntax::Command { escape_char: '\\' });
        assert!(state.scopes().retrieve_spec(&query, &state).unwrap().is_some());

        // The pre-shared spellings: `Arc<P>` and `Arc<dyn SpecsProvider<L>>`.
        let premade = Arc::new(Package::<PlainLang>::new("premade"));
        let dyn_made: Arc<dyn SpecsProvider<PlainLang>> =
            Arc::new(Package::<PlainLang>::new("dyn"));
        let state: ParsingState<PlainLang> =
            ParsingState::lang_initial_with_packages([premade]).expect("seed state");
        assert_eq!(
            state.scopes().provider_names().collect::<alloc::vec::Vec<_>>(),
            ["premade"]
        );
        let state: ParsingState<PlainLang> =
            ParsingState::lang_initial_with_packages([dyn_made]).expect("seed state");
        assert_eq!(
            state.scopes().provider_names().collect::<alloc::vec::Vec<_>>(),
            ["dyn"]
        );
    }

    // --- a lang with a canonical seed: lang_initial() is the crate-owned freeze of its data --

    #[derive(Debug, Clone, Copy)]
    struct SeededLang;
    impl Lang for SeededLang {
        type Features = crate::state::AllLangFeatures;
        type GroupTypeId = u32;
        type CallableTypeId = u32;
        type ModeId = ();
        type StateExt = ();
        type Event = ();
        type SessionExt = ();
        type SourceOrigin = Option<String>;
        type NodeExts = ();
        type InvocationSyntax = ();
        type Driver = crate::engine::StdParseDriver;

        fn initial_state_data() -> Result<StateData<Self>, FinalizeError> {
            Ok(StateData { rules: base_rules(), scopes: ScopeStack::new(), mode: (), ext: () })
        }
        fn make_node_ext(
            _kind: &crate::node::NodeKind<Self>,
            _span: &crate::source::SourceSpan<Self::SourceOrigin>,
            _state: &alloc::sync::Arc<crate::state::ParsingState<Self>>,
            _children: crate::node::StagedChildren<'_, Self>,
        ) -> Result<(), crate::node::NodeBuildError> {
            Ok(())
        }
    }

    #[test]
    fn initial_freezes_the_langs_seed_data() {
        let state: ParsingState<SeededLang> = ParsingState::lang_initial().expect("seed state");
        assert!(state.rules().groups_enabled());
        // The caches are built from the seed data at freeze:
        assert!(state.prefix_table().unwrap().match_at("{x").is_some());
        // Customizing the starting point goes through derived() — the finalize choke
        // point — and an empty delta reproduces the seed (the coherence contract's
        // mechanical check, trivial here since SeededLang has no normalizer):
        let derived = state.derived(&ParsingStateDelta::new()).unwrap();
        assert_eq!(derived.rules(), state.rules());
    }

    // --- a lang whose seed data itself fails: the Err surfaces at seeding time --------

    #[derive(Debug, Clone, Copy)]
    struct BrokenSeedLang;
    impl Lang for BrokenSeedLang {
        type Features = crate::state::AllLangFeatures;
        type GroupTypeId = u32;
        type CallableTypeId = u32;
        type ModeId = ();
        type StateExt = ();
        type Event = ();
        type SessionExt = ();
        type SourceOrigin = Option<String>;
        type NodeExts = ();
        type InvocationSyntax = ();
        type Driver = crate::engine::StdParseDriver;

        fn initial_state_data() -> Result<StateData<Self>, FinalizeError> {
            Err(FinalizeError::new("the seed definition data is unavailable"))
        }
        fn make_node_ext(
            _kind: &crate::node::NodeKind<Self>,
            _span: &crate::source::SourceSpan<Self::SourceOrigin>,
            _state: &alloc::sync::Arc<crate::state::ParsingState<Self>>,
            _children: crate::node::StagedChildren<'_, Self>,
        ) -> Result<(), crate::node::NodeBuildError> {
            Ok(())
        }
    }

    #[test]
    fn a_failing_seed_surfaces_from_both_seed_constructors() {
        use crate::scopes::Package;

        // The bare seed: initial_state_data's FinalizeError passes through…
        let err = ParsingState::<BrokenSeedLang>::lang_initial().unwrap_err();
        assert_eq!(err.message(), "the seed definition data is unavailable");
        assert_eq!(
            err.to_string(),
            "cannot build the parsing state: the seed definition data is unavailable"
        );
        // …and the packages form adds no failure source of its own — the same
        // error, before any package is pushed.
        let err = ParsingState::<BrokenSeedLang>::lang_initial_with_packages([
            Package::<BrokenSeedLang>::new("unused"),
        ])
        .unwrap_err();
        assert_eq!(err.message(), "the seed definition data is unavailable");
    }

    #[test]
    fn enable_flag_disables_and_reenables_without_carrying_the_data() {
        // The restore problem the enable_* gates exist for (DESIGN_RATIONALE [§dd-dr:tokens]): the
        // re-enabling delta names no CommandRules — the data survived the disabled scope.
        let state: ParsingState<PlainLang> =
            ParsingState::new(StateData { rules: base_rules(), scopes: ScopeStack::new(), mode: (), ext: () });

        let disabled = state.derived(&ParsingStateDelta::new().rules(TokenRulesOverrides {
            commands: CommandOverrides::disable(),
            ..TokenRulesOverrides::default()
        })).unwrap();
        assert!(!disabled.rules().commands_enabled());
        assert_eq!(disabled.rules().command_rules(), state.rules().command_rules());

        let reenabled = disabled.derived(&ParsingStateDelta::new().rules(TokenRulesOverrides {
            commands: CommandOverrides { enabled: Some(true), ..CommandOverrides::default() },
            ..TokenRulesOverrides::default()
        })).unwrap();
        assert!(reenabled.rules().commands_enabled());
        assert_eq!(reenabled.rules().command_rules(), state.rules().command_rules());
    }

    #[test]
    fn groups_gate_rebakes_the_prefix_table() {
        // The gate is baked into the per-state table at freeze time; toggling it through
        // deltas empties and rebuilds the table with the (untouched) group rules.
        let state: ParsingState<PlainLang> =
            ParsingState::new(StateData { rules: base_rules(), scopes: ScopeStack::new(), mode: (), ext: () });
        assert!(state.prefix_table().unwrap().match_at("{x").is_some());

        let disabled = state.derived(&ParsingStateDelta::new().rules(TokenRulesOverrides {
            groups: GroupOverrides::disable(),
            ..TokenRulesOverrides::default()
        })).unwrap();
        assert!(disabled.prefix_table().unwrap().match_at("{x").is_none());
        assert_eq!(disabled.rules().group_rules(), state.rules().group_rules());

        let reenabled = disabled.derived(&ParsingStateDelta::new().rules(TokenRulesOverrides {
            groups: GroupOverrides { enabled: Some(true), ..GroupOverrides::default() },
            ..TokenRulesOverrides::default()
        })).unwrap();
        assert!(reenabled.prefix_table().unwrap().match_at("{x").is_some());
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
        type Features = crate::state::AllLangFeatures;
        type GroupTypeId = u32;
        type CallableTypeId = u32;
        type ModeId = ();
        type StateExt = MathState;
        type Event = MathEvent;
        type SessionExt = ();
        type SourceOrigin = Option<String>;
        type NodeExts = ();
        type InvocationSyntax = ();
        type Driver = crate::engine::StdParseDriver;

        fn finalize_transition(
            new: &mut StateData<Self>,
            _prev: &ParsingState<Self>,
            events: &[MathEvent],
        ) -> Result<(), FinalizeError> {
            for event in events {
                match event {
                    MathEvent::EnterMath => new.ext.in_math = true,
                    MathEvent::LeaveMath => new.ext.in_math = false,
                }
            }
            // Pure normalization: the escape char is a function of the mode. Rules are
            // Arc-shared; rewriting one is clone-on-write.
            for rule in &mut new.rules.commands.rules {
                Arc::make_mut(rule).escape_char = if new.ext.in_math { '#' } else { '\\' };
            }
            Ok(())
        }
        fn make_node_ext(
            _kind: &crate::node::NodeKind<Self>,
            _span: &crate::source::SourceSpan<Self::SourceOrigin>,
            _state: &alloc::sync::Arc<crate::state::ParsingState<Self>>,
            _children: crate::node::StagedChildren<'_, Self>,
        ) -> Result<(), crate::node::NodeBuildError> {
            Ok(())
        }
    }

    #[test]
    fn finalize_centralizes_cross_cutting_rules() {
        let state: ParsingState<MathLang> =
            ParsingState::new(StateData { rules: base_rules(), scopes: ScopeStack::new(), mode: (), ext: MathState::default() });

        let in_math = state.derived(&ParsingStateDelta::new().event(MathEvent::EnterMath)).unwrap();
        assert!(in_math.ext().in_math);
        assert_eq!(in_math.rules().command_rules()[0].escape_char, '#');

        let out_again = in_math.derived(&ParsingStateDelta::new().event(MathEvent::LeaveMath)).unwrap();
        assert!(!out_again.ext().in_math);
        assert_eq!(out_again.rules().command_rules()[0].escape_char, '\\');

        // The intermediate states are untouched (functional contract).
        assert_eq!(state.rules().command_rules()[0].escape_char, '\\');
        assert_eq!(in_math.rules().command_rules()[0].escape_char, '#');
    }

    #[test]
    fn normalization_clobbers_in_scope_overrides_by_design() {
        // The pure-normalization idiom recomputes dependent settings at *every*
        // transition — an explicit escape-char override is clobbered. That trade-off is
        // the customizer author's documented choice (ARCHITECTURE.md [§dd-arch:state]).
        let state: ParsingState<MathLang> =
            ParsingState::new(StateData { rules: base_rules(), scopes: ScopeStack::new(), mode: (), ext: MathState::default() });

        let mut custom = base_rules::<MathLang>().commands.rules;
        Arc::make_mut(&mut custom[0]).escape_char = '@';
        let delta = ParsingStateDelta::new().rules(TokenRulesOverrides {
            commands: CommandOverrides { rules: Some(custom), ..CommandOverrides::default() },
            ..TokenRulesOverrides::default()
        });
        let derived = state.derived(&delta).unwrap();
        assert_eq!(derived.rules().command_rules()[0].escape_char, '\\');
    }

    #[test]
    fn ext_replacement_via_delta() {
        let state: ParsingState<MathLang> =
            ParsingState::new(StateData { rules: base_rules(), scopes: ScopeStack::new(), mode: (), ext: MathState::default() });
        let derived = state.derived(&ParsingStateDelta::new().ext(MathState { in_math: true })).unwrap();
        assert!(derived.ext().in_math);
        // finalize also ran on the replaced ext:
        assert_eq!(derived.rules().command_rules()[0].escape_char, '#');
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
        type Features = crate::state::AllLangFeatures;
        type GroupTypeId = u32;
        type CallableTypeId = u32;
        type ModeId = Mode;
        type StateExt = SeenEdge;
        type Event = ();
        type SessionExt = ();
        type SourceOrigin = Option<String>;
        type NodeExts = ();
        type InvocationSyntax = ();
        type Driver = crate::engine::StdParseDriver;

        fn initial_state_data() -> Result<StateData<Self>, FinalizeError> {
            // Coherent seed: text mode with comments enabled — exactly what
            // finalize_transition would normalize to (the hook's coherence contract).
            Ok(StateData {
                rules: base_rules(),
                scopes: ScopeStack::new(),
                mode: Mode::Text,
                ext: SeenEdge::default(),
            })
        }

        fn finalize_transition(
            new: &mut StateData<Self>,
            prev: &ParsingState<Self>,
            _events: &[()],
        ) -> Result<(), FinalizeError> {
            // Level normalization: comments are a text-mode feature in this toy
            // language — a pure function of the incoming mode.
            new.rules.comments.enabled = new.mode == Mode::Text;
            // Edge visibility: the hook compares prev.mode() with the applied override.
            new.ext =
                SeenEdge { entered_math: prev.mode() == Mode::Text && new.mode == Mode::Math };
            Ok(())
        }
        fn make_node_ext(
            _kind: &crate::node::NodeKind<Self>,
            _span: &crate::source::SourceSpan<Self::SourceOrigin>,
            _state: &alloc::sync::Arc<crate::state::ParsingState<Self>>,
            _children: crate::node::StagedChildren<'_, Self>,
        ) -> Result<(), crate::node::NodeBuildError> {
            Ok(())
        }
    }

    #[test]
    fn default_seed_mode_is_the_mode_ids_default() {
        // A lang with an enum mode that keeps the *default* initial_state_data: the
        // seed's mode is `ModeId::default()` (the `Default` bound exists for this).
        #[derive(Debug, Clone, Copy)]
        struct DefaultSeedLang;
        impl Lang for DefaultSeedLang {
            type Features = crate::state::AllLangFeatures;
            type GroupTypeId = u32;
            type CallableTypeId = u32;
            type ModeId = Mode;
            type StateExt = ();
            type Event = ();
            type SessionExt = ();
            type SourceOrigin = Option<String>;
            type NodeExts = ();
            type InvocationSyntax = ();
            type Driver = crate::engine::StdParseDriver;
            fn make_node_ext(
                _kind: &crate::node::NodeKind<Self>,
                _span: &crate::source::SourceSpan<Self::SourceOrigin>,
                _state: &alloc::sync::Arc<crate::state::ParsingState<Self>>,
                _children: crate::node::StagedChildren<'_, Self>,
            ) -> Result<(), crate::node::NodeBuildError> {
                Ok(())
            }
        }
        let state: ParsingState<DefaultSeedLang> =
            ParsingState::lang_initial().expect("seed state");
        assert_eq!(state.mode(), Mode::Text);
        // TrivialLang languages are modeless: `ModeId = ()`.
        let plain: ParsingState<PlainLang> = ParsingState::lang_initial().expect("seed state");
        assert_eq!(plain.mode(), ());
    }

    #[test]
    fn moded_seed_is_coherent_under_the_empty_delta() {
        // The mechanical pin of the coherence contract, mode included.
        let seed: ParsingState<ModedLang> = ParsingState::lang_initial().expect("seed state");
        assert_eq!(seed.mode(), Mode::Text);
        assert!(seed.rules().comments_enabled());
        let derived = seed.derived(&ParsingStateDelta::new()).unwrap();
        assert_eq!(derived.rules(), seed.rules());
        assert_eq!(derived.mode(), seed.mode());
        assert_eq!(derived.ext(), seed.ext());
    }

    #[test]
    fn mode_overrides_via_delta_and_is_inherited_otherwise() {
        let state: ParsingState<ModedLang> = ParsingState::lang_initial().expect("seed state");

        let math = state.derived(&ParsingStateDelta::new().mode(Mode::Math)).unwrap();
        assert_eq!(math.mode(), Mode::Math);
        assert_eq!(state.mode(), Mode::Text); // functional contract: base untouched

        // A delta that says nothing about the mode carries it over…
        let inherited = math.derived(&ParsingStateDelta::new().rules(TokenRulesOverrides {
            whitespace: WhitespaceOverrides::disable(),
            ..TokenRulesOverrides::default()
        })).unwrap();
        assert_eq!(inherited.mode(), Mode::Math);

        // …and a mode override travels with rules overrides in one delta — one
        // transition, one finalize run.
        let combo =
            state.derived(&ParsingStateDelta::new().mode(Mode::Math).rules(TokenRulesOverrides {
                whitespace: WhitespaceOverrides::disable(),
                ..TokenRulesOverrides::default()
            })).unwrap();
        assert_eq!(combo.mode(), Mode::Math);
        assert!(!combo.rules().whitespace_enabled());
        assert!(!combo.rules().comments_enabled()); // finalize interpreted the mode too
    }

    #[test]
    fn finalize_interprets_mode_changes_seeing_prev_and_new() {
        let state: ParsingState<ModedLang> = ParsingState::lang_initial().expect("seed state");

        // Entering math: level normalization applies (comments off), and the hook saw
        // the Text→Math edge.
        let math = state.derived(&ParsingStateDelta::new().mode(Mode::Math)).unwrap();
        assert!(!math.rules().comments_enabled());
        assert!(math.ext().entered_math);

        // A mode-silent transition inside math: still normalized (level, not edge)…
        let still_math = math.derived(&ParsingStateDelta::new().rules(TokenRulesOverrides {
            whitespace: WhitespaceOverrides::disable(),
            ..TokenRulesOverrides::default()
        })).unwrap();
        assert!(!still_math.rules().comments_enabled());
        // …and no Text→Math edge was seen (prev is already Math).
        assert!(!still_math.ext().entered_math);

        // Leaving math re-enables the text-mode feature (recomputed, not restored).
        let text_again = still_math.derived(&ParsingStateDelta::new().mode(Mode::Text)).unwrap();
        assert!(text_again.rules().comments_enabled());
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
                comments: CommentOverrides::disable(),
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
        assert!(!error.recovered.rules().comments_enabled());
        // …the carried delta is the one that was applied…
        assert_eq!(error.delta.scope_ops.len(), 3);
        // …and the base is untouched (functional contract holds on the Err path too).
        assert_eq!(base.scopes().len(), 0);
        assert!(base.rules().comments_enabled());
    }
}
