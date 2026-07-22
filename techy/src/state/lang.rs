//! The [`Lang`] trait: the compile-time customization bundle; with it, the
//! [`NodeExtTypes`] node-ext bundle and the [`SimpleLang`] all-defaults convenience.
//!
//! `NodeExtTypes` is defined here, next to `Lang`, rather than in the `node` topic:
//! its *meaning* is a node concern, but it is a constituent of the compile-time bundle,
//! and moving it there would recreate a module cycle for cosmetics.

use alloc::string::String;
use alloc::sync::Arc;
use core::fmt;
use core::hash::Hash;

use alloc::vec::Vec;

use crate::engine::{ParseDriver, StdParseDriver};
use crate::node::{BuildId, NodeExt, NodeKind, StagedNodes};
use crate::scopes::ScopeStack;
use crate::source::{SourceOrigin, SourceSpan};
use crate::token::{
    SpecialsMatch, TokenResult, TokenRules, TriggerChars, WhitespaceRules,
};


use super::parsing_state::{ParsingState, StateData};

/// The bundle of node extension types of a language: the **two-tier ext system**, orthogonal to structural node identity (a group with custom
/// data is still a group to all generic tooling).
///
/// Tier 1 — [`NodeExt`](NodeExtTypes::NodeExt) — sits uniformly on every node
/// (cross-cutting per-instance concerns: bindings handles, IDs, …). Tier 2 — the
/// per-kind `<Kind>NodeExt` types — carries kind-specific per-instance parse results.
///
/// Bundled behind one associated type (`Lang::NodeExts`) to keep [`Lang`] small; `()`
/// implements the bundle with every type `()`. Keep ext types word-sized where possible
/// (an index or `Arc` into Lang-owned storage) — `NodeKind` stores tier-2 exts inline.
///
/// The `Default` bound supplies the value builders use when a node carries no meaningful
/// ext (consistent with `Lang::StateExt: Default`).
pub trait NodeExtTypes {
    /// Tier 1: the uniform ext on every node.
    type NodeExt: Clone + fmt::Debug + Default + Send + Sync;
    /// Tier 2: ext of `Chars` nodes.
    type CharsNodeExt: Clone + fmt::Debug + Default + Send + Sync;
    /// Tier 2: ext of `Group` nodes.
    type GroupNodeExt: Clone + fmt::Debug + Default + Send + Sync;
    /// Tier 2: ext of `Callable` nodes.
    type CallableNodeExt: Clone + fmt::Debug + Default + Send + Sync;
    /// Tier 2: ext of `Comment` nodes.
    type CommentNodeExt: Clone + fmt::Debug + Default + Send + Sync;
    /// Tier 2: ext of `List` nodes.
    type ListNodeExt: Clone + fmt::Debug + Default + Send + Sync;
    /// Ext of a *parsed argument* record (not a node kind): language/extension data
    /// attached to one argument of one invocation — e.g. a reference-parsing extension
    /// caching `{domain: "fig", key: "Abc"}` next to the argument whose content it
    /// derives from, instead of re-parsing the argument node.
    type ArgumentExt: Clone + fmt::Debug + Default + Send + Sync;
    /// Ext of a *parsed slot* record (not a node kind): per-instance derived data about
    /// one content region of one invocation — e.g. a tabular extension caching the cell
    /// structure of an environment's body slot, or an itemize extension caching item
    /// boundaries (the slot-side symmetry of
    /// [`ArgumentExt`](NodeExtTypes::ArgumentExt)).
    type SlotExt: Clone + fmt::Debug + Default + Send + Sync;
}

/// The no-ext bundle: every ext type is `()`.
impl NodeExtTypes for () {
    type NodeExt = ();
    type CharsNodeExt = ();
    type GroupNodeExt = ();
    type CallableNodeExt = ();
    type CommentNodeExt = ();
    type ListNodeExt = ();
    type ArgumentExt = ();
    type SlotExt = ();
}

/// The compile-time type bundle of a language definition. Every core type takes one
/// `L: Lang` parameter — never five (the one-generic-parameter principle).
///
/// A minimal language is a ZST with only the associated types filled in; all methods have
/// working defaults (no transition customization, no specials). The latexlike preset
/// and FLM are the intended full implementors.
///
/// All associated types are `Send + Sync`: thread-safe states and trees are a core
/// contract — in practice these types are
/// enums, flags, and `Arc`s, so the bounds are nearly free.
// `'static` because a `Lang` is a compile-time type bundle (a unit marker type in
// practice) and `CallableSpec<L>: Any` (the downcast contract) requires every spec
// type — including generic ones like `StdCallableSpec<L>` — to be `'static`.
pub trait Lang: Sized + 'static {
    /// Identifier of a group *class* — the language-native taxonomy of "a delimited
    /// region viewed as one object" (the latexlike preset: content group vs. math
    /// group), **fully detached from delimiter spellings**. **Closed per language**: a language's group
    /// classes are known when the `Lang` is written, so this is typically a small enum —
    /// typed answers to "is this a math group?" without string comparison or a registry.
    /// Which *delimiter pairs* exist, and which class each maps to, is runtime data
    /// ([`GroupRule`](crate::token::GroupRule) values in the state's token rules) that
    /// any construct parser may extend mid-parse; only the class vocabulary is fixed —
    /// the exact parallel of [`CallableTypeId`](Lang::CallableTypeId) (closed invocation
    /// *forms*, runtime-registered *callables*). [`SimpleLang`] defaults this to `u32`
    /// for quick-start and test languages.
    type GroupTypeId: Copy + Eq + Hash + fmt::Debug + Send + Sync;

    /// Identifier of a callable *type* — an invocation form (the latexlike preset:
    /// macro / environment / specials). **Closed per language**:
    /// new invocation *forms* are never registered at runtime (new *callables* are —
    /// via the scope stack), so this is a per-language enum, not an open id. `Ord`
    /// because providers key their maps by it. [`SimpleLang`] defaults this to `u32`.
    type CallableTypeId: Copy + Ord + Hash + fmt::Debug + Send + Sync;

    /// Identifier of the **parsing mode** a state is in (the latexlike preset: text /
    /// math; verbatim-ish modes are candidates) — the third closed per-language
    /// vocabulary after [`GroupTypeId`](Lang::GroupTypeId) and
    /// [`CallableTypeId`](Lang::CallableTypeId), though deliberately not a `…TypeId`:
    /// it names the mode a state *is in*, not a classification of a syntactic object
    /// (the crate's Id-naming rule). Stored as plain state data
    /// ([`StateData::mode`]) with a matching [`ParsingStateDelta::mode`](super::ParsingStateDelta::mode) override
    /// channel: deltas *initiate* mode changes, and
    /// [`finalize_transition`](Lang::finalize_transition) *interprets* them.
    /// Mode is not lookup-private: definition visibility
    /// and any content-interpretation decision may key on it.
    ///
    /// `Copy + Eq + Hash` because modes are memo-key material — the session's
    /// derivation memo keys the delta's mode override *by value* (exact, unlike the
    /// identity-keyed rule payloads); `Default` supplies the seed state's mode (the
    /// default [`initial_state_data`](Lang::initial_state_data)). [`SimpleLang`]
    /// defaults this to `()` — no modes.
    type ModeId: Copy + Eq + Hash + Default + fmt::Debug + Send + Sync;

    /// Language-specific parsing state (e.g. feature-toggle flags). Typed — no `Any`
    /// maps; `()` for languages without extra state. Modal state belongs in the
    /// first-class [`ModeId`](Lang::ModeId) field instead — a preset needs no
    /// `in_math_mode` flag here.
    ///
    /// **Must be a plain value type — no interior mutability** (no `Mutex`, no atomics
    /// used for mutation): states are frozen at construction and their derived caches
    /// (including [`specials_trigger_chars`](Lang::specials_trigger_chars)'s result) are
    /// computed from the ext at freeze time. Mutating an ext behind a shared
    /// `Arc<ParsingState>` would silently desynchronize those caches and break the
    /// readers' peek-idempotence contract. (The interior-mutable set-once idiom
    /// sanctioned for *node* exts does not carry over here.)
    type StateExt: Clone + fmt::Debug + Default + Send + Sync;

    /// Semantic transition events (e.g. an `EnterMath`), consumed by
    /// [`finalize_transition`](Lang::finalize_transition). `()` if unused.
    type Event: Clone + fmt::Debug + Send + Sync;

    /// Parse-global **mutable** extension, `Default`-initialized and stored on
    /// [`ParserSession`](crate::engine::ParserSession) — the preset-owned mutable object
    /// of a parse, and the home for parse-history accumulation
    /// ([`ParseDriver::observe_transition`](crate::engine::ParseDriver::observe_transition))
    /// and parse-global caches.
    /// `()` if unused.
    ///
    /// Unlike [`StateExt`](Lang::StateExt) this is not `Clone`: sessions are transient
    /// single-parse objects, never shared or reverted — access is always `&mut`, through
    /// the session.
    type SessionExt: fmt::Debug + Default + Send + Sync;

    /// Origin metadata type for sources (plugged into `Source<O>`); conventionally
    /// `Option<String>`.
    type SourceOrigin: SourceOrigin;

    /// The node extension type bundle ([`NodeExtTypes`]); `()` for languages without
    /// custom node data.
    type NodeExts: NodeExtTypes;

    /// The language's [`ParseDriver`] type — the **instance** face of parse-time
    /// behavior: recovery policy, command
    /// resolution, the group descent-delta channel, construct provision. Reached by
    /// construct parsers as
    /// [`ParseContext::driver`](crate::constructs::ParseContext::driver), **concretely
    /// typed** — preset parsers call preset helper methods on it with no downcasts.
    ///
    /// Placement doctrine: `Lang` keeps the static hooks of layers callable outside a
    /// driven parse — [`initial_state_data`](Lang::initial_state_data)/
    /// [`finalize_transition`](Lang::finalize_transition) (state layer; `derived()` is
    /// out-of-parse-callable), [`scan_specials`](Lang::scan_specials)/
    /// [`specials_trigger_chars`](Lang::specials_trigger_chars) (tokenizer layer),
    /// [`finalize_node`](Lang::finalize_node) (builder/transform layer). Everything
    /// that only runs while a parse is driven lives on the driver. [`SimpleLang`]
    /// defaults this to [`StdParseDriver`].
    type Driver: ParseDriver<Self>;

    /// The language's canonical initial (seed) state data: base token rules, the seed
    /// scope stack (fallback providers included), and the initial state ext.
    /// The crate freezes the returned data into the seed state
    /// ([`ParsingState::initial`]) — the data→state step is crate-owned, so every other
    /// state a parse sees comes from [`derived()`](ParsingState::derived) and passes
    /// through [`finalize_transition`](Lang::finalize_transition). Callers customize the
    /// starting point by deriving from the seed with a delta, never by assembling a
    /// state from scratch.
    ///
    /// **Coherence contract:** `finalize_transition` does *not* run on the seed (it has
    /// no previous state), so the returned data must already satisfy every invariant the
    /// customizer maintains — if `finalize_transition` installs a `$…$` group rule
    /// whenever the mode is math, a seed whose mode is math must come with that rule in
    /// place. Both hooks have the same author, which keeps the contract local; a test
    /// asserting `initial().derived(&ParsingStateDelta::new())` is data-equivalent to
    /// `initial()` pins it mechanically.
    ///
    /// The default is the most neutral data: every syntax gate off (character-level
    /// content — no whitespace handling, groups, commands, comments, or specials), an
    /// empty scope stack, default mode and ext. Real languages return their canonical
    /// rules instead.
    fn initial_state_data() -> StateData<Self> {
        StateData {
            rules: TokenRules {
                enable_whitespace: false,
                whitespace: WhitespaceRules { chars: "".into() },
                enable_multi_newline_paragraphs: false,
                enable_groups: false,
                groups: Vec::new(),
                temporary_groups: Vec::new(),
                enable_commands: false,
                commands: Vec::new(),
                enable_comments: false,
                comments: Vec::new(),
                enable_specials: false,
                forbidden_chars: "".into(),
                expecting_group_close: None,
            },
            scopes: ScopeStack::new(),
            mode: Default::default(),
            ext: Default::default(),
        }
    }

    /// Transition customizer — the choke-point hook, run exactly once per
    /// [`derived()`](ParsingState::derived) call, after the delta's overrides have
    /// been applied and before the new state is frozen. Cross-cutting rules centralize
    /// here (e.g. FLM's "in math mode the escape char is `#`"); the override policy —
    /// pure normalization vs. event-driven — is this function's business.
    /// Never runs on the seed state (see
    /// [`initial_state_data`](Lang::initial_state_data)'s coherence contract). The
    /// default does nothing.
    ///
    /// **Mode transitions are interpreted here**:
    /// a delta's [`mode`](super::ParsingStateDelta::mode) override is already applied to
    /// `new.mode` when this hook runs — the override *is* the signal, no
    /// [`Event`](Lang::Event) needed for mode-shaped transitions. Compare
    /// [`prev.mode()`](ParsingState::mode) with `new.mode` to react to the change
    /// (adjust rules, disable features); events remain for non-modal semantics.
    ///
    /// **Must be a deterministic pure function of `(new, prev, events)`** — no side
    /// effects, no interior mutability, no dependence on call count. Derivations are
    /// deduplicated (the session's derivation memo — overrides-only deltas, keyed by
    /// `Arc` identity), so this runs once per unique *derivation*, not once per
    /// transition: `{a}{b}` under one state runs it **once** for two descents. That
    /// purity is also what makes the memo sound: a pointer-keyed hit substitutes a
    /// previous run's result. Anything history-shaped (counters, caches keyed by
    /// occurrence) belongs in
    /// [`ParseDriver::observe_transition`](crate::engine::ParseDriver::observe_transition), which
    /// fires on every transition, memo hits included.
    fn finalize_transition(
        new: &mut StateData<Self>,
        prev: &ParsingState<Self>,
        events: &[Self::Event],
    ) {
        let _ = (new, prev, events);
    }

    /// Specials scan: is a callable-triggering character sequence at `content[pos..]`?
    ///
    /// Recognition and resolution happen in one call — a [`SpecialsMatch`] carries both
    /// the name and the resolved spec (unknown-name fallback policy included), which
    /// makes scanning/lookup mismatches impossible by construction. Typically implemented
    /// as a fold over the state's scope stack
    /// ([`ScopeStack::scan_specials`](crate::scopes::ScopeStack::scan_specials)). Positions are
    /// absolute byte offsets into `content`.
    ///
    /// **Implementer obligations:**
    ///
    /// - A returned match must be non-empty and boundary-aligned: see the contract on
    ///   [`SpecialsMatch::end`]. A zero-width match would hang the parse loop; the
    ///   reader debug-asserts the contract.
    /// - May fail recoverably, but the recovery-token protocol has teeth: a
    ///   [`TokenError`](crate::token::TokenError) *without* a recovery token aborts the
    ///   parse even in tolerant mode, and a recovery's `resume_pos` must strictly
    ///   advance past the failed read's start (see
    ///   [`TokenRecovery::resume_pos`](crate::token::TokenRecovery)) — the content loop
    ///   aborts otherwise.
    /// - Specials have the *lowest* recognition precedence: the reader tries group
    ///   delimiters, command escapes, and comment starts first, so a trigger that
    ///   overlaps any of those silently never fires (no diagnostic). The `Lang` author
    ///   is the one who can create — and must avoid — such a collision.
    ///
    /// Only consulted when the current character is in
    /// [`specials_trigger_chars`](Lang::specials_trigger_chars) (cached per state).
    /// The default recognizes nothing.
    fn scan_specials<'s>(
        state: &ParsingState<Self>,
        content: &'s str,
        pos: usize,
    ) -> TokenResult<'s, Self, Option<SpecialsMatch<'s, Self>>> {
        let _ = (state, content, pos);
        Ok(None)
    }

    /// The characters that may start a specials trigger under `data` — the hot-path
    /// filter for [`scan_specials`](Lang::scan_specials). Computed when a state is
    /// frozen and cached on the state instance (rebuilt only at transitions, like the
    /// `PrefixTable`); receives [`StateData`] rather than [`ParsingState`] because it
    /// runs *while* the state is being built. Return [`TriggerChars::Any`] for fully
    /// dynamic scanners. The default: no specials.
    ///
    /// **Implementer obligations:**
    ///
    /// - The returned set must be a conservative *superset*: it must contain the first
    ///   character of every trigger [`scan_specials`](Lang::scan_specials) can match
    ///   under `data` (or be [`TriggerChars::Any`]). An omitted character means the
    ///   trigger silently never fires — no error, no diagnostic.
    /// - Must be a pure function of `data` — the result is cached on the frozen state
    ///   and never re-consulted.
    /// - The `enable_specials` gate is applied by the core; the implementation need not
    ///   check it.
    /// - "Rebuilt at transitions" means once per group descent, optional-argument probe,
    ///   and argument delta — keep it cheap (cache expensive derivations in an `Arc`
    ///   inside `StateExt` if needed).
    fn specials_trigger_chars(data: &StateData<Self>) -> TriggerChars {
        let _ = data;
        TriggerChars::default()
    }

    // --- Phase 6 finalization hook (July 2026, DESIGN_RATIONALE.md [§dd-dr:parsers-engine]) ---------------
    //
    // The parse-time dispatch hooks that used to sit here — `resolve_command`,
    // `make_paragraph_break_node`, `refine_diagnostic`, `observe_transition` — migrated
    // to the `ParseDriver` in Phase 7.2 (placement doctrine, DESIGN_RATIONALE.md [§dd-dr:parsers-engine]):
    // `Lang` keeps only hooks of layers callable outside a driven parse.

    /// Centralized node finalization, run by
    /// [`NodeTreeBuilder::add`](crate::node::NodeTreeBuilder::add) for **every** staged
    /// node (all kinds, before the staging checks). The builder is the single mutation
    /// boundary, so no node escapes finalization — no parser cooperation required;
    /// transforms and tests included. A preset dispatches to spec-specific behavior
    /// itself (match a `Callable`, read its `spec`, downcast, attach ext), and uniform
    /// per-node initialization gets a natural home.
    ///
    /// Implementations must be **idempotent**: transform-built trees pass nodes through
    /// a new builder, re-running finalization on already-finalized data. The hook also
    /// runs on speculatively staged nodes that may be abandoned (harmless — they drop
    /// unreachable). `staged` is the read-only view of the already-staged nodes, so a
    /// callable's hook can inspect its `children`. The default does nothing.
    fn finalize_node(
        kind: &mut NodeKind<Self>,
        ext: &mut NodeExt<Self>,
        span: &SourceSpan<Self::SourceOrigin>,
        parsing_state: &Arc<ParsingState<Self>>,
        children: &[BuildId],
        staged: &StagedNodes<'_, Self>,
    ) {
        let _ = (kind, ext, span, parsing_state, children, staged);
    }
}

/// All-defaults language marker: `impl SimpleLang for MyLang {}` yields a [`Lang`] with
/// every associated type defaulted (`ModeId`/`StateExt`/`Event`/`SessionExt`/`NodeExts`
/// = `()`, `SourceOrigin` = `Option<String>`, `GroupTypeId`/`CallableTypeId` = `u32`)
/// and the default method behavior — the workaround for associated-type defaults being
/// unstable.
///
/// The `u32` type ids are the quick-start escape from declaring id enums; a real language
/// definition should implement [`Lang`] directly and give both ids closed enum types.
///
/// A language needing *any* customization implements [`Lang`] directly instead (the
/// blanket impl makes the two mutually exclusive).
pub trait SimpleLang: Sized + 'static {}

impl<T: SimpleLang> Lang for T {
    type GroupTypeId = u32;
    type CallableTypeId = u32;
    type ModeId = ();
    type StateExt = ();
    type Event = ();
    type SessionExt = ();
    type SourceOrigin = Option<String>;
    type NodeExts = ();
    type Driver = StdParseDriver;
}

/// A closed vocabulary type that can list all of its values — the opt-in tooling bound
/// for the closed per-language vocabularies ([`Lang::CallableTypeId`],
/// [`Lang::GroupTypeId`], [`Lang::ModeId`]).
///
/// "Closed per language" means the values are known when the `Lang` is written; this
/// trait makes that closedness *statically listable*, so generic tooling can enumerate
/// (e.g. drive [`ScopeStack::iter_symbols`](crate::scopes::ScopeStack::iter_symbols)
/// once per callable type in `L::CallableTypeId::ALL`).
///
/// Deliberately **not** a required bound on the `Lang` associated types: [`SimpleLang`]
/// defaults the type ids to `u32` (the quick-start escape from declaring id enums), and
/// an open integer type has no value list. Languages with real id enums implement it
/// (the latexlike preset does for all three vocabularies); tooling that needs
/// enumeration states the bound where it is used
/// (`where L::CallableTypeId: ClosedVocabulary`).
pub trait ClosedVocabulary: Copy + Sized + 'static {
    /// Every value of the vocabulary, in declaration order.
    ///
    /// Implementations must keep this list in sync with the type's variants — for a
    /// `#[non_exhaustive]` enum, adding a variant means extending `ALL` in the same
    /// change.
    const ALL: &'static [Self];
}

/// The unit vocabulary: one value. (Matches [`SimpleLang`]'s `ModeId = ()` — "no modes"
/// still has the one mode a state is always in.)
impl ClosedVocabulary for () {
    const ALL: &'static [()] = &[()];
}
