//! [`ParsedArguments`] / [`ParsedSlots`]: the per-invocation record of a `Callable`
//! node — which spec'd arguments were provided, where each argument's/slot's region and
//! content nodes live in the tree, and (for arguments) which spec each region was
//! parsed against.
//!
//! **Modeled on pylatexenc's `ParsedArguments`**: pylatexenc keeps two parallel lists —
//! `argnlist` (one node or `None` per argument) and `arguments_spec_list` (the spec of
//! every argument, present or not). Here the two are zipped into one `Vec` of
//! [`ParsedArgument`] entries, each carrying its `Arc`'d [`ArgumentSpec`]: the record is
//! **self-describing** (a custom invocation parser may produce an argument structure the
//! callable spec didn't declare — `\newcommand`-alikes), and absent optionals keep their
//! spec, so by-name lookup can distinguish "not provided" from "no such argument".
//!
//! # Encoding: one child *region* per argument/slot
//!
//! A callable's children range is the concatenation of one contiguous **region** per
//! *provided* argument, followed by one region per slot. A region holds the argument's
//! full syntactic extent in source order: leading noise (comment nodes and
//! whitespace-only `Chars` nodes — there is no `pre_space` field; whitespace skipped
//! before an argument is a node like everywhere else), the syntax-bearing node(s)
//! (a `Group` for `{…}`/`[…]` forms, delimiters stored on the group; a `Chars` node for
//! `\frac 1 2` single tokens and provided `*` markers), and any trailing per-instance
//! syntax. Absent arguments have an entry but no region — reporting an argument absent
//! means having consumed *nothing* (noise scanned while looking for it is rewound and
//! re-parsed as enclosing content; see [`ArgumentParser`](crate::spec::ArgumentParser)).
//! The callable's child list is thus the **raw-syntax view** (child count ≠ argument
//! count); semantic access goes through these records.
//!
//! Each region also designates its **content nodes** — for `\textbf{abc}` the group's
//! children (braces excluded), for `\frac 1 2` the single `Chars` node, for
//! `[{arg with ]}]` the *inner* group's children. Content is designated by the parser at
//! parse time ([`ContentNodes`]) and read back as a plain node range: there is no
//! lone-group unwrap heuristic (pylatexenc's `get_content_nodelist()` +
//! `unwrap_double_group` hack), and — unlike pylatexenc's standard argument parsers,
//! which drop pre-argument comment nodes by default (`return_full_node_list=False`) —
//! noise is kept, out of the way of content.
//!
//! # Two-phase records — the accepted "honest cost"
//!
//! Resolved region ranges name positions in the **flattened** tree (the coordinate
//! system of `NodeData.children`), and those positions don't exist while parsers run: a
//! node's final index depends on parts of the tree not yet parsed when it is staged
//! (see [`NodeTreeBuilder`](super::NodeTreeBuilder)'s module docs). So a [`ChildRegion`]
//! is **staged** by the parser (child offsets into the callable's child list, plus a
//! [`ContentNodes`] designation in `BuildId` terms) and **resolved in place** by
//! [`NodeTreeBuilder::finish`](super::NodeTreeBuilder::finish) into global node-index
//! ranges. The phase is a runtime invariant the type system can't see — the price paid
//! so parsers can construct `ParsedArguments` directly instead of driving a bespoke
//! staging API. It is contained: resolution happens at exactly one point (`finish`), a
//! finished [`NodeTree`](super::NodeTree) cannot hold staged regions, and the
//! resolved-only accessors panic on a staged region — reachable only by a parser
//! reading back records it itself built pre-finish, a caller bug under the builder's
//! panic-on-contract-violation policy.
//!
//! Content-extraction conveniences beyond the stored ranges (keyval helpers, chars
//! flattening) remain *computed* views; extensions that want to cache derived
//! data per argument use the [`ext`](ParsedArgument::ext) slot instead.

use alloc::boxed::Box;
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::fmt;
use core::ops::Range;

use crate::spec::ArgumentSpec;
use crate::state::Lang;

use super::builder::BuildId;
use super::tree::NodeId;
use super::{ArgumentExt, SlotExt};

/// Parser-side designation of a region's content nodes, in staging coordinates. Both
/// forms name a contiguous run of one node's children *by construction* — contiguity in
/// the flattened tree needs no checking — and an empty sub-range stays anchored (the
/// content of `\m{}` is empty *inside the group*).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ContentNodes {
    /// Elements `i..j` of the region's own node list (`0` = the region's first node):
    /// content sitting directly among the callable's children — a `\frac 1 2` single
    /// token, a provided `*` marker (which counts as content — pylatexenc parity), or
    /// multi-node content of a custom parser.
    InRegion(Range<u32>),
    /// Children `i..j` of the staged node: content *inside* a region node — a `{…}`
    /// argument's group children, the inner group's children of `[{arg with ]}]`, a
    /// slot body `List`'s children. The node must be one of the region's nodes or a
    /// descendant of one (checked at resolution, where the layout exists).
    InChildrenOf(BuildId, Range<u32>),
}

/// The child region of one provided argument or slot: the full syntactic extent
/// (noise + content + delimiting syntax) and its designated content nodes.
///
/// **Two-phase** (module docs): built by a parser in staging coordinates, resolved by
/// [`NodeTreeBuilder::finish`](super::NodeTreeBuilder::finish) into global node-index
/// ranges. The read accessors ([`children`](ChildRegion::children),
/// [`content_range`](ChildRegion::content_range),
/// [`content_parent`](ChildRegion::content_parent)) exist only on resolved regions and
/// panic on staged ones — a finished tree never contains staged regions (the builder
/// validates staged-ness at `add()`), so the panic is only reachable by reading a
/// region one built oneself and never staged (the approved indexing-style exception,
/// panic policy).
#[derive(Clone, Debug)]
pub struct ChildRegion {
    state: RegionState,
}

#[derive(Clone, Debug)]
enum RegionState {
    /// As built by a parser: `children` are offsets into the callable's child list;
    /// `content` designates the content nodes in staging coordinates.
    Staged { children: Range<u32>, content: ContentNodes },
    /// After `finish()`: global node-index ranges, plus the node whose child list
    /// contains the content range (the callable itself for region-level content).
    Resolved {
        children: Range<u32>,
        content: Range<u32>,
        content_parent: u32,
        /// Debug-only provenance tag of the resolving tree, stamped into the
        /// [`NodeId`]s this record mints (see `NodeTree`'s `tag` field).
        #[cfg(debug_assertions)]
        tree_tag: u32,
    },
}

impl ChildRegion {
    /// A staged region: `children` is the offset range of the region's nodes within the
    /// callable's child list; `content` designates the content nodes.
    pub fn new(children: Range<u32>, content: ContentNodes) -> ChildRegion {
        ChildRegion { state: RegionState::Staged { children, content } }
    }

    /// A staged single-node region whose one node is itself the content — the common
    /// shape of `\frac 1 2` single-token arguments and provided `*` markers.
    /// `child_offset` indexes the callable's child list.
    pub fn single(child_offset: u32) -> ChildRegion {
        ChildRegion::new(child_offset..child_offset + 1, ContentNodes::InRegion(0..1))
    }

    /// Whether [`NodeTreeBuilder::finish`](super::NodeTreeBuilder::finish) has resolved
    /// this region to node-index ranges. Always `true` on regions read from a finished
    /// tree.
    pub fn is_resolved(&self) -> bool {
        matches!(self.state, RegionState::Resolved { .. })
    }

    /// The region's nodes — the argument's/slot's full syntactic extent, in source
    /// order — as a global node-index range of the finished tree (resolve to nodes via
    /// [`NodeTree::nodes_in`](super::NodeTree::nodes_in)).
    ///
    /// # Panics
    ///
    /// Panics on a staged region (see [`ChildRegion`]).
    pub fn children(&self) -> Range<u32> {
        self.resolved().0.clone()
    }

    /// The region's designated content nodes, as a global node-index range of the
    /// finished tree. Reading it is a plain slice — no unwrap heuristics; possibly
    /// empty (anchored by [`content_parent`](ChildRegion::content_parent)).
    ///
    /// # Panics
    ///
    /// Panics on a staged region (see [`ChildRegion`]).
    pub fn content_range(&self) -> Range<u32> {
        self.resolved().1.clone()
    }

    /// The node whose child list contains [`content_range`](ChildRegion::content_range):
    /// the argument's `Group`, the slot's body `List` — or the callable itself for
    /// region-level content. Answers delimiter queries ("the group node of this
    /// argument") and anchors empty content ranges (`\m{}`).
    ///
    /// # Panics
    ///
    /// Panics on a staged region (see [`ChildRegion`]).
    pub fn content_parent(&self) -> NodeId {
        let index = self.resolved().2;
        NodeId::new(index, self.resolved_tree_tag())
    }

    fn resolved(&self) -> (&Range<u32>, &Range<u32>, u32) {
        match &self.state {
            RegionState::Resolved { children, content, content_parent, .. } => {
                (children, content, *content_parent)
            }
            RegionState::Staged { .. } => panic!(
                "child region still staged: node-index ranges are minted by \
                 NodeTreeBuilder::finish() (two-phase record contract, node::arguments docs)"
            ),
        }
    }

    /// The resolving tree's provenance tag (`0` in release builds; only called on
    /// resolved regions — `resolved()` has already panicked otherwise).
    fn resolved_tree_tag(&self) -> u32 {
        #[cfg(debug_assertions)]
        if let RegionState::Resolved { tree_tag, .. } = &self.state {
            return *tree_tag;
        }
        0
    }

    /// The staged form, if not yet resolved (builder-side validation).
    pub(crate) fn staged(&self) -> Option<(&Range<u32>, &ContentNodes)> {
        match &self.state {
            RegionState::Staged { children, content } => Some((children, content)),
            RegionState::Resolved { .. } => None,
        }
    }

    /// Flip to resolved (called exactly once, by `NodeTreeBuilder::finish`).
    pub(crate) fn resolve(
        &mut self,
        children: Range<u32>,
        content: Range<u32>,
        content_parent: u32,
        tree_tag: u32,
    ) {
        let _ = tree_tag;
        self.state = RegionState::Resolved {
            children,
            content,
            content_parent,
            #[cfg(debug_assertions)]
            tree_tag,
        };
    }
}

/// One spec'd argument of one invocation: the spec it was parsed against, whether (and
/// where) it was provided, and per-argument ext data.
pub struct ParsedArgument<L: Lang> {
    /// The spec this argument was parsed against (pylatexenc's `arguments_spec_list`
    /// entry) — always present, so names and introspection work for absent optionals too.
    pub spec: Arc<ArgumentSpec<L>>,
    /// The argument's child region. `None` = the argument was not provided (pylatexenc's
    /// `None` in `argnlist`); an absent argument consumed nothing, not even noise.
    pub region: Option<ChildRegion>,
    /// Extension data attached to this argument (`Lang::NodeExts::ArgumentExt`) — e.g. a
    /// reference extension caching `{domain, key}` parsed out of the argument's content.
    pub ext: ArgumentExt<L>,
}

impl<L: Lang> ParsedArgument<L> {
    /// An argument parsed against `spec` occupying `region`.
    pub fn provided(spec: Arc<ArgumentSpec<L>>, region: ChildRegion) -> ParsedArgument<L> {
        ParsedArgument { spec, region: Some(region), ext: Default::default() }
    }

    /// An argument parsed against `spec` that was not provided.
    pub fn absent(spec: Arc<ArgumentSpec<L>>) -> ParsedArgument<L> {
        ParsedArgument { spec, region: None, ext: Default::default() }
    }

    /// Whether the argument was provided (pylatexenc's `was_provided()`).
    pub fn is_provided(&self) -> bool {
        self.region.is_some()
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

/// One content region ("slot") of one invocation. Slots always have a region (a region
/// that *exists*, with possibly empty content — unlike an absent optional argument):
/// for the standard environment shape it holds the body `List` node, whose children are
/// the content.
///
/// Slots are pure **record-level** vocabulary: there
/// is no spec-side slot declaration — the invocation parser that reads a callable's
/// body (the spec's sanctioned `make_invocation_parser` composition) mints these
/// records directly, with whatever parsers it drives internally. Self-description
/// therefore means carrying the `name` on the record itself — a deliberate
/// asymmetry with [`ParsedArgument`], which points at its `Arc<ArgumentSpec>`: an
/// argument spec carries parser/name/delta worth pointing at; a slot record has no
/// spec-side counterpart.
pub struct ParsedSlot<L: Lang> {
    /// Optional name for by-name access (an environment's `"body"`; a fence-block
    /// multi-slot construct may name several). Owned — slots are few per node.
    pub name: Option<Box<str>>,
    /// The slot's child region.
    pub region: ChildRegion,
    /// Extension data attached to this slot (`Lang::NodeExts::SlotExt`) — e.g. a tabular
    /// extension caching the cell structure derived from a body slot's content.
    pub ext: SlotExt<L>,
}

impl<L: Lang> ParsedSlot<L> {
    /// An unnamed slot occupying `region`, with default ext.
    pub fn new(region: ChildRegion) -> ParsedSlot<L> {
        ParsedSlot { name: None, region, ext: Default::default() }
    }

    /// A named slot occupying `region`, with default ext.
    pub fn named(name: impl Into<Box<str>>, region: ChildRegion) -> ParsedSlot<L> {
        ParsedSlot { name: Some(name.into()), region, ext: Default::default() }
    }

    /// The slot's name ([`get_named`](ParsedSlots::get_named) symmetry with
    /// [`ParsedArgument::name`]).
    pub fn name(&self) -> Option<&str> {
        self.name.as_deref()
    }
}

/// The parsed slots of one callable invocation: one [`ParsedSlot`] per content region,
/// in source order.
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

// Manual impls: derives would demand `L:` bounds although only associated types (already
// bounded) and `Arc`s are stored.

impl<L: Lang> Clone for ParsedArgument<L> {
    fn clone(&self) -> Self {
        ParsedArgument {
            spec: Arc::clone(&self.spec),
            region: self.region.clone(),
            ext: self.ext.clone(),
        }
    }
}

impl<L: Lang> fmt::Debug for ParsedArgument<L> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ParsedArgument")
            .field("spec", &self.spec)
            .field("region", &self.region)
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
        ParsedSlot {
            name: self.name.clone(),
            region: self.region.clone(),
            ext: self.ext.clone(),
        }
    }
}

impl<L: Lang> fmt::Debug for ParsedSlot<L> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ParsedSlot")
            .field("name", &self.name)
            .field("region", &self.region)
            .field("ext", &self.ext)
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
