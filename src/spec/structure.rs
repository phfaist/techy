//! Argument and slot specs: the declarative description of a callable's invocation shape.
//!
//! **Modeled on pylatexenc's `LatexArgumentSpec`** (decided July 2026, replacing the
//! Phase 4 `ArgumentKind` skeleton): an argument *is* a parser, optionally named, and may
//! request a modified parsing state for its own extent. The standard delimited forms stay
//! introspectable *data* ([`ArgumentParserSpec`]'s closed variants — declarative
//! recomposition and terse spec construction), while [`ArgumentParserSpec::Custom`]
//! carries pylatexenc's mid-granularity extension point: chars-only arguments
//! (`\label{...}`), comma-separated lists (`\cite{a,b}`), verbatim arguments, FLM's
//! bespoke argument types — without taking over the whole invocation.
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

/// A custom argument parser: the behavior extension point of [`ArgumentParserSpec`],
/// pylatexenc's "any `LatexParserBase` instance as `LatexArgumentSpec.parser`".
///
/// **Phase 6 grows the actual parse entry point** (it needs `ParseContext`); until then
/// the trait reserves the slot so spec-facing types are final. An implementation parses
/// one argument region and stages its node (or reports the argument absent); the standard
/// invocation path records the result in the node's
/// [`ParsedArguments`](crate::node::ParsedArguments) like any other argument.
pub trait ArgumentParser<L: Lang>: fmt::Debug {}

/// How a single argument is parsed: the standard delimited forms as data, or a custom
/// parser (pylatexenc's `'{'` / `'['` / `'*'` shorthands and `parser=` escape hatch).
///
/// The starter inventory covers LaTeX's common forms; more data variants (embellishments,
/// char-delimited `r`/`d` forms, verbatim) arrive with their consumers (Phase 6/7).
pub enum ArgumentParserSpec<L: Lang> {
    /// A mandatory argument delimited by the given group type (LaTeX: `{…}`) — **or**,
    /// failing that, a single expression: one content character (`\frac12`) or one full
    /// nested invocation (`\frac1\alpha`); the LaTeX acceptance rule, pylatexenc's
    /// standard `'{'` argument (Phase 6 notes, Q3 Option A).
    Group {
        /// The group type delimiting the argument.
        group_type: L::GroupTypeId,
    },
    /// An argument delimited by the given group type, present only when its open
    /// delimiter is next after skippable whitespace (LaTeX: `[…]`).
    OptionalGroup {
        /// The group type delimiting the argument.
        group_type: L::GroupTypeId,
    },
    /// An optional literal marker (LaTeX: the `*` of starred variants). When present it
    /// parses as a `Chars` node holding the marker text (pylatexenc behavior); absence is
    /// recorded in the node's [`ParsedArguments`](crate::node::ParsedArguments).
    Marker {
        /// The literal marker text (`"*"` in LaTeX).
        marker: Box<str>,
    },
    /// A custom parser — see [`ArgumentParser`].
    Custom(Arc<dyn ArgumentParser<L>>),
}

/// One argument accepted by a callable (pylatexenc's `LatexArgumentSpec`).
pub struct ArgumentSpec<L: Lang> {
    /// How the argument is parsed ([`ArgumentParserSpec`]).
    pub parser: ArgumentParserSpec<L>,
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
    pub fn new(parser: ArgumentParserSpec<L>) -> ArgumentSpec<L> {
        ArgumentSpec { parser, name: None, parsing_state_delta: None }
    }

    /// A mandatory group-delimited argument (LaTeX `{…}`).
    pub fn group(group_type: L::GroupTypeId) -> ArgumentSpec<L> {
        ArgumentSpec::new(ArgumentParserSpec::Group { group_type })
    }

    /// An optional group-delimited argument (LaTeX `[…]`).
    pub fn optional_group(group_type: L::GroupTypeId) -> ArgumentSpec<L> {
        ArgumentSpec::new(ArgumentParserSpec::OptionalGroup { group_type })
    }

    /// An optional literal marker argument (LaTeX `*`).
    pub fn marker(marker: impl Into<Box<str>>) -> ArgumentSpec<L> {
        ArgumentSpec::new(ArgumentParserSpec::Marker { marker: marker.into() })
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
// bounded in `Lang`) and `Arc`s are stored. No `PartialEq`: a spec may carry a custom
// parser (behavior has no structural equality) and a state delta.

impl<L: Lang> Clone for ArgumentParserSpec<L> {
    fn clone(&self) -> Self {
        match self {
            ArgumentParserSpec::Group { group_type } => {
                ArgumentParserSpec::Group { group_type: *group_type }
            }
            ArgumentParserSpec::OptionalGroup { group_type } => {
                ArgumentParserSpec::OptionalGroup { group_type: *group_type }
            }
            ArgumentParserSpec::Marker { marker } => {
                ArgumentParserSpec::Marker { marker: marker.clone() }
            }
            ArgumentParserSpec::Custom(parser) => ArgumentParserSpec::Custom(Arc::clone(parser)),
        }
    }
}

impl<L: Lang> fmt::Debug for ArgumentParserSpec<L> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ArgumentParserSpec::Group { group_type } => {
                f.debug_struct("Group").field("group_type", group_type).finish()
            }
            ArgumentParserSpec::OptionalGroup { group_type } => {
                f.debug_struct("OptionalGroup").field("group_type", group_type).finish()
            }
            ArgumentParserSpec::Marker { marker } => {
                f.debug_struct("Marker").field("marker", marker).finish()
            }
            ArgumentParserSpec::Custom(parser) => f.debug_tuple("Custom").field(parser).finish(),
        }
    }
}

impl<L: Lang> Clone for ArgumentSpec<L> {
    fn clone(&self) -> Self {
        ArgumentSpec {
            parser: self.parser.clone(),
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
