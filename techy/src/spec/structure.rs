//! Argument specs: the declarative description of a callable's invocation shape.
//!
//! An [`ArgumentSpec`] describes one argument a callable accepts: the
//! [`ArgumentParser`] that recognizes and parses it, an optional name for by-name
//! access to the parsed argument, and an optional parsing-state change applying to the
//! argument's own extent. The model follows pylatexenc's `LatexArgumentSpec`.
//!
//! **Every** argument routes to an `ArgumentParser` object. The core cannot know a
//! language's argument forms — which group class a `{…}` argument uses, whether `[…]`
//! is a group rule of the current parsing state or one declared for the argument
//! only — so the standard parsers (delimited group, optional group, literal marker,
//! expression, delimited verbatim) are core `ArgumentParser` implementations in
//! [`constructs`](crate::core::constructs), parameterized by group types and rules. The
//! latexlike preset turns the familiar `'{'` / `'['` / `'*'` / … argument codes into
//! configured instances (`latexlike::argument_specs`).
//!
//! **Arguments and slots.** Arguments *configure* an invocation (`\frac{a}{b}`,
//! `\item[label]`) and are declared here. Slots — the *content regions* of a parsed
//! callable, such as an environment's body, of which a fence-block construct may have
//! several — have no declaration at all: there is no `SlotSpec`. Parsing a body needs
//! invocation facts no declarative list can supply (the `\end{name}` back-reference,
//! the arguments parsed so far — pylatexenc's
//! `EnvironmentSpec.make_body_parser(token, nodeargd, …)`), so a body-bearing
//! callable's `make_invocation_parser` parses the body with whatever parsers it chooses
//! (terminator syntax parameterizes the core `EnvironmentBodyParser`,
//! `constructs::environment_parser`) and creates the
//! [`ParsedSlot`](crate::core::node::ParsedSlot) records itself. Arguments and slots
//! are the same thing in the parsed *records* — a region, a name, an ext — and
//! different things spec-side.

use alloc::boxed::Box;
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::any::Any;
use core::fmt;

use crate::constructs::{ConstructParserResult, ParseContext};
use crate::node::{ArgumentExt, BuildId, ContentNodes};
use crate::state::{Lang, ParsingStateDelta};

/// What an [`ArgumentParser`] returns for a *provided* argument: the region's staged
/// nodes, which of them are the content, and the argument's ext.
///
/// The nodes are the argument's full syntactic extent in source order — leading noise
/// (comment nodes, whitespace-only `Chars` nodes) first, then the syntax-bearing
/// node(s). The standard invocation parser appends them to the callable's child list
/// and records a staged [`ChildRegion`](crate::core::node::ChildRegion) over them;
/// `content` becomes that region's content designation (both [`ContentNodes`] forms are
/// already relative to the region or anchored on a staged node); `ext` becomes the ext
/// of the [`ParsedArgument`](crate::core::node::ParsedArgument) record.
pub struct ParsedArgumentNodes<L: Lang> {
    /// The region's nodes, in source order (staged, not yet claimed by a parent).
    pub nodes: Vec<BuildId>,
    /// The content designation, relative to this region.
    pub content: ContentNodes,
    /// The argument's ext (`Lang::NodeExts::ArgumentExt`), minted by the parser — the
    /// knowledge-holder for the argument it just parsed. The standard parsers know
    /// nothing about a custom ext and fill `Default::default()` under their
    /// `where ArgumentExt<L>: Default` bound; custom parsers mint their own.
    pub ext: ArgumentExt<L>,
}

impl<L: Lang> ParsedArgumentNodes<L> {
    /// A provided argument's output: the region's `nodes`, the `content` designation
    /// among them, and the argument's minted `ext` (`()` for no-ext languages).
    pub fn new(
        nodes: Vec<BuildId>,
        content: ContentNodes,
        ext: ArgumentExt<L>,
    ) -> ParsedArgumentNodes<L> {
        ParsedArgumentNodes { nodes, content, ext }
    }
}

// Manual impls: derives would demand `L:` bounds although only associated types
// (already bounded in `Lang`) are stored.

impl<L: Lang> Clone for ParsedArgumentNodes<L> {
    fn clone(&self) -> Self {
        ParsedArgumentNodes {
            nodes: self.nodes.clone(),
            content: self.content.clone(),
            ext: self.ext.clone(),
        }
    }
}

impl<L: Lang> fmt::Debug for ParsedArgumentNodes<L> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ParsedArgumentNodes")
            .field("nodes", &self.nodes)
            .field("content", &self.content)
            .field("ext", &self.ext)
            .finish()
    }
}

/// How one argument of an invocation is recognized and parsed.
///
/// This is the single argument-parsing interface — the core privileges no argument form
/// of its own — and corresponds to pylatexenc's "any `LatexParserBase` instance as
/// `LatexArgumentSpec.parser`". The standard implementations are in
/// [`constructs`](crate::core::constructs):
/// [`GroupArgumentParser`](crate::core::constructs::GroupArgumentParser),
/// [`OptionalGroupArgumentParser`](crate::core::constructs::OptionalGroupArgumentParser),
/// [`MarkerArgumentParser`](crate::core::constructs::MarkerArgumentParser),
/// [`ExpressionParser`](crate::core::constructs::ExpressionParser). An
/// [`ArgumentSpec`] pairs one of them with the argument's name and state delta.
///
/// The entry point is [`parse_argument`](ArgumentParser::parse_argument). An
/// implementation parses one argument region and stages its nodes, designating which of
/// them are the content ([`ChildRegion`](crate::core::node::ChildRegion) /
/// [`ContentNodes`](crate::core::node::ContentNodes)), or reports the argument absent;
/// the standard invocation path records the result in the node's
/// [`ParsedArguments`](crate::core::node::ParsedArguments) like any other argument. A
/// parser that needs group delimiters declares the
/// [`GroupRule`](crate::core::token::GroupRule)s it wants through a state delta covering
/// the argument's extent — an optional-argument parser declaring `[`…`]` for that
/// argument only, a custom spec declaring `<`…`>`.
///
/// **An argument parser owns its argument's entire region, leading noise included.** It
/// scans whitespace and comments itself (usually with the standard noise-scan helper)
/// and stages them as ordinary nodes — comment nodes, whitespace-only `Chars` nodes —
/// ahead of the argument's syntax. The core never scans noise on a parser's behalf:
/// noise handling is inseparable from the argument's syntax (a verbatim argument whose
/// delimiter is the comment character must see the raw token stream), and the scan must
/// run under the argument's own parsing-state delta.
///
/// **Reporting the argument absent means consuming nothing**: the reader is rewound
/// past any scanned noise, which is then parsed as enclosing content, and nodes staged
/// on speculation are never claimed (the builder drops them).
///
/// **Thread safety is part of the contract** (`Send + Sync` supertraits; see
/// [`CallableSpec`](super::CallableSpec)'s note). An argument parser is stored
/// behavior — `Arc`-shared inside an [`ArgumentSpec`] and reused for every invocation —
/// so it is immutable, takes `&self`, and receives everything specific to one use as
/// arguments; the construct parsers it drives internally are temporaries instead.
///
/// **Downcasting is part of the contract** (`Any` supertrait;
/// [`CallableSpec`](super::CallableSpec)'s downcasting note applies): a consumer
/// recovers a parser's concrete type from a stored `Arc<dyn ArgumentParser<L>>` or
/// `&dyn ArgumentParser<L>`.
pub trait ArgumentParser<L: Lang>: fmt::Debug + Send + Sync + Any {
    /// Parse one argument at the context's current position.
    ///
    /// `cx.state` is the argument's own parsing state: the caller has already applied
    /// [`ArgumentSpec::parsing_state_delta`] on top of the invocation's base state, so
    /// the whole extent — the noise scan included — runs under it, and the caller
    /// reverts it structurally afterwards. `spec` is the argument spec being parsed
    /// against; its `name` serves diagnostics, and its delta is already applied.
    ///
    /// Returns `Ok(Some(_))` with the region's staged nodes and content designation
    /// when the argument is provided, and `Ok(None)` when it is absent — where
    /// **absent means nothing was consumed**: the reader is rewound past any noise
    /// scanned while probing, which is then parsed as enclosing content, and nodes
    /// staged on speculation are never claimed (the builder drops them).
    ///
    /// Whether absence is an error is the parser's own policy: an optional argument
    /// stays silent, while a mandatory one records its diagnostic (tolerant recovery)
    /// or aborts (strict recovery) here, where it detects the absence, before reporting
    /// the argument absent.
    ///
    /// There is deliberately no channel for an after-effect delta: an argument changes
    /// no state beyond its own extent.
    fn parse_argument(
        &self,
        cx: &mut ParseContext<'_, '_, L>,
        spec: &ArgumentSpec<L>,
    ) -> ConstructParserResult<L, Option<ParsedArgumentNodes<L>>>;

    /// Whether this argument can be satisfied by consuming nothing — whether reporting
    /// it absent is a *valid* outcome rather than a diagnosed recovery.
    ///
    /// An optional group or a `*` marker: yes. A mandatory group or an expression: no.
    /// (pylatexenc's `LatexParserBase.contents_can_be_empty`.)
    ///
    /// The default of
    /// [`CallableSpec::requires_content`](super::CallableSpec::requires_content)
    /// consults this, and the guard at an expression position uses that answer to
    /// decide whether a callable may appear *bare* as a single-token argument
    /// (`\frac\mymacro 2` is fine when every argument of `\mymacro` can match empty).
    ///
    /// Defaults to `true`, like pylatexenc's base class; a parser that *requires*
    /// syntax must override it to `false`. Leaving the default costs its callables that
    /// guard diagnostic — they dispatch in full, parsing their arguments greedily in
    /// expression position — whereas a wrongly `false` answer would diagnose valid
    /// input.
    fn can_match_empty(&self) -> bool {
        true
    }
}

mod sealed {
    use super::{ArgumentParser, Lang};
    use alloc::sync::Arc;

    // Inference markers: they let the by-value blanket coexist with the Arc
    // pass-through impls (trait coherence would otherwise reject the pair on a
    // Lang-generic trait — the [`IntoCallableSpec`](crate::spec::IntoCallableSpec)
    // mechanism note). Callers never name them — the marker parameter is inferred;
    // each argument shape matches exactly one impl.
    pub struct ByValue;
    pub struct SharedConcrete;
    pub struct SharedDyn;

    pub trait SealedParser<L: Lang, M> {}
    impl<L: Lang, P: ArgumentParser<L> + 'static> SealedParser<L, ByValue> for P {}
    impl<L: Lang, P: ArgumentParser<L> + 'static> SealedParser<L, SharedConcrete> for Arc<P> {}
    impl<L: Lang> SealedParser<L, SharedDyn> for Arc<dyn ArgumentParser<L>> {}
}

/// Sealed conversion into a shared [`ArgumentParser`] — what [`ArgumentSpec::new`] and
/// [`new_unnamed`](ArgumentSpec::new_unnamed) accept as their parser argument.
///
/// A parser passes **by value**
/// (`ArgumentSpec::new(GroupArgumentParser::new(…), "title")`, with no `Arc::new` at
/// the call site), while an already-shared **`Arc<P>`** or
/// **`Arc<dyn ArgumentParser<L>>`** passes through unchanged rather than being wrapped
/// a second time, so one parser reused across many argument specs stays one value. The
/// spec-side counterpart is [`IntoCallableSpec`](super::IntoCallableSpec).
///
/// Sealed: the three implementations are the whole vocabulary; downstream code
/// implements [`ArgumentParser`], never this trait. (The `M` parameter is a sealed
/// inference marker distinguishing the three argument shapes — it never needs to be
/// named.)
pub trait IntoArgumentParser<L: Lang, M>: sealed::SealedParser<L, M> {
    /// Convert into the shared parser handle [`ArgumentSpec`]s store.
    fn into_argument_parser(self) -> Arc<dyn ArgumentParser<L>>;
}

impl<L: Lang, P: ArgumentParser<L> + 'static> IntoArgumentParser<L, sealed::ByValue> for P {
    fn into_argument_parser(self) -> Arc<dyn ArgumentParser<L>> {
        Arc::new(self)
    }
}

impl<L: Lang, P: ArgumentParser<L> + 'static> IntoArgumentParser<L, sealed::SharedConcrete>
    for Arc<P>
{
    fn into_argument_parser(self) -> Arc<dyn ArgumentParser<L>> {
        self
    }
}

impl<L: Lang> IntoArgumentParser<L, sealed::SharedDyn> for Arc<dyn ArgumentParser<L>> {
    fn into_argument_parser(self) -> Arc<dyn ArgumentParser<L>> {
        self
    }
}

/// One argument accepted by a callable (pylatexenc's `LatexArgumentSpec`).
///
/// Build one with [`new`](ArgumentSpec::new), which names it, or with
/// [`new_unnamed`](ArgumentSpec::new_unnamed), and collect the arguments of one
/// callable into a [`StdCallableSpec`](super::StdCallableSpec) or a preset spec type.
/// For a latexlike language the whole list is usually written as argument codes
/// instead — `"o"` for an optional `[…]` argument, `"m"` for a mandatory one — see
/// [`argument_specs`](crate::latexlike::argument_specs) for the code table.
pub struct ArgumentSpec<L: Lang> {
    /// The parser that recognizes and parses this argument ([`ArgumentParser`]).
    pub parser: Arc<dyn ArgumentParser<L>>,
    /// Optional name for by-name access to the parsed argument (pylatexenc's
    /// `argname`). Names outlast positions: inserting an optional argument renumbers
    /// the positions, while the names stay valid.
    pub name: Option<Box<str>>,
    /// Parse this argument under a modified parsing state (pylatexenc's
    /// `parsing_state_delta`): `\text{…}` leaves math mode for its argument, `\href`'s
    /// URL argument disables specials. The state is derived for the argument's extent
    /// and reverts structurally at its end.
    pub parsing_state_delta: Option<ParsingStateDelta<L>>,
}

impl<L: Lang> ArgumentSpec<L> {
    /// An argument named `name`, parsed by `parser`.
    ///
    /// Prefer this over [`new_unnamed`](ArgumentSpec::new_unnamed): a name gives
    /// by-name access to the parsed argument and survives changes to the argument list
    /// that positions do not. The parser passes by value or already `Arc`-shared
    /// ([`IntoArgumentParser`]).
    pub fn new<M>(
        parser: impl IntoArgumentParser<L, M>,
        name: impl Into<Box<str>>,
    ) -> ArgumentSpec<L> {
        ArgumentSpec {
            parser: parser.into_argument_parser(),
            name: Some(name.into()),
            parsing_state_delta: None,
        }
    }

    /// An unnamed argument with the given parser ([`new`](ArgumentSpec::new) names it).
    pub fn new_unnamed<M>(parser: impl IntoArgumentParser<L, M>) -> ArgumentSpec<L> {
        ArgumentSpec {
            parser: parser.into_argument_parser(),
            name: None,
            parsing_state_delta: None,
        }
    }

    /// Parse the argument under the state derived through `delta`.
    pub fn with_state_delta(mut self, delta: ParsingStateDelta<L>) -> ArgumentSpec<L> {
        self.parsing_state_delta = Some(delta);
        self
    }
}

// Manual impls: derives would demand `L:` bounds although only associated types (already
// bounded in `Lang`) and `Arc`s are stored. No `PartialEq`: a spec carries a parser
// (behavior has no structural equality) and possibly a state delta.

impl<L: Lang> Clone for ArgumentSpec<L> {
    fn clone(&self) -> Self {
        ArgumentSpec {
            parser: Arc::clone(&self.parser),
            name: self.name.clone(),
            parsing_state_delta: self.parsing_state_delta.clone(),
        }
    }
}

impl<L: Lang> fmt::Debug for ArgumentSpec<L> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ArgumentSpec")
            .field("parser", &self.parser)
            .field("name", &self.name)
            .field("parsing_state_delta", &self.parsing_state_delta)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::constructs::MarkerArgumentParser;
    use alloc::format;

    #[derive(Debug, Clone, Copy)]
    struct PlainLang;
    impl crate::state::TrivialLang for PlainLang {}

    #[test]
    fn dyn_argument_parser_downcasts_to_its_concrete_type() {
        // The `Any` supertrait: a stored `Arc<dyn ArgumentParser<L>>` gives its
        // concrete type back through dyn-to-`Any` upcasting.
        let parser: Arc<dyn ArgumentParser<PlainLang>> =
            Arc::new(MarkerArgumentParser::new("*"));
        let any: &dyn Any = &*parser;
        let recovered = any
            .downcast_ref::<MarkerArgumentParser>()
            .expect("downcast to MarkerArgumentParser");
        // The recovered reference answers the concrete value's own state — the "*"
        // marker (readable here through the `Debug` rendering only).
        assert!(format!("{recovered:?}").contains("\"*\""), "{recovered:?}");
    }
}
