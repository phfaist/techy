//! Argument specs: the declarative description of a callable's invocation shape.
//!
//! **Modeled on pylatexenc's `LatexArgumentSpec`**: an argument *is* a parser, optionally named, and may
//! request a modified parsing state for its own extent. **Every** argument routes to an
//! [`ArgumentParser`] object: the core cannot know a language's argument forms — which group class a
//! `{…}` argument uses, whether `[…]` is a group rule of the current state or a
//! momentarily-minted one — so the standard parsers (delimited group, optional group,
//! literal marker, expression, delimited verbatim) live in
//! [`constructs`](crate::core::constructs) as core `ArgumentParser` implementations,
//! parameterized by group types and rules; the preset's code factory
//! (`latexlike::argument_specs`) resolves pylatexenc's `'{'` / `'['` /
//! `'*'` / … shorthands into configured instances.
//!
//! **Arguments vs. slots.** Arguments *configure* an invocation (`\frac{a}{b}`,
//! `\item[label]`) and are declared here, spec-side. Slots — the *content regions* of a
//! parsed callable (an environment's body; a fence-block construct may have several) —
//! are pure **record-level** vocabulary: there is no `SlotSpec`. Body parsing needs
//! invocation facts no declarative list can supply — the `\end{name}` back-reference,
//! the arguments parsed so far (pylatexenc's `EnvironmentSpec.make_body_parser(token,
//! nodeargd, …)` precedent) — so a body-bearing callable's sanctioned
//! `make_invocation_parser` takeover parses its body with whatever parsers it chooses
//! (terminator syntax parameterizes the core `EnvironmentBodyParser`,
//! `constructs::environment_parser`) and mints the
//! [`ParsedSlot`](crate::node::ParsedSlot) records directly. Arguments and slots are
//! thus the same thing at the *record* level (region + name + ext), not at the
//! spec/parser level.

use alloc::boxed::Box;
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::fmt;

use crate::constructs::{ConstructParserResult, ParseContext};
use crate::node::{ArgumentExt, BuildId, ContentNodes};
use crate::state::{Lang, ParsingStateDelta};

/// What an [`ArgumentParser`] returns for a *provided* argument: the region's staged
/// nodes, the content designation among them, and the argument's minted ext (the
/// `parse_argument` shape).
///
/// The nodes are the argument's full syntactic extent in source order — leading noise
/// (comment nodes, whitespace-only `Chars` nodes), then the syntax-bearing node(s). The
/// standard invocation parser appends them to the callable's child list and records a
/// staged [`ChildRegion`](crate::node::ChildRegion) over them; `content` passes through
/// as the region's designation (both [`ContentNodes`] forms are already relative to the
/// region / anchored on a staged node); `ext` becomes the
/// [`ParsedArgument`](crate::node::ParsedArgument) record's ext.
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

/// An argument parser: how one argument of an invocation is recognized and parsed —
/// pylatexenc's "any `LatexParserBase` instance as `LatexArgumentSpec.parser`", and the
/// single argument-parsing interface (no privileged data forms in the core).
///
/// The entry point is [`parse_argument`](ArgumentParser::parse_argument).
/// An implementation parses one argument region and stages its nodes, designating the
/// content nodes among them ([`ChildRegion`](crate::node::ChildRegion) /
/// [`ContentNodes`](crate::node::ContentNodes)), or reports the argument absent; the
/// standard invocation path records the result in the node's
/// [`ParsedArguments`](crate::node::ParsedArguments) like any other argument. Parsers
/// needing group delimiters install the [`GroupRule`](crate::token::GroupRule)s they
/// want via a state delta for the argument's extent (an optional-argument parser
/// momentarily declaring `[`…`]`, a custom spec declaring `<`…`>`). The standard
/// implementations live in [`constructs`](crate::core::constructs):
/// [`GroupArgumentParser`](crate::constructs::GroupArgumentParser),
/// [`OptionalGroupArgumentParser`](crate::constructs::OptionalGroupArgumentParser),
/// [`MarkerArgumentParser`](crate::constructs::MarkerArgumentParser),
/// [`ExpressionParser`](crate::constructs::ExpressionParser).
///
/// **Noise ownership**:
/// an argument parser owns its argument's *entire* region, leading noise included — it
/// scans whitespace and comments itself (typically via the standard noise-scan helper) and stages them as ordinary nodes (comment nodes, whitespace-only `Chars`
/// nodes) ahead of the argument's syntax. The core never scans noise on a parser's
/// behalf: noise policy is inseparable from the argument's syntax (a verbatim argument
/// whose delimiter is the comment character must see the raw token stream), and the
/// scan must run under the argument's own parsing-state delta. Reporting the argument
/// **absent means consuming nothing**: the reader is rewound past any scanned noise —
/// it is re-parsed as enclosing content — and speculatively staged nodes are simply
/// never claimed (the builder drops them).
///
/// **Thread safety is part of the contract** (`Send + Sync` supertraits;
/// see [`CallableSpec`](super::CallableSpec)'s note).
/// Argument parsers are tier-1 *stored* behavior objects (`Arc`-shared inside
/// [`ArgumentSpec`]s): immutable, `&self`, every per-use input arriving as arguments —
/// unlike the tier-2 construct-parser temporaries they drive internally.
pub trait ArgumentParser<L: Lang>: fmt::Debug + Send + Sync {
    /// Parse one argument at the context's current position.
    ///
    /// `cx.state` is the argument's own state — the caller has already stacked the
    /// [`ArgumentSpec::parsing_state_delta`] on the invocation's base state, so the
    /// whole extent (noise scan included) runs under it; the caller reverts
    /// structurally afterwards. `spec` is the spec being parsed against (its `name`
    /// serves diagnostics; its delta is already applied).
    ///
    /// Returns `Ok(Some(_))` with the region's staged nodes and content designation
    /// when the argument is provided; `Ok(None)` when it is absent — and **absent means
    /// nothing was consumed**: the reader is rewound past any probed noise (re-parsed
    /// as enclosing content) and speculatively staged nodes are simply never claimed
    /// (the builder drops them). Whether absence is an error is the parser's own
    /// policy: an optional argument stays silent, a mandatory one records its
    /// diagnostic (tolerant) or aborts (strict) at this detection site
    /// before reporting absent. There is deliberately no
    /// after-effect delta channel: an argument scopes no state beyond its own extent.
    fn parse_argument(
        &self,
        cx: &mut ParseContext<'_, '_, L>,
        spec: &ArgumentSpec<L>,
    ) -> ConstructParserResult<L, Option<ParsedArgumentNodes<L>>>;

    /// Can this argument be satisfied consuming nothing — is reporting it absent a
    /// *valid* outcome rather than a diagnosed recovery? An optional group or `*`
    /// marker: yes; a mandatory group or expression: no (slots
    /// session; pylatexenc's `LatexParserBase.contents_can_be_empty`).
    ///
    /// Consulted by [`CallableSpec::requires_content`](super::CallableSpec::requires_content)'s
    /// default, which the expression position's guard uses to decide whether a callable
    /// may appear *bare* as a single-token argument (`\frac\mymacro 2` is fine when
    /// `\mymacro`'s arguments can all match empty). Defaults to `true` (the pylatexenc
    /// base-class polarity): a parser that *requires* syntax must override to `false` —
    /// leaving the default costs its callables the guard diagnostic (they dispatch
    /// fully, parsing their arguments greedily in expression position), whereas a
    /// wrongly-`false` answer would spuriously diagnose valid input.
    fn can_match_empty(&self) -> bool {
        true
    }
}

/// One argument accepted by a callable (pylatexenc's `LatexArgumentSpec`).
pub struct ArgumentSpec<L: Lang> {
    /// The parser recognizing and parsing this argument ([`ArgumentParser`]).
    pub parser: Arc<dyn ArgumentParser<L>>,
    /// Optional name for by-name access to parsed arguments (pylatexenc's `argname`):
    /// more future-proof than positions — inserting an optional argument renumbers
    /// positions, names stay valid.
    pub name: Option<Box<str>>,
    /// Parse this argument under a modified state (pylatexenc's `parsing_state_delta`):
    /// `\text{…}` leaves math mode for its argument, `\href`'s URL argument disables
    /// specials. Applied via `derived()` around the argument's extent and reverted
    /// structurally.
    pub parsing_state_delta: Option<ParsingStateDelta<L>>,
}

impl<L: Lang> ArgumentSpec<L> {
    /// An unnamed argument with the given parser and no state delta.
    pub fn new(parser: Arc<dyn ArgumentParser<L>>) -> ArgumentSpec<L> {
        ArgumentSpec { parser, name: None, parsing_state_delta: None }
    }

    /// Attach a name for by-name access.
    pub fn named(mut self, name: impl Into<Box<str>>) -> ArgumentSpec<L> {
        self.name = Some(name.into());
        self
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

