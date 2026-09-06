//! The [`Lang`] trait — the types and hooks that define a language — with the
//! [`NodeExtTypes`] and [`InvocationSyntax`] bundles it names, the [`TrivialLang`]
//! all-defaults shortcut, and the [`ClosedVocabulary`] enumeration bound.
//!
//! [Defining a custom language](crate::guide::custom_lang) walks through an
//! implementation from scratch.

// `NodeExtTypes` is defined here, next to `Lang`, rather than in the node modules:
// its *meaning* is a node concern, but it is a constituent of the compile-time
// bundle, and moving it there would recreate a module cycle for cosmetics.

use alloc::string::String;
use alloc::sync::Arc;
use core::fmt;
use core::hash::Hash;

use crate::engine::{ParseDriver, StdParseDriver};
use crate::node::{NodeBuildError, NodeExt, NodeKind, StagedChildren};
use crate::source::{Source, SourceOrigin, SourceSpan};
use crate::token::{
    SpecialsMatch, SpecialsScanError, StdTokenization, Tokenization, TriggerChars,
};

use super::features::{AllLangFeatures, LangFeatures};
use super::parsing_state::{FinalizeError, ParsingState, StateData};

/// The node extension types of a language: the data a language attaches to each node,
/// and to each parsed argument and slot record, alongside the structural data.
///
/// An extension never changes what a node *is* — a group with custom data is still a
/// group to every generic tool that reads the tree.
///
/// - [`NodeExt`](NodeExtTypes::NodeExt) is attached to **every** node. A language that
///   wants different data per node kind puts an enum inside it; the one place that
///   creates the value, [`Lang::make_node_ext`], is where the two are kept consistent.
/// - [`ArgumentExt`](NodeExtTypes::ArgumentExt) and
///   [`SlotExt`](NodeExtTypes::SlotExt) are attached to the parsed argument and slot
///   records.
///
/// The three are bundled behind one associated type ([`Lang::NodeExts`]) to keep
/// [`Lang`] small; `()` implements the bundle with every type `()`. Nodes store the
/// extension inline, so keep these types small where possible — an index, or an `Arc`
/// into storage the language owns.
///
/// **An extension value is created once, when the thing it belongs to is created**,
/// by whichever party has the knowledge: the node extension by
/// [`Lang::make_node_ext`] as the node is staged, the argument extension by the
/// [`ArgumentParser`](crate::core::constructs::ArgumentParser) that parsed the
/// argument, the slot extension where the
/// [`ParsedSlot`](crate::core::node::ParsedSlot) record is built. Nothing is ever
/// default-initialized and filled in later, which is why these types carry no
/// `Default` bound; a restaged copy keeps the extension values it was parsed with.
pub trait NodeExtTypes {
    /// The extension attached to every node, created by [`Lang::make_node_ext`].
    type NodeExt: Clone + fmt::Debug + Send + Sync;
    /// The extension of a *parsed argument* record — not of a node: data the language
    /// attaches to one argument of one invocation.
    ///
    /// A reference-parsing extension might store `{domain: "fig", key: "Abc"}` next to
    /// the argument it derived them from, so that nothing has to parse the argument
    /// node again. The value is created by the argument's
    /// [`ArgumentParser`](crate::core::constructs::ArgumentParser) and returned in its
    /// [`ParsedArgumentNodes`](crate::core::constructs::ParsedArgumentNodes) output.
    /// The standard argument parsers are defined only `where ArgumentExt<L>: Default`,
    /// since what they know about a custom extension is nothing. An argument that was
    /// not present has no extension, because nothing was parsed.
    type ArgumentExt: Clone + fmt::Debug + Send + Sync;
    /// The extension of a *parsed slot* record — not of a node: data the language
    /// derives about one content region of one invocation.
    ///
    /// This is the slot-side counterpart of
    /// [`ArgumentExt`](NodeExtTypes::ArgumentExt): a table extension might store the
    /// cell structure of an environment's body, a list extension the item boundaries.
    /// The value is required when the
    /// [`ParsedSlot`](crate::core::node::ParsedSlot) record is built. The latexlike
    /// preset uses this member for its body marker
    /// ([`BodySlotExt`](crate::core::node::BodySlotExt)).
    type SlotExt: Clone + fmt::Debug + Send + Sync;
}

/// The no-extension bundle: every extension type is `()`.
impl NodeExtTypes for () {
    type NodeExt = ();
    type ArgumentExt = ();
    type SlotExt = ();
}

/// The contract on a language's invocation-syntax type ([`Lang::InvocationSyntax`]):
/// how one callable invocation was written.
///
/// A value of this type records the spelling facts of a single invocation — the escape
/// character used, any space that followed it, an environment's begin and end syntax —
/// in whatever canonical form the language prefers. It is stored on the node, as
/// [`CallableData::invocation_syntax`](crate::core::node::CallableData::invocation_syntax).
/// The type is generic over the language because its one method works against the
/// language's own source type.
///
/// This is separate from the node extension ([`NodeExtTypes`]), which holds a
/// language's *derived* data: what is recorded here is what re-emitting the source
/// has to work from. How faithfully a tree can be turned back into source is
/// therefore the language's own choice — byte-exact, exact up to insignificant
/// spacing, or approximate — because recomposition reads only what the node stores.
/// `()` records nothing, in which case a tool that does not know the language sees
/// only the name and span of each callable.
///
/// The latexlike preset records its macro, environment, and specials forms in
/// [`latexlike::InvocationSyntaxData`](crate::latexlike::InvocationSyntaxData).
///
/// Building a value is a separate, opt-in contract,
/// [`FromInvocation`](crate::core::constructs::FromInvocation), which the standard
/// staging sites use. A language whose value cannot be built from an
/// [`Invocation`](crate::core::constructs::Invocation) alone stages its callables
/// through custom parsers instead.
pub trait InvocationSyntax<L: Lang>: Clone + fmt::Debug + Send + Sync + 'static {
    /// A copy of `self` in which every field that is a span into source text has
    /// been replaced by the owned text it refers to.
    ///
    /// `source` is the node's **own** source: in a tree drawn from several sources,
    /// each node is materialized against the one its span refers to. Fields that do
    /// not refer to source text — rule handles, plain characters — are copied
    /// unchanged. Called by
    /// [`NodeTree::materialize`](crate::core::node::NodeTree::materialize) together
    /// with the node's structural payload.
    #[must_use]
    fn materialized(&self, source: &Source<L::SourceOrigin>) -> Self;
}

/// Records nothing about how an invocation was written, so there is nothing to
/// materialize.
impl<L: Lang> InvocationSyntax<L> for () {
    fn materialized(&self, _source: &Source<L::SourceOrigin>) {}
}

/// The definition of a language: the types the parsing machinery is generic over, and
/// the hooks the language implements.
///
/// Implement it to teach the machinery a new markup language. Every core type of the
/// crate takes one `L: Lang` parameter, so the associated types chosen here fix the
/// vocabulary for a whole parse — the mode, group and callable identifiers, the state
/// and node extension types, the tokenization, and the driver.
///
/// A minimal language is a zero-sized type with only the associated types filled in.
/// Every method has a working default — no transition customization, no specials —
/// except [`make_node_ext`](Lang::make_node_ext), which has none because a node
/// extension value has no default ([`NodeExtTypes`]); a language without extension
/// data writes it as the one-liner `Ok(())`. Simpler still, `impl TrivialLang for
/// MyLang {}` supplies everything (see [`TrivialLang`]). The
/// [`latexlike`](crate::latexlike) preset is the worked-out full implementation.
///
/// Every associated type is `Send + Sync`, because parsing states and node trees must
/// be usable from several threads. In practice these types are enums, flags, and
/// `Arc`s, so the bounds cost nothing.
///
/// [Defining a custom language](crate::guide::custom_lang) works through an
/// implementation step by step.
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

    /// Identifier of a group *class*: the language's own answer to "what kind of
    /// delimited region is this?" — the latexlike preset distinguishes a content group
    /// from a math group. It says nothing about how the delimiters are spelled.
    ///
    /// The set of classes is **fixed when the language is written**, so this is
    /// typically a small enum, and a question like "is this a math group?" is answered
    /// by a match rather than a string comparison or a registry lookup.
    ///
    /// Which *delimiter pairs* exist, and which class each one belongs to, is runtime
    /// data instead — [`GroupRule`](crate::core::token::GroupRule) values in the
    /// state's token rules, which any construct parser may extend during a parse. Only
    /// the vocabulary of classes is fixed. [`CallableTypeId`](Lang::CallableTypeId)
    /// works the same way: fixed invocation forms, callables registered at runtime.
    ///
    /// [`TrivialLang`] uses `u32` here.
    type GroupTypeId: Copy + Eq + Hash + fmt::Debug + Send + Sync;

    /// Identifier of a callable *type*: an invocation form — the latexlike preset has
    /// macros, environments, and specials.
    ///
    /// The set of forms is **fixed when the language is written**; new *callables* are
    /// registered at runtime, through the scope stack, but new invocation forms never
    /// are. So this is a per-language enum rather than an open identifier. It is `Ord`
    /// because providers key their definition maps by it. [`TrivialLang`] uses `u32`.
    type CallableTypeId: Copy + Ord + Hash + fmt::Debug + Send + Sync;

    /// Identifier of the **parsing mode** a state is in — the latexlike preset has
    /// text and math modes. Like the group and callable identifiers, the set of modes
    /// is fixed when the language is written.
    ///
    /// The mode is ordinary state data ([`StateData::mode`]) with a matching override
    /// on the delta ([`ParsingStateDelta::mode`](super::ParsingStateDelta::mode)): a
    /// delta *initiates* a mode change and
    /// [`finalize_transition`](Lang::finalize_transition) *interprets* it. Anything may
    /// depend on the mode, including which definitions are visible and how content is
    /// interpreted.
    ///
    /// `Copy + Eq + Hash` because a mode is compared by value when the session decides
    /// whether two derivations are the same. `Default` supplies the seed state's mode
    /// for the default [`initial_state_data`](Lang::initial_state_data).
    /// [`TrivialLang`] uses `()`, meaning the language has no modes.
    type ModeId: Copy + Eq + Hash + Default + fmt::Debug + Send + Sync;

    /// The language's own parsing state — feature-toggle flags, for instance. A
    /// concrete type, not a map of type-erased values; `()` for a language that needs
    /// none. Modal state belongs in [`ModeId`](Lang::ModeId) instead, so a preset needs
    /// no `in_math_mode` flag here.
    ///
    /// **It must be a plain value type, with no interior mutability** — no `Mutex`, no
    /// atomics used for mutation. A parsing state is frozen when it is built, and its
    /// derived caches, including the result of
    /// [`specials_trigger_chars`](Lang::specials_trigger_chars), are computed from this
    /// value at that moment. Changing it afterwards, behind a shared
    /// `Arc<ParsingState>`, would leave those caches describing the old value and break
    /// the guarantee that peeking at a token twice gives the same answer. The
    /// set-once-through-interior-mutability idiom allowed for *node* extensions does
    /// not apply here.
    type StateExt: Clone + fmt::Debug + Default + Send + Sync;

    /// A semantic transition event, such as an `EnterMath`, listed on
    /// [`ParsingStateDelta::events`](super::ParsingStateDelta::events). `()` for a
    /// language that uses none.
    ///
    /// **There are two kinds of event**, and which kind one is decides who interprets
    /// it:
    ///
    /// - A **context-free** event can be interpreted from the new data, the previous
    ///   state, and the events alone. [`finalize_transition`](Lang::finalize_transition)
    ///   interprets it, inside a parse and outside one alike.
    /// - A **context-dependent** event has an effect that depends on the *enclosing*
    ///   states at the point where it is used — the latexlike restore on leaving a math
    ///   context is one. The driver translates it into ordinary overrides
    ///   ([`ParseDriver::resolve_state_event`](crate::core::ParseDriver::resolve_state_event),
    ///   which receives the session's stack of enclosing states) inside
    ///   [`ParseContext::derive_state`](crate::core::constructs::ParseContext::derive_state),
    ///   so it never reaches `finalize_transition`. If one does reach it anyway — a
    ///   bare [`derived()`](ParsingState::derived) call outside a parse — the
    ///   customizer must **return `Err`** rather than ignore it, because the context
    ///   the event needs does not exist there.
    type Event: Clone + fmt::Debug + Send + Sync;

    /// The language's **mutable** data for one whole parse: initialized with
    /// `Default` and stored on the
    /// [`ParserSession`](crate::core::ParserSession). `()` for a language that uses
    /// none.
    ///
    /// This is where a language accumulates what it learns as the parse proceeds —
    /// counters and history recorded from
    /// [`ParseDriver::observe_transition`](crate::core::ParseDriver::observe_transition),
    /// caches that span the parse.
    ///
    /// Unlike [`StateExt`](Lang::StateExt) it is not `Clone`: a session exists for one
    /// parse and is never shared or rolled back, so it is always reached as `&mut`
    /// through the session.
    type SessionExt: fmt::Debug + Default + Send + Sync;

    /// The type describing where a source came from, used as the `O` of
    /// [`Source<O>`](crate::source::Source). Conventionally `Option<String>`.
    type SourceOrigin: SourceOrigin;

    /// The language's tokenization, declared as one type
    /// ([`Tokenization`](crate::token::Tokenization)): the token type its readers
    /// produce ([`Token<Self>`](crate::token::Token)), the stream-position type they
    /// hand out ([`StreamPosition<Self>`](crate::token::StreamPosition)), and how the
    /// reader for a parse over one source is built.
    ///
    /// Languages tokenized by [`StdTokenReader`](crate::token::StdTokenReader) — every
    /// language of this crate — declare
    /// [`StdTokenization`](crate::token::StdTokenization), which supplies
    /// [`StdToken<Self>`](crate::token::StdToken),
    /// [`StdStreamPosition`](crate::token::StdStreamPosition), and that reader. A
    /// language tokenized differently declares a zero-sized type of its own that
    /// implements [`Tokenization`](crate::token::Tokenization).
    ///
    /// The declaration fixes the *types* for the whole parse; which reader **instance**
    /// serves one parse is still a driver decision, through
    /// [`ParseDriver::make_token_reader`](crate::engine::ParseDriver::make_token_reader) —
    /// whose default body builds the reader this declaration names. A driver that hands
    /// its reader configuration it holds overrides that hook.
    type Tokenization: Tokenization<Self>;

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

    /// Whether this language's parse trees are **span-tiled**.
    ///
    /// *Span tiling* is the property that:
    ///
    /// - the children of every [`List`](crate::node::NodeKind::List) and
    ///   [`Group`](crate::node::NodeKind::Group) node tile the parent's interior —
    ///   they lie in one source, in reading order, with no gaps and no overlaps;
    /// - a [`Callable`](crate::node::NodeKind::Callable)'s children block is
    ///   span-contiguous within the node's span (with the documented exclusions for
    ///   attached and hidden regions, which lie outside it);
    /// - every positional payload sits at its pinned position: a
    ///   [`Chars`](crate::node::NodeKind::Chars) node's content is its whole span, a
    ///   comment's start delimiter, content and post-space partition the comment
    ///   node's span, a group's delimiters are the prefix and the suffix of the group
    ///   node's span, and so on.
    ///
    /// A tree with this property is *span-tiled*. That every node carries exactly one
    /// span is a separate, unconditional rule — it holds either way.
    ///
    /// This is a fact about the language's tokenization and parsers, not a choice: the
    /// property holds exactly when the language's token readers serve each parse in
    /// reading order, without gaps, from one source, and a source changes only where a
    /// parser builds a new reader over another source
    /// ([`ParseContext::parse_attached_source`](crate::constructs::ParseContext::parse_attached_source)).
    /// Hence the name: the language *obeys* span tiling, or it does not.
    ///
    /// `true` (the default): the parsing machinery enforces the property — a token
    /// stream that breaks it is reported as an implementation error — and every
    /// span-based accessor answers exactly:
    /// [`NodeSlice::span`](crate::node::NodeSlice::span) and
    /// [`source_text`](crate::node::NodeSlice::source_text) cover a sibling run with
    /// no holes, [`NodeRef::span_content`](crate::node::NodeRef::span_content) reads
    /// back the text the node was parsed from, and the source recomposer re-emits the
    /// input byte for byte.
    ///
    /// `false`: the language's readers may serve tokens from several sources at one
    /// nesting level (a reader that expands macros as it reads is the motivating
    /// case). Several sources are the typical reason to declare `false`, not the only
    /// one: a language declares it for any tokenization its parsers must make no
    /// assumption about — a reader over a single source that skips bytes or serves
    /// them out of order, for instance. The const states what the parsers may assume,
    /// nothing about how many sources a reader draws on. The parsers then make no
    /// assumption about where tokens come from: a node covering several tokens is
    /// recorded with the span the reader *describes*
    /// ([`TokenReader::source_span_describing`](crate::token::TokenReader::source_span_describing));
    /// its content is recorded as owned text
    /// ([`TextContent::Owned`](crate::source::TextContent::Owned)) unless it lies in the
    /// node's own source; and no tiling holds — span-based accessors answer the
    /// coordinates the parser recorded, nothing more. Trees still satisfy every rule
    /// [`validate_tree`](crate::node::validate_tree) checks, and every consumer that
    /// reads node *data* (text content, names, delimiters, payloads) works exactly as
    /// documented.
    const OBEYS_SPAN_TILING: bool = true;

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
    /// Recognition and resolution happen in one call — a [`SpecialsMatch`] carries the
    /// resolved spec (unknown-name fallback policy included) and the matched text is the
    /// name, which makes scanning/lookup mismatches impossible by construction. Typically implemented
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
    ///   reader validates the contract.
    /// - A failure is a [`SpecialsScanError`]: a condition plus a byte range in
    ///   `content`. The hook cannot describe how to carry on past it — it knows neither
    ///   the reader's token type nor its stream positions — so the reader lifts the
    ///   failure into an unrecoverable [`TokenError`](crate::token::TokenError), which
    ///   aborts the parse even in tolerant mode. A condition the scan *can* carry on
    ///   from is expressed as a match to a spec whose parser diagnoses it.
    /// - Specials have the *lowest* recognition precedence: the reader tries group
    ///   delimiters, command escapes, and comment starts first, so a trigger that
    ///   overlaps any of those silently never fires (no diagnostic). The `Lang` author
    ///   is the one who can create — and must avoid — such a collision.
    ///
    /// Only consulted when the current character is in
    /// [`specials_trigger_chars`](Lang::specials_trigger_chars) (cached per state).
    /// The default recognizes nothing.
    fn scan_specials(
        state: &ParsingState<Self>,
        content: &str,
        pos: usize,
    ) -> Result<Option<SpecialsMatch<Self>>, SpecialsScanError> {
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
    ///
    /// Deliberately infallible: a conservative superset is always answerable —
    /// [`TriggerChars::Any`] satisfies the contract with no computation at all.
    /// Embedding or binding code whose implementation can still fail should report
    /// the failure through the embedding's own channel and answer the conservative
    /// superset ([`TriggerChars::Any`]); the per-position work then falls to
    /// [`scan_specials`](Lang::scan_specials), which can fail recoverably.
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
    /// `Ok(())` (what [`TrivialLang`]'s blanket impl does; a `Lang` written directly
    /// spells the `Ok(())` one-liner).
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
    ///
    /// The view **borrows the staging storage the pending staging call is about to
    /// grow** — nothing borrowed from it may be held past this call; whatever the
    /// ext needs is copied into the returned value. A safe Rust implementation
    /// cannot get this wrong (the view's lifetime is call-scoped and
    /// [`NodeExt`] carries none), so the rule is stated for the code the compiler
    /// is not checking: an embedding that adapts this hook across a boundary
    /// where lifetimes are erased must enforce it itself.
    ///
    /// # Errors
    ///
    /// `Err` means the ext could not be computed — typically
    /// [`NodeBuildError::ExtMintFailed`], the variant that exists as this hook's
    /// error channel. The error type is the **builder-level** [`NodeBuildError`],
    /// not a parse error, because the mint also runs for consumer-built trees
    /// (the explicit transform-side recipe), where no parse or span context
    /// exists. Inside a parse, the staging entry point
    /// ([`ParseContext::stage_node`](crate::constructs::ParseContext::stage_node))
    /// reports it like every other builder error, and its callers' lift applies
    /// the condition split: `ExtMintFailed` — the mint's own reported operational
    /// failure — becomes a [`HookFailed`](crate::error::HookFailed) condition,
    /// while every other builder error becomes an
    /// [`ImplementationError`](crate::constructs::ImplementationError)
    /// (via [`implementation_error`](crate::constructs::ParseContext::implementation_error));
    /// either way the parse aborts under any recovery policy, with the live
    /// traceback attached.
    /// An infallible implementation wraps its ext in `Ok(...)` and that is the
    /// only change.
    fn make_node_ext(
        kind: &NodeKind<Self>,
        span: &SourceSpan<Self::SourceOrigin>,
        state: &Arc<ParsingState<Self>>,
        children: StagedChildren<'_, Self>,
    ) -> Result<NodeExt<Self>, NodeBuildError>;
}

/// The trivial language — for tests and machinery experiments: `impl TrivialLang for
/// MyLang {}` yields a [`Lang`] with every associated type defaulted
/// (`Features` = [`AllLangFeatures`], `ModeId`/`StateExt`/`Event`/`SessionExt`/
/// `NodeExts` = `()`, `SourceOrigin` = `Option<String>`,
/// `Tokenization` = [`StdTokenization`](crate::token::StdTokenization),
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
    type Tokenization = StdTokenization;
    type NodeExts = ();
    type InvocationSyntax = ();
    type Driver = StdParseDriver;

    /// The trivial mint: no ext data (`NodeExt = ()`), infallibly.
    fn make_node_ext(
        _kind: &NodeKind<Self>,
        _span: &SourceSpan<Self::SourceOrigin>,
        _state: &Arc<ParsingState<Self>>,
        _children: StagedChildren<'_, Self>,
    ) -> Result<(), NodeBuildError> {
        Ok(())
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
