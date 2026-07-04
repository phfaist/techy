//! Argument and slot structure specs: the declarative description of a callable's
//! invocation shape.
//!
//! **Phase 4 skeleton.** These types exist so [`CallableSpec`](super::CallableSpec) can
//! expose its structure and libraries can hold real specs; the set of argument kinds and
//! the slot separator/terminator machinery (including the invocation-name back-reference
//! that makes `\end{align}` match the `align` that opened) grow in Phase 6, when
//! `ArgumentsParser`/`SlotsParser` make the requirements concrete (ARCHITECTURE.md §specs;
//! decision recorded in DESIGN_RATIONALE.md §3.4).
//!
//! **Arguments vs. slots.** Arguments *configure* an invocation (`\frac{a}{b}`,
//! `\item[label]`); slots contain *content regions* (an environment's body). A macro has
//! no slots; an environment has exactly one; a fence-block specials construct may have
//! several. The boundary is a spec-owned guideline, not core law — the machinery
//! underneath is shared.

use alloc::boxed::Box;
use alloc::vec::Vec;

use crate::token::GroupTypeId;

/// The kind of a single argument: how it is delimited and whether it may be absent.
///
/// Starter set (Phase 4); more kinds (verbatim-delimited, single-token forms, …) arrive
/// with `ArgumentsParser` in Phase 6, which also pins down the exact acceptance semantics
/// (e.g. the LaTeX rule that a mandatory `{…}` argument also accepts a single token).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ArgumentKind {
    /// A required argument delimited by the given group type (LaTeX: `{…}`).
    Mandatory {
        /// The group type delimiting the argument.
        group_type: GroupTypeId,
    },
    /// An argument delimited by the given group type that may be absent (LaTeX: `[…]`).
    Optional {
        /// The group type delimiting the argument.
        group_type: GroupTypeId,
    },
    /// An optional literal marker (LaTeX: the `*` of starred variants). Presence is
    /// per-instance data, recorded in the node's `ArgsLayout` (Phase 5).
    Star {
        /// The literal marker text (`"*"` in LaTeX).
        marker: Box<str>,
    },
}

/// One argument in a callable's argument structure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArgumentSpec {
    /// How the argument is delimited ([`ArgumentKind`]).
    pub kind: ArgumentKind,
    /// Optional name for by-name access to parsed arguments (pylatexenc's `argname`).
    pub name: Option<Box<str>>,
}

impl ArgumentSpec {
    /// An unnamed argument of the given kind.
    pub fn new(kind: ArgumentKind) -> ArgumentSpec {
        ArgumentSpec { kind, name: None }
    }

    /// Attach a name for by-name access.
    pub fn named(mut self, name: impl Into<Box<str>>) -> ArgumentSpec {
        self.name = Some(name.into());
        self
    }
}

/// The argument structure of a callable: an ordered list of [`ArgumentSpec`]s.
///
/// The default is *no arguments* — the neutral element, suitable for simple callables
/// (`~`-style specials) and unknown-callable fallback specs.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ArgumentStructureSpec {
    /// The arguments, in invocation order.
    pub arguments: Vec<ArgumentSpec>,
}

/// One content region of a callable.
///
/// **Phase 4 skeleton:** identification only. Separators and terminators — where
/// terminator patterns may reference the invocation name (`\end{align}` must match the
/// `align` that opened; a `---` fence closes with `---`) — arrive with `SlotsParser`
/// (Phase 6).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SlotSpec {
    /// Optional name for by-name access to parsed slots.
    pub name: Option<Box<str>>,
}

/// The slot structure of a callable: an ordered list of [`SlotSpec`]s.
///
/// The default is *no slots* (macro-shaped); an environment-shaped callable has exactly
/// one (its body).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SlotStructureSpec {
    /// The content regions, in source order.
    pub slots: Vec<SlotSpec>,
}
