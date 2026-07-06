//! [`ParsedArguments`] / [`ParsedSlots`]: the per-invocation record of a `Callable`
//! node — which spec'd arguments were provided, where each region's node lives in the
//! node's children range, and which spec each region was parsed against.
//!
//! **Modeled on pylatexenc's `ParsedArguments`** (decided July 2026, replacing the
//! Phase 5 `ArgsLayout`/`SlotsLayout` offset maps): pylatexenc keeps two parallel lists —
//! `argnlist` (one node or `None` per argument) and `arguments_spec_list` (the spec of
//! every argument, present or not). Here the two are zipped into one `Vec` of
//! [`ParsedArgument`] entries, each carrying its `Arc`'d [`ArgumentSpec`]: the record is
//! **self-describing** (a custom invocation parser may produce an argument structure the
//! callable spec didn't declare — `\newcommand`-alikes), and absent optionals keep their
//! spec, so by-name lookup can distinguish "not provided" from "no such argument".
//!
//! **Encoding: one node per region** (unchanged from the Phase 5 design session,
//! DESIGN_RATIONALE.md §3.5). A callable's children range holds one node per *provided*
//! argument — the argument's natural node, delimiters included: a `Group` for `{…}`/`[…]`
//! forms, the single-expression node for `\frac12`-style acceptance, a `Chars` node for
//! provided markers (`*`, pylatexenc behavior) — followed by one `List` node per slot.
//! Absent arguments have an entry but no node.
//!
//! Per-instance syntax the spec doesn't determine (the level-2 recomposition requirement,
//! ARCHITECTURE.md §nodes) is recorded here as [`TextContent`] — today the whitespace
//! skipped before each argument (`pre_space`); slot separator/terminator records arrive
//! with `SlotsParser` (Phase 6). Recomposability must not depend on `Lang` cooperation.
//!
//! Content-extraction conveniences (pylatexenc's `get_content_nodelist()` /
//! `get_content_as_chars()` / keyval helpers) are deliberately *computed accessors*, not
//! stored fields — the group node's children *are* the content, and stored copies would
//! diverge under transforms. They arrive as `NodeRef`-level views (Phase 7); extensions
//! that want to *cache* derived data per argument use the [`ext`](ParsedArgument::ext)
//! slot instead.

use alloc::sync::Arc;
use alloc::vec::Vec;
use core::fmt;

use crate::source::TextContent;
use crate::spec::ArgumentSpec;
use crate::spec::SlotSpec;
use crate::state::Lang;

use super::ArgumentExt;

/// One spec'd argument of one invocation: the spec it was parsed against, whether (and
/// where) it was provided, and per-instance records.
pub struct ParsedArgument<L: Lang> {
    /// The spec this argument was parsed against (pylatexenc's `arguments_spec_list`
    /// entry) — always present, so names and introspection work for absent optionals too.
    pub spec: Arc<ArgumentSpec<L>>,
    /// Offset into the callable node's children range of the argument's node (the entire
    /// argument, delimiters included). `None` = the argument was not provided
    /// (pylatexenc's `None` in `argnlist`).
    pub child: Option<u32>,
    /// Whitespace skipped before the argument (`\frac {a}`), reproduced verbatim by
    /// level-2 recomposition. Empty when absent or when nothing was skipped.
    pub pre_space: TextContent,
    /// Extension data attached to this argument (`Lang::NodeExts::ArgumentExt`) — e.g. a
    /// reference extension caching `{domain, key}` parsed out of the argument's content.
    pub ext: ArgumentExt<L>,
}

impl<L: Lang> ParsedArgument<L> {
    /// An argument parsed against `spec` whose node is at child offset `child`.
    pub fn provided(spec: Arc<ArgumentSpec<L>>, child: u32) -> ParsedArgument<L> {
        ParsedArgument { spec, child: Some(child), pre_space: TextContent::empty(), ext: Default::default() }
    }

    /// An argument parsed against `spec` that was not provided.
    pub fn absent(spec: Arc<ArgumentSpec<L>>) -> ParsedArgument<L> {
        ParsedArgument { spec, child: None, pre_space: TextContent::empty(), ext: Default::default() }
    }

    /// Record the whitespace skipped before the argument.
    pub fn with_pre_space(mut self, pre_space: TextContent) -> ParsedArgument<L> {
        self.pre_space = pre_space;
        self
    }

    /// Whether the argument was provided (pylatexenc's `was_provided()`).
    pub fn is_provided(&self) -> bool {
        self.child.is_some()
    }

    /// The argument's name, per its spec.
    pub fn name(&self) -> Option<&str> {
        self.spec.name.as_deref()
    }
}

/// The parsed arguments of one callable invocation: one [`ParsedArgument`] per spec'd
/// argument, in invocation order (pylatexenc's `ParsedArguments`).
pub struct ParsedArguments<L: Lang> {
    /// The per-argument entries.
    pub arguments: Vec<ParsedArgument<L>>,
}

impl<L: Lang> ParsedArguments<L> {
    /// A record with no arguments (matches the no-argument default spec).
    pub fn empty() -> ParsedArguments<L> {
        ParsedArguments { arguments: Vec::new() }
    }

    /// The number of spec'd arguments (provided or not).
    pub fn len(&self) -> usize {
        self.arguments.len()
    }

    /// Whether the record has no argument entries.
    pub fn is_empty(&self) -> bool {
        self.arguments.is_empty()
    }

    /// The entry of argument `i`.
    pub fn get(&self, i: usize) -> Option<&ParsedArgument<L>> {
        self.arguments.get(i)
    }

    /// The entry of the argument named `name` (a scan over the specs' names — argument
    /// counts are small, and the specs are the single source of truth for names).
    pub fn get_named(&self, name: &str) -> Option<&ParsedArgument<L>> {
        self.arguments.iter().find(|arg| arg.name() == Some(name))
    }

    /// The entries, in invocation order.
    pub fn iter(&self) -> impl Iterator<Item = &ParsedArgument<L>> {
        self.arguments.iter()
    }
}

impl<L: Lang> From<Vec<ParsedArgument<L>>> for ParsedArguments<L> {
    fn from(arguments: Vec<ParsedArgument<L>>) -> ParsedArguments<L> {
        ParsedArguments { arguments }
    }
}

/// One slot of one invocation. Slots always have a node (an empty body is an empty
/// `List` node — a region that *exists* with no content, unlike an absent optional
/// argument). Separator/terminator syntax records arrive with `SlotsParser` (Phase 6).
pub struct ParsedSlot<L: Lang> {
    /// The spec this slot was parsed against.
    pub spec: Arc<SlotSpec<L>>,
    /// Offset into the callable node's children range (a `List` node).
    pub child: u32,
}

impl<L: Lang> ParsedSlot<L> {
    /// A slot parsed against `spec` whose `List` node is at child offset `child`.
    pub fn new(spec: Arc<SlotSpec<L>>, child: u32) -> ParsedSlot<L> {
        ParsedSlot { spec, child }
    }

    /// The slot's name, per its spec.
    pub fn name(&self) -> Option<&str> {
        self.spec.name.as_deref()
    }
}

/// The parsed slots of one callable invocation: one [`ParsedSlot`] per spec'd slot, in
/// source order.
pub struct ParsedSlots<L: Lang> {
    /// The per-slot entries.
    pub slots: Vec<ParsedSlot<L>>,
}

impl<L: Lang> ParsedSlots<L> {
    /// A record with no slots (macro-shaped callables).
    pub fn empty() -> ParsedSlots<L> {
        ParsedSlots { slots: Vec::new() }
    }

    /// The number of slots.
    pub fn len(&self) -> usize {
        self.slots.len()
    }

    /// Whether the record has no slots.
    pub fn is_empty(&self) -> bool {
        self.slots.is_empty()
    }

    /// The entry of slot `i`.
    pub fn get(&self, i: usize) -> Option<&ParsedSlot<L>> {
        self.slots.get(i)
    }

    /// The entry of the slot named `name`.
    pub fn get_named(&self, name: &str) -> Option<&ParsedSlot<L>> {
        self.slots.iter().find(|slot| slot.name() == Some(name))
    }

    /// The entries, in source order.
    pub fn iter(&self) -> impl Iterator<Item = &ParsedSlot<L>> {
        self.slots.iter()
    }
}

impl<L: Lang> From<Vec<ParsedSlot<L>>> for ParsedSlots<L> {
    fn from(slots: Vec<ParsedSlot<L>>) -> ParsedSlots<L> {
        ParsedSlots { slots }
    }
}

// --- materialization (crate-internal; see NodeTree::materialize) ----------------------

impl<L: Lang> ParsedArguments<L> {
    pub(crate) fn materialized(&self, source_content: &str) -> ParsedArguments<L> {
        ParsedArguments {
            arguments: self
                .arguments
                .iter()
                .map(|arg| ParsedArgument {
                    spec: Arc::clone(&arg.spec),
                    child: arg.child,
                    pre_space: arg.pre_space.materialized(source_content),
                    ext: arg.ext.clone(),
                })
                .collect(),
        }
    }
}

// Manual impls: derives would demand `L:` bounds although only associated types (already
// bounded) and `Arc`s are stored.

impl<L: Lang> Clone for ParsedArgument<L> {
    fn clone(&self) -> Self {
        ParsedArgument {
            spec: Arc::clone(&self.spec),
            child: self.child,
            pre_space: self.pre_space.clone(),
            ext: self.ext.clone(),
        }
    }
}

impl<L: Lang> fmt::Debug for ParsedArgument<L> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ParsedArgument")
            .field("spec", &self.spec)
            .field("child", &self.child)
            .field("pre_space", &self.pre_space)
            .field("ext", &self.ext)
            .finish()
    }
}

impl<L: Lang> Clone for ParsedArguments<L> {
    fn clone(&self) -> Self {
        ParsedArguments { arguments: self.arguments.clone() }
    }
}

impl<L: Lang> fmt::Debug for ParsedArguments<L> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_list().entries(&self.arguments).finish()
    }
}

impl<L: Lang> Clone for ParsedSlot<L> {
    fn clone(&self) -> Self {
        ParsedSlot { spec: Arc::clone(&self.spec), child: self.child }
    }
}

impl<L: Lang> fmt::Debug for ParsedSlot<L> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ParsedSlot")
            .field("spec", &self.spec)
            .field("child", &self.child)
            .finish()
    }
}

impl<L: Lang> Clone for ParsedSlots<L> {
    fn clone(&self) -> Self {
        ParsedSlots { slots: self.slots.clone() }
    }
}

impl<L: Lang> fmt::Debug for ParsedSlots<L> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_list().entries(&self.slots).finish()
    }
}
