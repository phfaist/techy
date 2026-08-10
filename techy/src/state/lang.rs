//! The [`Lang`] trait: the compile-time customization bundle; with it, the
//! [`NodeExtTypes`] node-ext bundle and the [`TrivialLang`] all-defaults convenience.
//!
//! `NodeExtTypes` is defined here, next to `Lang`, rather than in the `node` topic:
//! its *meaning* is a node concern, but it is a constituent of the compile-time bundle,
//! and moving it there would recreate a module cycle for cosmetics.

use alloc::string::String;
use alloc::sync::Arc;
use core::fmt;
use core::hash::Hash;

use crate::engine::{ParseDriver, StdParseDriver};
use crate::node::{NodeExt, NodeKind, StagedChildren};
use crate::source::{Source, SourceOrigin, SourceSpan};
use crate::token::{SpecialsMatch, TokenResult, TriggerChars};

use super::features::{AllLangFeatures, LangFeatures};
use super::parsing_state::{FinalizeError, ParsingState, StateData};

/// The bundle of node extension types of a language — per-instance language data
/// attached alongside the structural node/record data, orthogonal to structural
/// identity (a group with custom data is still a group to all generic tooling).
///
/// - [`NodeExt`](NodeExtTypes::NodeExt) sits uniformly on **every node**; a lang
///   wanting kind-shaped data uses an enum inside it (coherence is enforced at the
///   single minting point, [`Lang::make_node_ext`]).
/// - [`ArgumentExt`](NodeExtTypes::ArgumentExt) /
///   [`SlotExt`](NodeExtTypes::SlotExt) ride on the parsed argument/slot records.
///
/// Bundled behind one associated type (`Lang::NodeExts`) to keep [`Lang`] small; `()`
/// implements the bundle with every type `()`. Keep ext types word-sized where possible
/// (an index or `Arc` into Lang-owned storage) — nodes store the ext inline.
///
/// **Population is initialization** — deliberately **no `Default` bounds**: an ext
/// value is minted exactly once, at creation, by the party with the knowledge — the
/// node ext by [`Lang::make_node_ext`] at staging, the argument ext by the
/// [`ArgumentParser`](crate::spec::ArgumentParser) that parsed the argument, the slot
/// ext by the invocation composition that mints the
/// [`ParsedSlot`](crate::node::ParsedSlot) record. There is no
/// "default-initialized, populated later" state anywhere in the ext system; restaged
/// copies carry their exts verbatim as frozen parse facts.
pub trait NodeExtTypes {
    /// The uniform ext on every node, minted by [`Lang::make_node_ext`].
    type NodeExt: Clone + fmt::Debug + Send + Sync;
    /// Ext of a *parsed argument* record (not a node kind): language/extension data
    /// attached to one argument of one invocation — e.g. a reference-parsing extension
    /// caching `{domain: "fig", key: "Abc"}` next to the argument whose content it
    /// derives from, instead of re-parsing the argument node. Minted by the argument's
    /// [`ArgumentParser`](crate::spec::ArgumentParser) — its
    /// [`ParsedArgumentNodes`](crate::spec::ParsedArgumentNodes) output carries the
    /// value (the standard parsers are defined only `where ArgumentExt<L>: Default` —
    /// their knowledge about a custom ext *is* "nothing", and the bound says so).
    /// Absent arguments carry no ext (nothing was parsed, so nothing was minted).
    type ArgumentExt: Clone + fmt::Debug + Send + Sync;
    /// Ext of a *parsed slot* record (not a node kind): per-instance derived data about
    /// one content region of one invocation — e.g. a tabular extension caching the cell
    /// structure of an environment's body slot, or an itemize extension caching item
    /// boundaries (the slot-side symmetry of
    /// [`ArgumentExt`](NodeExtTypes::ArgumentExt)). Demanded at
    /// [`ParsedSlot`](crate::node::ParsedSlot) construction; the latexlike preset
    /// claims this member for its body marker
    /// ([`BodySlotExt`](crate::node::BodySlotExt)).
    type SlotExt: Clone + fmt::Debug + Send + Sync;
}

/// The no-ext bundle: every ext type is `()`.
impl NodeExtTypes for () {
    type NodeExt = ();
    type ArgumentExt = ();
    type SlotExt = ();
}

/// The contract on a language's invocation-syntax payload type
/// ([`Lang::InvocationSyntax`]): the recorded **trigger-spelling facts** of one
/// callable invocation — what was written to invoke it (escape character,
/// syntactic post-space, an environment's begin/end syntax), in the language's own logical
/// canonical form. `L`-parameterized like [`ParseDriver<L>`]: the payload's
/// source-facing method speaks the language's own source-origin type.
///
/// The payload is a **parse-level-syntax channel**, distinct from the node ext
/// (preset-logic data, [`NodeExtTypes`]): it is stored as the
/// [`CallableData::invocation_syntax`](crate::node::CallableData::invocation_syntax)
/// field and is what makes recomposition accuracy the *language's* choice —
/// byte-exact vs. up-to-noise vs. loose is decided by what the language records
/// here, and recomposition reads raw node payload only. `()` records nothing (the
/// trivial impl): Lang-agnostic tooling then sees only name + span of the
/// language's callables, by design.
///
/// Like [`NodeExtTypes`], this trait lives beside [`Lang`]: its *meaning* is a
/// node concern, but it is a constituent of the compile-time bundle. The
/// latexlike preset's payload type is the *data* enum
/// [`latexlike::InvocationSyntaxData`](crate::latexlike::InvocationSyntaxData).
///
/// Construction is a separate, opt-in contract
/// ([`FromInvocation`](crate::constructs::FromInvocation)) consulted by the
/// standard staging sites; a language whose payload cannot be built from an
/// [`Invocation`](crate::constructs::Invocation) alone stages its callables
/// through custom parsers instead.
pub trait InvocationSyntax<L: Lang>: Clone + fmt::Debug + Send + Sync + 'static {
    /// A copy with every span-backed field resolved to owned text against
    /// `source` — the carrying node's **own** source (the `Spanned` invariant;
    /// a multi-source tree materializes each node against the source its span
    /// lives in). Called by
    /// [`NodeTree::materialize`](crate::node::NodeTree::materialize) alongside
    /// the structural payload's own materialization. Source-independent fields
    /// (rule `Arc`s, plain chars) pass through unchanged.
    #[must_use]
    fn materialized(&self, source: &Source<L::SourceOrigin>) -> Self;
}

/// The no-record payload: nothing was recorded, nothing to materialize.
impl<L: Lang> InvocationSyntax<L> for () {
    fn materialized(&self, _source: &Source<L::SourceOrigin>) {}
}

/// The compile-time type bundle of a language definition. Every core type takes one
/// `L: Lang` parameter — never five (the one-generic-parameter principle).
///
/// A minimal language is a ZST with only the associated types filled in; every method
/// except [`make_node_ext`](Lang::make_node_ext) has a working default (no transition
/// customization, no specials) — the one exception exists because node exts have no
/// default value ([`NodeExtTypes`]'s population-is-initialization rule; a no-ext lang's
/// body is the empty one-liner). The latexlike preset and FLM are the intended full
/// implementors.
///
/// All associated types are `Send + Sync`: thread-safe states and trees are a core
/// contract — in practice these types are
/// enums, flags, and `Arc`s, so the bounds are nearly free.
// `'static` because a `Lang` is a compile-time type bundle (a unit marker type in
// practice) and `CallableSpec<L>: Any` (the downcast contract) requires every spec
// type — including generic ones like `StdCallableSpec<L>` — to be `'static`.
pub trait Lang: Sized + 'static {
    /// The language's compile-time feature declarations ([`LangFeatures`]): one
    /// presence answer per parsing feature, from whitespace handling to the
    /// definition scope stack. Declaring a feature absent means the language has no
    /// such feature at all — stated once, at the type level, where no runtime data
    /// can contradict it (the [`LangFeatures`] docs define the absent / disabled /
    /// empty vocabulary).
    ///
    /// Full-syntax languages declare [`AllLangFeatures`] — what [`TrivialLang`]'s
    /// blanket impl supplies, and what the [`latexlike`](crate::latexlike) preset
    /// uses; [`NoLangFeatures`](super::NoLangFeatures) declares every feature absent;
    /// any other combination is a custom [`LangFeatures`] type. Code that requires a
    /// feature bounds on the matching per-feature trait
    /// ([`LangHasWhitespace`](super::LangHasWhitespace),
    /// [`LangHasGroups`](super::LangHasGroups), …) rather than spelling the
    /// declaration out.
    type Features: LangFeatures;

    /// Identifier of a group *class* — the language-native taxonomy of "a delimited
    /// region viewed as one object" (the latexlike preset: content group vs. math
    /// group), **fully detached from delimiter spellings**. **Closed per language**: a language's group
    /// classes are known when the `Lang` is written, so this is typically a small enum —
    /// typed answers to "is this a math group?" without string comparison or a registry.
    /// Which *delimiter pairs* exist, and which class each maps to, is runtime data
    /// ([`GroupRule`](crate::token::GroupRule) values in the state's token rules) that
    /// any construct parser may extend mid-parse; only the class vocabulary is fixed —
    /// the exact parallel of [`CallableTypeId`](Lang::CallableTypeId) (closed invocation
    /// *forms*, runtime-registered *callables*). [`TrivialLang`] defaults this to `u32`
    /// for test languages.
    type GroupTypeId: Copy + Eq + Hash + fmt::Debug + Send + Sync;

    /// Identifier of a callable *type* — an invocation form (the latexlike preset:
    /// macro / environment / specials). **Closed per language**:
    /// new invocation *forms* are never registered at runtime (new *callables* are —
    /// via the scope stack), so this is a per-language enum, not an open id. `Ord`
    /// because providers key their maps by it. [`TrivialLang`] defaults this to `u32`.
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
    /// default [`initial_state_data`](Lang::initial_state_data)). [`TrivialLang`]
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
    /// permitted for *node* exts does not carry over here.)
    type StateExt: Clone + fmt::Debug + Default + Send + Sync;

    /// Semantic transition events (e.g. an `EnterMath`), carried on
    /// [`ParsingStateDelta::events`](super::ParsingStateDelta::events). `()` if
    /// unused.
    ///
    /// **Events come in two classes**, and the split decides who consumes them:
    ///
    /// - **Context-free** events — interpretable from `(new, prev, events)` alone —
    ///   are consumed by [`finalize_transition`](Lang::finalize_transition), in and
    ///   out of parses alike.
    /// - **Context-dependent** events — whose effect depends on the *enclosing*
    ///   states at the point of use (the latexlike exit-math-context restore) — are
    ///   lowered to ordinary override patches by the driver
    ///   ([`ParseDriver::resolve_state_event`](crate::engine::ParseDriver::resolve_state_event),
    ///   which receives the session's enclosing-state stack) inside
    ///   [`ParseContext::derive_state`](crate::constructs::ParseContext::derive_state),
    ///   and never reach `finalize_transition`. A context-dependent event that
    ///   *does* reach it — a bare out-of-parse
    ///   [`derived()`](ParsingState::derived) call — must **error loudly**
    ///   (`finalize_transition` returns `Err`), never be silently dropped: the
    ///   context it needs does not exist there.
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

    /// The language's recorded **invocation-syntax payload**
    /// (the [`InvocationSyntax`] bound trait): the trigger-spelling facts stored
    /// per callable invocation on
    /// [`CallableData::invocation_syntax`](crate::node::CallableData::invocation_syntax).
    /// `()` records nothing; the latexlike preset records its macro / environment /
    /// specials forms
    /// ([`latexlike::InvocationSyntaxData`](crate::latexlike::InvocationSyntaxData)).
    ///
    /// Minted by the invocation parser that stages the node — the standard sites
    /// construct it via the opt-in
    /// [`FromInvocation`](crate::constructs::FromInvocation) contract; takeover
    /// parsers staging through
    /// [`stage_node`](crate::constructs::ParseContext::stage_node) supply the
    /// value themselves.
    type InvocationSyntax: InvocationSyntax<Self>;

    /// The language's [`ParseDriver`] type — the **instance** face of parse-time
    /// behavior: recovery policy, command
    /// resolution, the group descent-delta channel, construct provision. Reached by
    /// construct parsers as
    /// [`ParseContext::driver`](crate::constructs::ParseContext::driver), **concretely
    /// typed** — preset parsers call preset helper methods on it with no downcasts.
    ///
    /// Placement rule: `Lang` keeps the static hooks of layers callable outside a
    /// driven parse — [`initial_state_data`](Lang::initial_state_data)/
    /// [`finalize_transition`](Lang::finalize_transition) (state layer; `derived()` is
    /// out-of-parse-callable), [`scan_specials`](Lang::scan_specials)/
    /// [`specials_trigger_chars`](Lang::specials_trigger_chars) (tokenizer layer),
    /// [`make_node_ext`](Lang::make_node_ext) (staging/transform layer). Everything
    /// that only runs while a parse is driven lives on the driver. [`TrivialLang`]
    /// defaults this to [`StdParseDriver`].
    type Driver: ParseDriver<Self>;

    /// The language's canonical initial (seed) state data: base token rules, the seed
    /// scope stack (fallback providers included), and the initial state ext.
    /// The crate freezes the returned data into the seed state
    /// ([`ParsingState::lang_initial`]) — the data→state step is crate-owned, so every other
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
    /// asserting `lang_initial()?.derived(&ParsingStateDelta::new())` is data-equivalent
    /// to `lang_initial()?` pins it mechanically.
    ///
    /// The default is the most neutral data — [`StateData::empty`]: every syntax gate
    /// off (character-level content — no whitespace handling, groups, commands,
    /// comments, or specials), an empty scope stack, default mode and ext. Real
    /// languages return their canonical rules instead.
    ///
    /// # Fallibility
    ///
    /// Returns `Err` ([`FinalizeError`]) when the seed data cannot be assembled —
    /// a seed built from configuration or external definition data can be invalid
    /// or unavailable, and this is where that failure surfaces (an embedding whose
    /// seed-building code fails reports through the same channel). The failure
    /// surfaces from the [`lang_initial`](ParsingState::lang_initial) family, before
    /// any parse exists — a broken seed is never parsed with. An implementation
    /// that cannot fail wraps its data in `Ok(...)` and that is the only change;
    /// the default does exactly that.
    fn initial_state_data() -> Result<StateData<Self>, FinalizeError> {
        Ok(StateData::empty())
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
    ///
    /// # Fallibility
    ///
    /// Returns `Err` ([`FinalizeError`]) to **refuse** the transition — above all
    /// for a *context-dependent* event reaching this hook (see the two-class
    /// contract on [`Event`](Lang::Event)): such an event is meaningless without
    /// the enclosing-state context that only in-parse driver lowering has, and a
    /// customizer that recognizes one here must fail loudly rather than silently
    /// ignore it. The failure folds into the
    /// [`DeriveError`](super::DeriveError) that
    /// [`derived()`](ParsingState::derived) returns
    /// ([`finalize_error`](super::DeriveError::finalize_error)); inside a driven
    /// parse it aborts as an implementation error (the driver failed to lower —
    /// extension wiring, not source input). The default does nothing and returns
    /// `Ok(())`. The seed never runs this hook, so seed construction stays
    /// infallible ([`initial_state_data`](Lang::initial_state_data)'s coherence
    /// contract).
    fn finalize_transition(
        new: &mut StateData<Self>,
        prev: &ParsingState<Self>,
        events: &[Self::Event],
    ) -> Result<(), FinalizeError> {
        let _ = (new, prev, events);
        Ok(())
    }

    /// Specials scan: is a callable-triggering character sequence at `content[pos..]`?
    ///
    /// Recognition and resolution happen in one call — a [`SpecialsMatch`] carries both
    /// the name and the resolved spec (unknown-name fallback policy included), which
    /// makes scanning/lookup mismatches impossible by construction. Typically implemented
    /// as a fold over the state's scope stack
    /// ([`ScopeStack::scan_specials`](crate::scopes::ScopeStack::scan_specials)). Positions are
    /// absolute byte offsets into `content`; `pos` is passed through to implementations
    /// unchecked, under the `pos` contract documented on
    /// [`SpecsProvider::scan_specials`](crate::scopes::SpecsProvider::scan_specials)
    /// (within `content`'s bounds, on a character boundary).
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

    /// The characters that may start a specials trigger under `data` — the fast
    /// pre-check filter for [`scan_specials`](Lang::scan_specials). Computed when a state is
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
    /// - The specials gate ([`SpecialsRules::enabled`](crate::token::SpecialsRules::enabled))
    ///   is applied by the core; the implementation need not
    ///   check it.
    /// - "Rebuilt at transitions" means once per group descent, optional-argument probe,
    ///   and argument delta — keep it cheap (cache expensive derivations in an `Arc`
    ///   inside `StateExt` if needed).
    fn specials_trigger_chars(data: &StateData<Self>) -> TriggerChars {
        let _ = data;
        TriggerChars::default()
    }

    // --- Node-ext minting (API review, DESIGN_RATIONALE.md [§dd-dr:ext-minting]) -----
    //
    // The parse-time dispatch hooks that used to sit here — `resolve_command`,
    // `make_paragraph_break_node`, `refine_diagnostic`, `observe_transition` — migrated
    // to the `ParseDriver` in Phase 7.2 (placement doctrine, DESIGN_RATIONALE.md [§dd-dr:parsers-engine]):
    // `Lang` keeps only hooks of layers callable outside a driven parse.

    /// Mint the [`NodeExt`] of one node about to be staged — the language's **one**
    /// chance to compute per-node data, with the node's full parts in view.
    ///
    /// **The only required [`Lang`] method** (every other method has a working
    /// default): [`NodeExt`] carries no `Default` bound, so a lang that declares a
    /// real ext type must say how it is initialized — and a lang without one returns
    /// `()` (what [`TrivialLang`]'s blanket impl does; a `Lang` written directly
    /// spells the empty one-liner).
    ///
    /// **Who runs it, when**: `make_node_ext` runs inside
    /// [`ParseContext::stage_node`](crate::constructs::ParseContext::stage_node)
    /// during parsing, and wherever a transform author writes the call explicitly
    /// (mint, inspect/adjust if needed, then
    /// [`NodeTreeBuilder::add`](crate::node::NodeTreeBuilder::add)); nowhere else,
    /// ever. It runs **once per node, at creation** — restaged copies carry their
    /// already-minted exts verbatim as frozen parse facts, never re-minted (there is
    /// no idempotence contract because there is no re-run).
    ///
    /// `kind` is the node's structural payload, by shared reference — the hook reads,
    /// it cannot change the kind. A preset dispatches to spec-specific behavior
    /// itself (match a `Callable`, read its `spec`, downcast, compute ext).
    /// `children` is the **subtree-deep, descent-only** view of the node's staged
    /// children ([`StagedChildren`]): child views resolve *their* children
    /// recursively — argument content at grandchild depth is reachable (computing
    /// `{domain, key}` from `\ref{fig:abc}`) — but no siblings, ancestors, or
    /// unrelated staged nodes are exposed. There is deliberately no parent access:
    /// staging is bottom-up, the parent does not exist yet; downward context is
    /// [`StateExt`](Lang::StateExt)'s job.
    fn make_node_ext(
        kind: &NodeKind<Self>,
        span: &SourceSpan<Self::SourceOrigin>,
        state: &Arc<ParsingState<Self>>,
        children: StagedChildren<'_, Self>,
    ) -> NodeExt<Self>;
}

/// The trivial language — for tests and machinery experiments: `impl TrivialLang for
/// MyLang {}` yields a [`Lang`] with every associated type defaulted
/// (`Features` = [`AllLangFeatures`], `ModeId`/`StateExt`/`Event`/`SessionExt`/
/// `NodeExts` = `()`, `SourceOrigin` = `Option<String>`,
/// `GroupTypeId`/`CallableTypeId` = `u32`) and the default method
/// behavior — the workaround for associated-type defaults being unstable. The default
/// driver resolves nothing.
///
/// Any customization means implementing [`Lang`] directly: the blanket impl makes the
/// two mutually exclusive, so the first command, real id enum, or hook forces the full
/// [`Lang`] implementation.
pub trait TrivialLang: Sized + 'static {}

impl<T: TrivialLang> Lang for T {
    type Features = AllLangFeatures;
    type GroupTypeId = u32;
    type CallableTypeId = u32;
    type ModeId = ();
    type StateExt = ();
    type Event = ();
    type SessionExt = ();
    type SourceOrigin = Option<String>;
    type NodeExts = ();
    type InvocationSyntax = ();
    type Driver = StdParseDriver;

    /// The trivial mint: no ext data (`NodeExt = ()`).
    fn make_node_ext(
        _kind: &NodeKind<Self>,
        _span: &SourceSpan<Self::SourceOrigin>,
        _state: &Arc<ParsingState<Self>>,
        _children: StagedChildren<'_, Self>,
    ) {
    }
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
/// Deliberately **not** a required bound on the `Lang` associated types: [`TrivialLang`]
/// defaults the type ids to `u32`, and
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

/// The unit vocabulary: one value. (Matches [`TrivialLang`]'s `ModeId = ()` — "no modes"
/// still has the one mode a state is always in.)
impl ClosedVocabulary for () {
    const ALL: &'static [()] = &[()];
}
