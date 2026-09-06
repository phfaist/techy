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

    /// How the language is tokenized, declared as one type
    /// ([`Tokenization`](crate::core::token::Tokenization)): the token type its readers
    /// produce ([`Token<Self>`](crate::core::token::Token)), the stream-position type
    /// they return ([`StreamPosition<Self>`](crate::core::token::StreamPosition)), and
    /// how the reader for a parse over one source is built.
    ///
    /// A language tokenized by [`StdTokenReader`](crate::core::token::StdTokenReader) —
    /// every language in this crate — declares
    /// [`StdTokenization`](crate::core::token::StdTokenization), which supplies
    /// [`StdToken<Self>`](crate::core::token::StdToken),
    /// [`StdStreamPosition`](crate::core::token::StdStreamPosition), and that reader. A
    /// language tokenized differently declares a zero-sized type of its own that
    /// implements [`Tokenization`](crate::core::token::Tokenization).
    ///
    /// This declaration fixes the *types* for the whole parse. Which reader
    /// **instance** serves a given parse remains the driver's decision, through
    /// [`ParseDriver::make_token_reader`](crate::core::ParseDriver::make_token_reader),
    /// whose default body builds the reader this declaration names; a driver that
    /// passes configuration of its own to the reader overrides that hook.
    type Tokenization: Tokenization<Self>;

    /// The node extension types ([`NodeExtTypes`]); `()` for a language with no custom
    /// node data.
    type NodeExts: NodeExtTypes;

    /// What the language records about how each callable invocation was written (the
    /// [`InvocationSyntax`] trait). The value is stored on
    /// [`CallableData::invocation_syntax`](crate::core::node::CallableData::invocation_syntax).
    ///
    /// `()` records nothing; the latexlike preset records its macro, environment, and
    /// specials forms
    /// ([`latexlike::InvocationSyntaxData`](crate::latexlike::InvocationSyntaxData)).
    ///
    /// The value is built by the invocation parser that stages the node: the standard
    /// sites build it through the opt-in
    /// [`FromInvocation`](crate::core::constructs::FromInvocation) contract, while a
    /// parser staging a node itself with
    /// [`stage_node`](crate::core::constructs::ParseContext::stage_node) supplies the
    /// value directly.
    type InvocationSyntax: InvocationSyntax<Self>;

    /// The language's [`ParseDriver`] type: the object that carries the behavior of a
    /// running parse — the recovery policy, command resolution, the state a group
    /// descent installs, and which construct parser handles what. A construct parser
    /// reaches it, with this exact type and no downcast, as
    /// [`ParseContext::driver`](crate::core::constructs::ParseContext::driver).
    ///
    /// The division of labor: `Lang` holds the hooks that can also be called outside a
    /// running parse — [`initial_state_data`](Lang::initial_state_data) and
    /// [`finalize_transition`](Lang::finalize_transition) (state, since `derived()` can
    /// be called anywhere), [`scan_specials`](Lang::scan_specials) and
    /// [`specials_trigger_chars`](Lang::specials_trigger_chars) (tokenization), and
    /// [`make_node_ext`](Lang::make_node_ext) (building nodes, in a parse or in a
    /// transform). Everything that only happens while a parse runs is on the driver
    /// instead. [`TrivialLang`] uses [`StdParseDriver`].
    type Driver: ParseDriver<Self>;

    /// Whether this language's parse trees are **span-tiled**.
    ///
    /// *Span tiling* is the property that:
    ///
    /// - the children of every [`List`](crate::core::node::NodeKind::List) and
    ///   [`Group`](crate::core::node::NodeKind::Group) node tile the parent's interior —
    ///   they lie in one source, in reading order, with no gaps and no overlaps;
    /// - a [`Callable`](crate::core::node::NodeKind::Callable)'s children block is
    ///   span-contiguous within the node's span (with the documented exclusions for
    ///   attached and hidden regions, which lie outside it);
    /// - every positional payload sits at its pinned position: a
    ///   [`Chars`](crate::core::node::NodeKind::Chars) node's content is its whole span, a
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
    /// ([`ParseContext::parse_attached_source`](crate::core::constructs::ParseContext::parse_attached_source)).
    /// Hence the name: the language *obeys* span tiling, or it does not.
    ///
    /// `true` (the default): the parsing machinery enforces the property — a token
    /// stream that breaks it is reported as an implementation error — and every
    /// span-based accessor answers exactly:
    /// [`NodeSlice::span`](crate::core::node::NodeSlice::span) and
    /// [`source_text`](crate::core::node::NodeSlice::source_text) cover a sibling run with
    /// no holes, [`NodeRef::span_content`](crate::core::node::NodeRef::span_content) reads
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
    /// ([`TokenReader::source_span_describing`](crate::core::token::TokenReader::source_span_describing));
    /// its content is recorded as owned text
    /// ([`TextContent::Owned`](crate::source::TextContent::Owned)) unless it lies in the
    /// node's own source; and no tiling holds — span-based accessors answer the
    /// coordinates the parser recorded, nothing more. Trees still satisfy every rule
    /// [`validate_tree`](crate::core::node::validate_tree) checks, and every consumer that
    /// reads node *data* (text content, names, delimiters, payloads) works exactly as
    /// documented.
    const OBEYS_SPAN_TILING: bool = true;

    /// The language's initial (seed) state data: its base token rules, its initial
    /// scope stack including any fallback providers, and its initial state extension.
    ///
    /// The crate freezes what this returns into the seed state,
    /// [`ParsingState::lang_initial`]. Turning data into a state is the crate's job, so
    /// every other state a parse sees comes from
    /// [`derived()`](ParsingState::derived) and has passed through
    /// [`finalize_transition`](Lang::finalize_transition). A caller who wants a
    /// different starting point derives from the seed with a delta; there is no way to
    /// assemble a state from scratch.
    ///
    /// **The seed must already be consistent.** `finalize_transition` does not run on
    /// it — there is no previous state — so the data returned here must already satisfy
    /// every invariant that hook maintains. If `finalize_transition` installs a `$…$`
    /// group rule whenever the mode is math, then a seed whose mode is math must come
    /// with that rule already in place. The same author writes both hooks, and a test
    /// asserting that `lang_initial()?.derived(&ParsingStateDelta::new())` holds the
    /// same data as `lang_initial()?` checks it mechanically.
    ///
    /// The default is the most neutral data possible, [`StateData::empty`]: every
    /// syntax feature turned off, so the content is plain characters with no whitespace
    /// handling, groups, commands, comments, or specials; an empty scope stack; the
    /// default mode and extension. A real language returns its own rules instead.
    ///
    /// # Errors
    ///
    /// Return `Err` ([`FinalizeError`]) when the seed data cannot be assembled: a seed
    /// built from configuration or from external definition data can be invalid or
    /// unavailable, and an embedding whose seed-building code fails reports it the same
    /// way. The failure comes out of the
    /// [`lang_initial()`](ParsingState::lang_initial) family, before any parse exists,
    /// so a broken seed is never parsed with. An implementation that cannot fail wraps
    /// its data in `Ok(...)`, as the default does.
    fn initial_state_data() -> Result<StateData<Self>, FinalizeError> {
        Ok(StateData::empty())
    }

    /// Adjusts the new state data at every state transition: run exactly once per
    /// [`derived()`](ParsingState::derived) call, after the delta's overrides have been
    /// applied and before the new state is frozen.
    ///
    /// This is where a rule that must hold in *every* state is written, once — "in math
    /// mode the escape character is `#`" — instead of in each piece of code that writes
    /// a delta. Whether the implementation recomputes dependent settings from the data
    /// every time or acts only on events is its own choice. It never runs on the seed
    /// state, which has no previous state (see
    /// [`initial_state_data`](Lang::initial_state_data)). The default does nothing.
    ///
    /// **Mode changes are interpreted here.** A delta's
    /// [`mode`](super::ParsingStateDelta::mode) override is already applied to
    /// `new.mode` by the time this hook runs, and that is the whole signal — a mode
    /// change needs no [`Event`](Lang::Event). Compare
    /// [`prev.mode()`](ParsingState::mode) with `new.mode` to react to the change, by
    /// adjusting rules or turning features off. Events remain for changes that are not
    /// mode-shaped.
    ///
    /// **It must be a deterministic function of `(new, prev, events)`**: no side
    /// effects, no interior mutability, no dependence on how often it has been called.
    /// The session computes a repeated identical derivation only once, so this hook
    /// runs once per distinct *derivation* rather than once per transition — parsing
    /// `{a}{b}` under one state runs it **once** for the two descents. Determinism is
    /// what makes reusing the earlier result correct. Anything that must see every
    /// occurrence — counters, caches keyed by position in the document — belongs in
    /// [`ParseDriver::observe_transition`](crate::core::ParseDriver::observe_transition),
    /// which does run at every transition.
    ///
    /// # Errors
    ///
    /// Return `Err` ([`FinalizeError`]) to **refuse** the transition. The main reason
    /// is a *context-dependent* event reaching this hook (see the two kinds of event on
    /// [`Event`](Lang::Event)): such an event means nothing without the enclosing
    /// states, which only the driver has while a parse runs, so an implementation that
    /// recognizes one here must fail rather than quietly ignore it.
    ///
    /// The refusal comes back to the caller as
    /// [`DeriveError::finalize_error`](super::DeriveError::finalize_error). Inside a
    /// driven parse it aborts the parse as an implementation error, since it means the
    /// driver failed to translate the event — a wiring mistake in an extension, not
    /// bad input. The default returns `Ok(())`. This hook never runs on the seed, so
    /// building the seed cannot fail here.
    fn finalize_transition(
        new: &mut StateData<Self>,
        prev: &ParsingState<Self>,
        events: &[Self::Event],
    ) -> Result<(), FinalizeError> {
        let _ = (new, prev, events);
        Ok(())
    }

    /// Reports whether a character sequence that invokes a callable starts at
    /// `content[pos..]`, and which callable it invokes.
    ///
    /// Recognition and lookup happen in the same call: the returned
    /// [`SpecialsMatch`] holds the resolved spec — including whatever the language does
    /// with an unknown name — and the matched text is the name, so the two can never
    /// disagree. A typical implementation searches the state's scope stack
    /// ([`ScopeStack::scan_specials`](crate::core::specs::ScopeStack::scan_specials)).
    ///
    /// Positions are absolute byte offsets into `content`. `pos` is passed to the
    /// implementation unchecked, under the contract documented on
    /// [`SpecsProvider::scan_specials`](crate::core::specs::SpecsProvider::scan_specials):
    /// it is within `content`'s bounds and on a character boundary.
    ///
    /// **Implementer obligations:**
    ///
    /// - A returned match must be non-empty and boundary-aligned: see the contract on
    ///   [`SpecialsMatch::end`]. A zero-width match would hang the parse loop; the
    ///   reader validates the contract.
    /// - A failure is a [`SpecialsScanError`]: a condition plus a byte range in
    ///   `content`. This function cannot describe how to carry on past one — it knows
    ///   neither the reader's token type nor its stream positions — so the reader turns
    ///   the failure into an unrecoverable
    ///   [`TokenError`](crate::core::token::TokenError), which aborts the parse even in
    ///   tolerant mode. A problem the scan *can* carry on from is better expressed as a
    ///   match to a spec whose parser reports it.
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

    /// The characters that may start a specials trigger under `data`: the quick filter
    /// that decides whether [`scan_specials`](Lang::scan_specials) is worth calling at
    /// all.
    ///
    /// It is computed when a state is frozen and stored on that state, so it is
    /// recomputed only at a transition, like the delimiter prefix table. It receives
    /// [`StateData`] rather than [`ParsingState`] because it runs while the state is
    /// still being built. Return [`TriggerChars::Any`] for a scanner too dynamic to
    /// enumerate. The default declares no specials.
    ///
    /// **Implementer obligations:**
    ///
    /// - The returned set must be a conservative *superset*: it must contain the first
    ///   character of every trigger [`scan_specials`](Lang::scan_specials) can match
    ///   under `data` (or be [`TriggerChars::Any`]). An omitted character means the
    ///   trigger silently never fires — no error, no diagnostic.
    /// - Must be a pure function of `data` — the result is cached on the frozen state
    ///   and never re-consulted.
    /// - The specials gate ([`SpecialsRules::enabled`](crate::core::token::SpecialsRules::enabled))
    ///   is applied by the core; the implementation need not
    ///   check it.
    /// - Recomputing at every transition means once per group descent, optional-argument
    ///   probe, and argument delta, so keep it cheap — put anything expensive behind an
    ///   `Arc` in [`StateExt`](Lang::StateExt) and read it from there.
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

    /// Computes the [`NodeExt`] of one node that is about to be staged: the language's
    /// single opportunity to derive per-node data, with all of the node's parts in
    /// view.
    ///
    /// **This is the only required [`Lang`] method**; every other one has a working
    /// default. [`NodeExt`] has no `Default` bound, so a language that declares a real
    /// extension type has to say how it is initialized, and a language without one
    /// writes `Ok(())` — which is what [`TrivialLang`]'s blanket implementation
    /// provides.
    ///
    /// **When it runs.** During parsing, inside
    /// [`ParseContext::stage_node`](crate::core::constructs::ParseContext::stage_node);
    /// outside parsing, wherever a transform author calls it explicitly before
    /// [`NodeTreeBuilder::add`](crate::core::node::NodeTreeBuilder::add). Nowhere else.
    /// It runs **once per node, when the node is created**: a restaged copy keeps the
    /// extension it already has, so there is no second run and no idempotence
    /// requirement.
    ///
    /// `kind` is the node's structural payload, by shared reference — this method reads
    /// it and cannot change it. A language that needs behavior specific to one spec
    /// does the dispatch itself: match a `Callable`, read its `spec`, downcast, compute
    /// the extension.
    ///
    /// `children` is a view of the node's already-staged children
    /// ([`StagedChildren`]) that reaches down the whole subtree: a child view resolves
    /// *its* children in turn, so argument content two levels down is reachable — which
    /// is how `{domain, key}` is computed from `\ref{fig:abc}`. It exposes no siblings,
    /// no ancestors, and no unrelated staged nodes. There is no access to the parent,
    /// because staging works bottom-up and the parent does not exist yet; data that has
    /// to come from above belongs in [`StateExt`](Lang::StateExt).
    ///
    /// The view borrows the staging storage that the pending staging call is about to
    /// grow, so nothing borrowed from it may outlive this call: copy whatever the
    /// extension needs into the returned value. Safe Rust cannot get this wrong, since
    /// the view's lifetime ends with the call and [`NodeExt`] has none; the rule is
    /// stated for code the compiler does not check, such as an embedding that forwards
    /// this hook across a boundary where lifetimes are erased.
    ///
    /// # Errors
    ///
    /// Return `Err` when the extension cannot be computed — normally
    /// [`NodeBuildError::ExtMintFailed`], the variant that exists for exactly this.
    /// The error type is the builder's [`NodeBuildError`] rather than a parse error,
    /// because this method also runs while a consumer builds a tree by hand, where
    /// there is no parse and no span context.
    ///
    /// Inside a parse,
    /// [`ParseContext::stage_node`](crate::core::constructs::ParseContext::stage_node)
    /// reports it like any other builder error, and its callers then split the two
    /// cases: `ExtMintFailed`, this method's own reported failure, becomes a
    /// [`HookFailed`](crate::error::HookFailed) condition, while every other builder
    /// error becomes an
    /// [`ImplementationError`](crate::core::constructs::ImplementationError) (through
    /// [`implementation_error`](crate::core::constructs::ParseContext::implementation_error)).
    /// Either way the parse aborts under any recovery policy, with the live traceback
    /// attached. An implementation that cannot fail wraps its extension in `Ok(...)`.
    fn make_node_ext(
        kind: &NodeKind<Self>,
        span: &SourceSpan<Self::SourceOrigin>,
        state: &Arc<ParsingState<Self>>,
        children: StagedChildren<'_, Self>,
    ) -> Result<NodeExt<Self>, NodeBuildError>;
}

/// An all-defaults language for tests and for experiments with the machinery: writing
/// `impl TrivialLang for MyLang {}` gives `MyLang` a full [`Lang`] implementation.
///
/// Every associated type is filled in — `Features` = [`AllLangFeatures`],
/// `ModeId`, `StateExt`, `Event`, `SessionExt` and `NodeExts` = `()`, `SourceOrigin` =
/// `Option<String>`, `Tokenization` =
/// [`StdTokenization`](crate::core::token::StdTokenization), `GroupTypeId` and
/// `CallableTypeId` = `u32` — and every method keeps its default behavior. The default
/// driver resolves no commands. (Rust has no defaults for associated types, which is
/// why this is a separate trait rather than defaults on [`Lang`].)
///
/// Customizing anything means implementing [`Lang`] directly: a blanket implementation
/// makes the two mutually exclusive, so the first command, real identifier enum, or
/// hook calls for the full implementation.
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

    /// No extension data (`NodeExt = ()`), and it cannot fail.
    fn make_node_ext(
        _kind: &NodeKind<Self>,
        _span: &SourceSpan<Self::SourceOrigin>,
        _state: &Arc<ParsingState<Self>>,
        _children: StagedChildren<'_, Self>,
    ) -> Result<(), NodeBuildError> {
        Ok(())
    }
}

/// A type whose values can all be listed: the opt-in bound for the three per-language
/// vocabularies whose values are fixed when the language is written
/// ([`Lang::CallableTypeId`], [`Lang::GroupTypeId`], [`Lang::ModeId`]).
///
/// Those types are already closed by construction; this trait makes the list available
/// to generic code, so a tool can iterate over every value — calling
/// [`ScopeStack::iter_symbols`](crate::core::specs::ScopeStack::iter_symbols) once for
/// each callable type in `L::CallableTypeId::ALL`, for instance.
///
/// It is deliberately **not** required by [`Lang`]: [`TrivialLang`] uses `u32` for the
/// identifier types, and an integer type has no list of values. A language with real
/// enums implements it — the latexlike preset does for all three — and code that needs
/// to enumerate states the bound where it uses it, as
/// `where L::CallableTypeId: ClosedVocabulary`.
pub trait ClosedVocabulary: Copy + Sized + 'static {
    /// Every value of the vocabulary, in declaration order.
    ///
    /// Implementations must keep this list in sync with the type's variants — for a
    /// `#[non_exhaustive]` enum, adding a variant means extending `ALL` in the same
    /// change.
    const ALL: &'static [Self];
}

/// The one-value vocabulary. This is [`TrivialLang`]'s `ModeId`: a language with "no
/// modes" still has the single mode every one of its states is in.
impl ClosedVocabulary for () {
    const ALL: &'static [()] = &[()];
}
