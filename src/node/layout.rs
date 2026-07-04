//! [`ArgsLayout`] / [`SlotsLayout`]: the per-instance invocation layout of a `Callable`
//! node — which spec'd arguments are present, and where each region lives in the node's
//! children range.
//!
//! **Encoding: one node per region** (decided July 2026, Phase 5 design session; see
//! DESIGN_RATIONALE.md §3.5). A callable's children range holds one node per *present*
//! argument — the argument's natural node, typically a `Group` — followed by one `List`
//! node per slot. Layout entries map spec positions to child offsets; regions absent from
//! the source (unused optional arguments, star markers) have layout entries but no node.
//!
//! The layouts also record **per-instance syntax choices the spec doesn't determine**
//! (the level-2 recomposition requirement, ARCHITECTURE.md §nodes): today the spelling of
//! a matched marker ([`ArgLayout::Marker`]); matched-delimiter alternatives, verbatim
//! fences, and slot separators arrive with `ArgumentsParser`/`SlotsParser` (Phase 6). Such
//! syntax is stored *here*, as [`TextContent`], not in ext types — recomposability must
//! not depend on `Lang` cooperation.

use alloc::vec::Vec;

use crate::source::TextContent;

/// How one spec'd argument was matched, and where its node is.
///
/// Entries parallel the spec's `ArgumentStructureSpec`: `args_layout.args[i]` describes
/// the invocation of `spec.arguments().arguments[i]`.
#[derive(Clone, Debug)]
pub enum ArgLayout {
    /// The argument was not given (an unused optional argument). No child node.
    Absent,
    /// The argument is present as a node.
    Present {
        /// Offset into the callable node's children range.
        child: u32,
    },
    /// The argument is present as a literal marker (a star-like argument): it has
    /// content-free syntax and therefore no child node.
    Marker {
        /// The marker as written (level-2 recomposition reproduces, not guesses).
        text: TextContent,
    },
}

impl ArgLayout {
    /// Whether the argument was given (as a node or as a marker).
    pub fn is_present(&self) -> bool {
        !matches!(self, ArgLayout::Absent)
    }

    /// The child offset, when the argument is present as a node.
    pub fn child(&self) -> Option<u32> {
        match self {
            ArgLayout::Present { child } => Some(*child),
            _ => None,
        }
    }
}

/// The argument layout of one callable invocation: one [`ArgLayout`] per spec'd argument.
#[derive(Clone, Debug, Default)]
pub struct ArgsLayout {
    /// The per-argument entries, parallel to the spec's argument structure.
    pub args: Vec<ArgLayout>,
}

impl ArgsLayout {
    /// A layout with no arguments (matches the no-argument default spec).
    pub fn empty() -> ArgsLayout {
        ArgsLayout { args: Vec::new() }
    }

    /// The number of spec'd arguments (present or not).
    pub fn len(&self) -> usize {
        self.args.len()
    }

    /// Whether the layout has no argument entries.
    pub fn is_empty(&self) -> bool {
        self.args.is_empty()
    }

    /// The entry for spec-argument `i`.
    pub fn get(&self, i: usize) -> Option<&ArgLayout> {
        self.args.get(i)
    }
}

impl From<Vec<ArgLayout>> for ArgsLayout {
    fn from(args: Vec<ArgLayout>) -> ArgsLayout {
        ArgsLayout { args }
    }
}

/// Where one slot's content region lives. Slots always have a node (an empty body is an
/// empty `List` node — a region that *exists* with no content, unlike an absent optional
/// argument). Separator/terminator syntax records arrive with `SlotsParser` (Phase 6).
#[derive(Clone, Debug)]
pub struct SlotLayout {
    /// Offset into the callable node's children range (a `List` node).
    pub child: u32,
}

/// The slot layout of one callable invocation: one [`SlotLayout`] per spec'd slot.
#[derive(Clone, Debug, Default)]
pub struct SlotsLayout {
    /// The per-slot entries, parallel to the spec's slot structure.
    pub slots: Vec<SlotLayout>,
}

impl SlotsLayout {
    /// A layout with no slots (macro-shaped callables).
    pub fn empty() -> SlotsLayout {
        SlotsLayout { slots: Vec::new() }
    }

    /// The number of slots.
    pub fn len(&self) -> usize {
        self.slots.len()
    }

    /// Whether the layout has no slots.
    pub fn is_empty(&self) -> bool {
        self.slots.is_empty()
    }

    /// The entry for slot `i`.
    pub fn get(&self, i: usize) -> Option<&SlotLayout> {
        self.slots.get(i)
    }
}

impl From<Vec<SlotLayout>> for SlotsLayout {
    fn from(slots: Vec<SlotLayout>) -> SlotsLayout {
        SlotsLayout { slots }
    }
}
