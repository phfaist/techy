//! The [`Lang`] trait: the compile-time customization bundle; with it, the
//! [`NodeExtTypes`] node-ext bundle and the [`SimpleLang`] all-defaults convenience.
//!
//! `NodeExtTypes` is defined here, next to `Lang`, rather than in the `node` topic:
//! its *meaning* is a node concern, but it is a constituent of the compile-time bundle,
//! and moving it there would recreate a module cycle for cosmetics (ARCHITECTURE.md
//! §engine stratum note).

use alloc::string::String;
use alloc::sync::Arc;
use core::fmt;
use core::hash::Hash;

use alloc::vec::Vec;

use crate::library::LibraryStack;
use crate::node::{BuildId, NodeExt, NodeKind, StagedNodes};
use crate::source::{SourceOrigin, SourceSpan};
use crate::spec::CallableSpec;
use crate::token::{
    SpecialsMatch, Token, TokenResult, TokenRules, TriggerChars, WhitespaceRules,
};

use super::delta::ParsingStateDelta;
use super::parsing_state::{ParsingState, StateData};

/// The bundle of node extension types of a language: the **two-tier ext system** of
/// ARCHITECTURE.md §nodes, orthogonal to structural node identity (a group with custom
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
    /// derives from, instead of re-parsing the argument node (decided July 2026).
    type ArgumentExt: Clone + fmt::Debug + Default + Send + Sync;
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
}

/// The compile-time type bundle of a language definition. Every core type takes one
/// `L: Lang` parameter — never five (ARCHITECTURE.md §2 principle 2).
///
/// A minimal language is a ZST with only the associated types filled in; all methods have
/// working defaults (no transition customization, no specials). The latexlike preset
/// (Phase 7) and FLM are the intended full implementors.
///
/// All associated types are `Send + Sync`: thread-safe states and trees are a core
/// contract (decided July 2026; see DESIGN_RATIONALE.md) — in practice these types are
/// enums, flags, and `Arc`s, so the bounds are nearly free.
pub trait Lang: Sized {
    /// Identifier of a group *class* — the language-native taxonomy of "a delimited
    /// region viewed as one object" (the latexlike preset: content group vs. math
    /// group), **fully detached from delimiter spellings** (revised July 2026; formerly
    /// a per-delimiter-pair identity). **Closed per language**: a language's group
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
    /// macro / environment / specials). **Closed per language** (decided July 2026):
    /// new invocation *forms* are never registered at runtime (new *callables* are —
    /// via libraries), so this is a per-language enum, not an open id. `Ord` because
    /// libraries key their maps by it. [`SimpleLang`] defaults this to `u32`.
    type CallableTypeId: Copy + Ord + Hash + fmt::Debug + Send + Sync;

    /// Language-specific parsing state (e.g. a math-mode flag). Typed — no `Any` maps;
    /// `()` for languages without extra state.
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
    /// ([`observe_transition`](Lang::observe_transition)) and parse-global caches
    /// (decided July 2026, DESIGN_RATIONALE.md §3.6). `()` if unused.
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

    /// The language's canonical initial (seed) state data: base token rules, seed
    /// libraries (unknown-callable fallbacks included), and the initial state ext.
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
    /// whenever the ext says math mode, a seed whose ext says math mode must come with
    /// that rule in place. Both hooks have the same author, which keeps the contract
    /// local; a test asserting `initial().derived(&ParsingStateDelta::new())` is
    /// data-equivalent to `initial()` pins it mechanically.
    ///
    /// The default is the most neutral data: every syntax gate off (character-level
    /// content — no whitespace handling, groups, commands, comments, or specials), no
    /// libraries, default ext. Real languages return their canonical rules instead.
    fn initial_state_data() -> StateData<Self> {
        StateData {
            rules: TokenRules {
                enable_whitespace: false,
                whitespace: WhitespaceRules { chars: String::new() },
                enable_multi_newline_paragraphs: false,
                enable_groups: false,
                groups: Vec::new(),
                enable_commands: false,
                commands: Vec::new(),
                enable_comments: false,
                comments: Vec::new(),
                enable_specials: false,
                forbidden_chars: String::new(),
                expecting_group_close: None,
            },
            libraries: LibraryStack::new(),
            ext: Default::default(),
        }
    }

    /// Transition customizer — the choke-point hook, run exactly once per
    /// [`derived()`](ParsingState::derived) call, after the delta's overrides have
    /// been applied and before the new state is frozen. Cross-cutting rules centralize
    /// here (e.g. FLM's "in math mode the escape char is `#`"); the override policy —
    /// pure normalization vs. event-driven — is this function's business
    /// (ARCHITECTURE.md §state). Never runs on the seed state (see
    /// [`initial_state_data`](Lang::initial_state_data)'s coherence contract). The
    /// default does nothing.
    ///
    /// **Must be a deterministic pure function of `(new, prev, events)`** — no side
    /// effects, no interior mutability, no dependence on call count. Derivations are
    /// deduplicated (the session's group-interior memo), so this runs once per unique
    /// *derivation*, not once per transition: `{a}{b}` under one state runs it **once**
    /// for two descents. Anything history-shaped (counters, caches keyed by occurrence)
    /// belongs in [`observe_transition`](Lang::observe_transition), which fires on every
    /// transition, memo hits included.
    fn finalize_transition(
        new: &mut StateData<Self>,
        prev: &ParsingState<Self>,
        events: &[Self::Event],
    ) {
        let _ = (new, prev, events);
    }

    /// Per-event transition **observation** (decided July 2026, DESIGN_RATIONALE.md
    /// §3.6): called by the session-mediated derivation helpers
    /// ([`ParserSession::derived_state`](crate::engine::ParserSession::derived_state),
    /// [`ParserSession::group_interior_state`](crate::engine::ParserSession::group_interior_state))
    /// on **every** transition event — memo hits included, which is what
    /// [`finalize_transition`](Lang::finalize_transition) structurally cannot see (it
    /// runs once per unique *derivation*, not once per transition). Parse-history
    /// accumulation ("how many times did the parse enter math mode") belongs here, in
    /// the session's [`SessionExt`](Lang::SessionExt) — never in `finalize_transition`,
    /// where structural scope reverts and memoization would make counts wrong twice
    /// over.
    ///
    /// Observational only: it receives the already-frozen `new` state and cannot alter
    /// the transition's outcome (the session layer is data-equivalent to
    /// [`ParsingState::derived`]). The default does nothing.
    fn observe_transition(
        ext: &mut Self::SessionExt,
        prev: &ParsingState<Self>,
        new: &ParsingState<Self>,
        delta: &ParsingStateDelta<Self>,
    ) {
        let _ = (ext, prev, new, delta);
    }

    /// Specials scan: is a callable-triggering character sequence at `content[pos..]`?
    ///
    /// Recognition and resolution happen in one call — a [`SpecialsMatch`] carries both
    /// the name and the resolved spec (unknown-name fallback policy included), which
    /// makes scanning/lookup mismatches impossible by construction. Typically implemented
    /// by a preset dispatching to the state's libraries (Phase 4+). Positions are
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

    // --- Phase 6 dispatch/finalization hooks (July 2026, DESIGN_RATIONALE.md §3.6) ------

    /// Resolve a [`Command`](crate::token::TokenKind::Command) token to its invocation
    /// form and behavior spec. Typically implemented by a preset dispatching to the
    /// state's libraries via a [`CallableQuery`](crate::library::CallableQuery) — the
    /// token carries the fired escape character for syntax disambiguation. `Specials`
    /// tokens need no hook: recognition = resolution, the token already carries its spec.
    ///
    /// The default resolves nothing; the nodes parser then diagnoses the command as
    /// unresolvable and recovers (span-backed chars-node fallback, DESIGN_RATIONALE.md
    /// §3.8).
    fn resolve_command(
        state: &ParsingState<Self>,
        token: &Token<'_, Self>,
    ) -> Option<ResolvedCallable<Self>> {
        let _ = (state, token);
        // ### PhF-Review: It might be good to produce a diagnostic here, so that user's are alerted to missing command resolution machinery in case of "unknown command" errors
        None
    }

    /// The node kind representing a paragraph break. The *core* stages the returned kind
    /// with the token's span and the current state (a `Lang` cannot stage nodes itself);
    /// a preset may return a callable-shaped kind (FLM's paragraph constructs) without
    /// any core change.
    ///
    /// **Constraint:** the kind is staged with *no children*, so a callable-shaped kind
    /// must carry no argument regions and no slots — the builder's region-tiling assert
    /// panics otherwise. (Structurally intrinsic: this hook has no session/builder and
    /// cannot stage children.)
    ///
    /// The default preserves the whitespace-as-chars invariant (§3.5): a whitespace-only
    /// `Chars` kind, span-backed over the full token span (newlines included).
    fn make_paragraph_break_node(
        state: &ParsingState<Self>,
        token: &Token<'_, Self>,
    ) -> NodeKind<Self> {
        let _ = state;
        NodeKind::chars(token.span)
    }

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

/// The result of [`Lang::resolve_command`]: which invocation form the command resolved
/// to, and the behavior spec to drive its parse — exactly what the dispatch loop needs
/// to build an `Invocation` (the core cannot know a preset's type ids).
pub struct ResolvedCallable<L: Lang> {
    /// The invocation form (latexlike: macro / environment / …).
    pub callable_type: L::CallableTypeId,
    /// The resolved behavior spec.
    pub spec: Arc<dyn CallableSpec<L>>,
}

// Manual impls: derives would demand `L:` bounds although only associated types (already
// bounded) and an `Arc` are stored.

impl<L: Lang> Clone for ResolvedCallable<L> {
    fn clone(&self) -> Self {
        ResolvedCallable { callable_type: self.callable_type, spec: Arc::clone(&self.spec) }
    }
}

impl<L: Lang> fmt::Debug for ResolvedCallable<L> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ResolvedCallable")
            .field("callable_type", &self.callable_type)
            .field("spec", &self.spec)
            .finish()
    }
}

/// All-defaults language marker: `impl SimpleLang for MyLang {}` yields a [`Lang`] with
/// every associated type defaulted (`StateExt`/`Event`/`SessionExt`/`NodeExts` = `()`,
/// `SourceOrigin` = `Option<String>`, `GroupTypeId`/`CallableTypeId` = `u32`) and the
/// default method behavior — the workaround for associated-type defaults being unstable.
///
/// The `u32` type ids are the quick-start escape from declaring id enums; a real language
/// definition should implement [`Lang`] directly and give both ids closed enum types.
///
/// A language needing *any* customization implements [`Lang`] directly instead (the
/// blanket impl makes the two mutually exclusive).
pub trait SimpleLang: Sized {}

impl<T: SimpleLang> Lang for T {
    type GroupTypeId = u32;
    type CallableTypeId = u32;
    type StateExt = ();
    type Event = ();
    type SessionExt = ();
    type SourceOrigin = Option<String>;
    type NodeExts = ();
}
