//! Argument and slot specs: the declarative description of a callable's invocation shape.
//!
//! **Modeled on pylatexenc's `LatexArgumentSpec`** (decided July 2026, replacing the
//! Phase 4 `ArgumentKind` skeleton): an argument *is* a parser, optionally named, and may
//! request a modified parsing state for its own extent. **Every** argument routes to an
//! [`ArgumentParser`] object (revised July 2026, dropping the earlier closed data
//! variants): the core cannot know a language's argument forms — which group class a
//! `{…}` argument uses, whether `[…]` is a group rule of the current state or a
//! momentarily-minted one — so the standard parsers (delimited group, optional group,
//! literal marker, chars-only, comma-lists, verbatim, …) are provided by the preset as
//! `ArgumentParser` implementations, exactly pylatexenc's resolution of the `'{'` / `'['`
//! / `'*'` shorthands into standard parser instances.
//!
//! The slot separator/terminator machinery (including the invocation-name back-reference
//! that makes `\end{align}` match the `align` that opened) grows in Phase 6, when
//! `ArgumentsParser`/`SlotsParser` make the requirements concrete (ARCHITECTURE.md §specs).
//!
//! **Arguments vs. slots.** Arguments *configure* an invocation (`\frac{a}{b}`,
//! `\item[label]`); slots contain *content regions* (an environment's body). A macro has
//! no slots; an environment has exactly one; a fence-block specials construct may have
//! several. The boundary is a spec-owned guideline, not core law — the machinery
//! underneath is shared.

use alloc::boxed::Box;
use alloc::sync::Arc;
use core::fmt;

use crate::state::{Lang, ParsingStateDelta};

/// An argument parser: how one argument of an invocation is recognized and parsed —
/// pylatexenc's "any `LatexParserBase` instance as `LatexArgumentSpec.parser`", and the
/// single argument-parsing interface (no privileged data forms in the core).
///
/// **Phase 6 grows the actual parse entry point** (it needs `ParseContext`); until then
/// the trait reserves the slot so spec-facing types are final. An implementation parses
/// one argument region and stages its nodes, designating the content nodes among them
/// ([`ChildRegion`](crate::node::ChildRegion) /
/// [`ContentNodes`](crate::node::ContentNodes)), or reports the argument absent; the
/// standard invocation path records the result in the node's
/// [`ParsedArguments`](crate::node::ParsedArguments) like any other argument. Parsers
/// needing group delimiters install the [`GroupRule`](crate::token::GroupRule)s they
/// want via a state delta for the argument's extent (an optional-argument parser
/// momentarily declaring `[`…`]`, a custom spec declaring `<`…`>`).
///
/// **Noise ownership** (decided July 2026, regions session; DESIGN_RATIONALE.md §3.5):
/// an argument parser owns its argument's *entire* region, leading noise included — it
/// scans whitespace and comments itself (typically via the standard noise-scan helper,
/// Phase 6) and stages them as ordinary nodes (comment nodes, whitespace-only `Chars`
/// nodes) ahead of the argument's syntax. The core never scans noise on a parser's
/// behalf: noise policy is inseparable from the argument's syntax (a verbatim argument
/// whose delimiter is the comment character must see the raw token stream), and the
/// scan must run under the argument's own parsing-state delta. Reporting the argument
/// **absent means consuming nothing**: the reader is rewound past any scanned noise —
/// it is re-parsed as enclosing content — and speculatively staged nodes are simply
/// never claimed (the builder drops them).
///
/// **Thread safety is part of the contract** (`Send + Sync` supertraits, decided July
/// 2026; see [`CallableSpec`](super::CallableSpec)'s note and DESIGN_RATIONALE.md).
pub trait ArgumentParser<L: Lang>: fmt::Debug + Send + Sync {}

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

/// One content region of a callable.
///
/// Separators and terminators — where terminator patterns may reference the invocation
/// name (`\end{align}` must match the `align` that opened; a `---` fence closes with
/// `---`) — arrive with `SlotsParser` (Phase 6).
pub struct SlotSpec<L: Lang> {
    /// Optional name for by-name access to parsed slots.
    pub name: Option<Box<str>>,
    /// Parse this slot's content under a modified state (pylatexenc's
    /// `make_body_parsing_state_delta`): verbatim environments, `\begin{align}` bodies in
    /// math mode, FLM's block-level environments. Applied via `derived()` around the
    /// slot's extent and reverted structurally.
    pub parsing_state_delta: Option<ParsingStateDelta<L>>,
}

impl<L: Lang> SlotSpec<L> {
    /// An unnamed slot with no state delta.
    pub fn new() -> SlotSpec<L> {
        SlotSpec { name: None, parsing_state_delta: None }
    }

    /// Attach a name for by-name access.
    pub fn named(mut self, name: impl Into<Box<str>>) -> SlotSpec<L> {
        self.name = Some(name.into());
        self
    }

    /// Parse the slot's content under the state derived through `delta`.
    pub fn with_state_delta(mut self, delta: ParsingStateDelta<L>) -> SlotSpec<L> {
        self.parsing_state_delta = Some(delta);
        self
    }
}

impl<L: Lang> Default for SlotSpec<L> {
    fn default() -> Self {
        SlotSpec::new()
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

impl<L: Lang> Clone for SlotSpec<L> {
    fn clone(&self) -> Self {
        SlotSpec {
            name: self.name.clone(),
            parsing_state_delta: self.parsing_state_delta.clone(),
        }
    }
}

impl<L: Lang> fmt::Debug for SlotSpec<L> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SlotSpec")
            .field("name", &self.name)
            .field("parsing_state_delta", &self.parsing_state_delta)
            .finish()
    }
}
