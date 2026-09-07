//! The per-invocation record of a `Callable` node: [`ParsedArguments`] and
//! [`ParsedSlots`].
//!
//! Together they record which declared arguments were provided, where each provided
//! argument's and each slot's nodes sit among the callable's children, which of those
//! nodes are its content, and — for arguments — which
//! [`ArgumentSpec`](crate::core::specs::ArgumentSpec) each was parsed against.
//!
//! # Modeled on pylatexenc's `ParsedArguments`
//!
//! pylatexenc keeps two parallel lists: `argnlist` (one node or `None` per argument)
//! and `arguments_spec_list` (the spec of every argument, present or not). Here the two
//! are zipped into one `Vec` of [`ParsedArgument`] entries, each holding its `Arc`'d
//! spec, which makes the record self-describing. A custom invocation parser may produce
//! an argument structure the callable spec never declared (`\newcommand`-alikes), and
//! an absent optional keeps its spec, so a lookup by name can tell "not provided" from
//! "no such argument".
//!
//! # Encoding: one child region per argument or slot
//!
//! A callable's children are the concatenation of one contiguous region per *provided*
//! argument, followed by one region per slot. A region holds the argument's full
//! syntactic extent in source order: leading noise (comment nodes and whitespace-only
//! `Chars` nodes — there is no `pre_space` field, since whitespace skipped before an
//! argument becomes a node like everywhere else), the syntax-bearing node or nodes (a
//! `Group` for the `{…}` and `[…]` forms, with the delimiters stored on the group; a
//! `Chars` node for `\frac 1 2` single tokens and provided `*` markers), and any
//! trailing per-instance syntax.
//!
//! An absent argument has an entry but no region: reporting an argument absent means
//! having consumed *nothing*, and noise scanned while looking for it is rewound and
//! re-parsed as enclosing content (see
//! [`ArgumentParser`](crate::core::constructs::ArgumentParser)). The callable's child
//! list is therefore the raw-syntax view, in which the child count does not match the
//! argument count; access by argument goes through these records.
//!
//! Each region also designates its content nodes — for `\textbf{abc}` the group's
//! children, braces excluded; for `\frac 1 2` the single `Chars` node; for
//! `[{arg with ]}]` the *inner* group's children. The parser designates the content at
//! parse time ([`ContentNodes`]) and it reads back as a plain node range. There is no
//! heuristic that unwraps a lone group (pylatexenc's `get_content_nodelist()` plus
//! `unwrap_double_group`), and, unlike pylatexenc's standard argument parsers, which
//! drop pre-argument comment nodes by default (`return_full_node_list=False`), noise is
//! kept — out of the way of the content.
//!
//! # Two-phase records
//!
//! A resolved region's ranges name positions in the flattened tree (the coordinate
//! system of `NodeData.children`), and those positions do not exist while parsers run:
//! a node's final index depends on parts of the tree that are still unparsed when it is
//! staged (see [`NodeTreeBuilder`](super::NodeTreeBuilder)'s module documentation). A
//! [`ChildRegion`] is therefore first *staged* by the parser — child offsets into the
//! callable's child list, plus a [`ContentNodes`] designation in `BuildId` terms — and
//! then *resolved in place* by
//! [`NodeTreeBuilder::finish`](super::NodeTreeBuilder::finish) into global node-index
//! ranges.
//!
//! The price of letting parsers construct `ParsedArguments` directly, instead of
//! driving a separate staging API, is that a record's phase is a runtime invariant the
//! type system cannot see. It stays contained: resolution happens at exactly one point,
//! a finished [`NodeTree`](super::NodeTree) can never hold a staged region, and the
//! accessors that read resolved coordinates panic on a staged one — reachable only by
//! reading back records one built oneself and never finished.
//!
//! Content-extraction conveniences beyond the stored ranges (keyval helpers, chars
//! flattening) stay *computed* views; an extension that wants to cache derived data per
//! argument uses the [`ext`](ParsedArgument::ext) slot instead.

use alloc::boxed::Box;
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::fmt;
use core::ops::Range;

use crate::spec::ArgumentSpec;
use crate::state::Lang;

use super::builder::BuildId;
use super::tree::{NodeId, TreeTag};
use super::{ArgumentExt, SlotExt};

/// A parser's designation of a region's content nodes, in staging coordinates.
///
/// Both forms name a contiguous run of one node's children by construction, so
/// contiguity in the flattened tree needs no checking, and an empty sub-range stays
/// anchored where it was designated: the content of `\m{}` is empty *inside the
/// group*. [`ChildRegion::new`] takes one of these; the finished tree reports the
/// result through [`ChildRegion::content_range`] and
/// [`ChildRegion::content_parent`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ContentNodes {
    /// Elements `i..j` of the region's own node list, `0` being the region's first
    /// node: content that sits directly among the callable's children.
    ///
    /// This is the shape of a `\frac 1 2` single token, of a provided `*` marker
    /// (which counts as content, matching pylatexenc), and of multi-node content from a
    /// custom parser. The shipped slot-level example is the `attached` slot of
    /// [`input_macro_spec`](crate::latexlike::input_macro_spec); such a slot has no
    /// wrapper node, so
    /// [`NodeRef::slot_content_parent`](super::NodeRef::slot_content_parent) answers
    /// `None` for it.
    InRegion(Range<u32>),
    /// Children `i..j` of the named staged node: content *inside* one of the region's
    /// nodes.
    ///
    /// This is the shape of a `{…}` argument's group children, of the inner group's
    /// children in `[{arg with ]}]`, and of a slot body `List`'s children. The named
    /// node must be one of the region's nodes or a descendant of one, which is checked
    /// when the region is resolved and the layout exists.
    InChildrenOf(BuildId, Range<u32>),
}

/// The children of one provided argument or one slot, and which of them are its
/// content.
///
/// A region covers its argument's or slot's full syntactic extent in source order —
/// leading noise, the syntax-bearing nodes, any trailing syntax — and designates the
/// contiguous run of nodes within it that is the content.
///
/// Regions are two-phase. A parser builds one in staging coordinates
/// ([`new`](ChildRegion::new), [`single`](ChildRegion::single)), and
/// [`NodeTreeBuilder::finish`](super::NodeTreeBuilder::finish) resolves it into global
/// node-index ranges of the finished tree. [`children`](ChildRegion::children),
/// [`content_range`](ChildRegion::content_range) and
/// [`content_parent`](ChildRegion::content_parent) read the resolved form and panic on
/// a staged region; [`staged`](ChildRegion::staged) reads the staged form and answers
/// `None` on a resolved one.
///
/// Every region read from a finished tree is resolved — the builder checks that staged
/// records are staged when the callable is added — so that panic is reachable only by
/// reading back a region one built oneself and never finished. These are among the
/// crate's few deliberate panics (see the [Panics list](techy::guide::panics)).
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
        /// The resolving tree's layout tag, stamped into the [`NodeId`]s this
        /// record mints (see [`TreeTag`]).
        tree_tag: TreeTag,
    },
}

impl ChildRegion {
    /// A staged region: `children` is the offset range of the region's nodes within
    /// the callable's child list, and `content` designates which of them are the
    /// content.
    pub fn new(children: Range<u32>, content: ContentNodes) -> ChildRegion {
        ChildRegion { state: RegionState::Staged { children, content } }
    }

    /// A staged region of one node that is itself the content — the shape of
    /// `\frac 1 2` single-token arguments and provided `*` markers.
    ///
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

    /// The region's nodes — the argument's or slot's full syntactic extent, in source
    /// order — as a global node-index range of the finished tree.
    ///
    /// Turn the range into nodes with
    /// [`NodeTree::nodes_in`](super::NodeTree::nodes_in), or reach the same nodes
    /// directly from the callable with
    /// [`NodeRef::argument_nodes`](super::NodeRef::argument_nodes).
    ///
    /// # Panics
    ///
    /// Panics on a staged region (see [`ChildRegion`]); guard with
    /// [`is_resolved`](ChildRegion::is_resolved) or read the staged form via
    /// [`staged`](ChildRegion::staged).
    pub fn children(&self) -> Range<u32> {
        self.resolved().0.clone()
    }

    /// The region's designated content nodes, as a global node-index range of the
    /// finished tree.
    ///
    /// The range is read as a plain slice, with no unwrapping heuristics. It may be
    /// empty — `\m{}` designates no content node — in which case
    /// [`content_parent`](ChildRegion::content_parent) still says where that empty
    /// content sits.
    ///
    /// # Panics
    ///
    /// Panics on a staged region (see [`ChildRegion`]); guard with
    /// [`is_resolved`](ChildRegion::is_resolved) or read the staged form via
    /// [`staged`](ChildRegion::staged).
    pub fn content_range(&self) -> Range<u32> {
        self.resolved().1.clone()
    }

    /// The node whose child list contains
    /// [`content_range`](ChildRegion::content_range): the argument's `Group`, the
    /// slot's body `List`, or the callable itself when the content sits directly among
    /// the callable's children.
    ///
    /// This is what answers "which group node is this argument?" and what anchors an
    /// empty content range (`\m{}`).
    ///
    /// # Panics
    ///
    /// Panics on a staged region (see [`ChildRegion`]); guard with
    /// [`is_resolved`](ChildRegion::is_resolved) or read the staged form via
    /// [`staged`](ChildRegion::staged).
    pub fn content_parent(&self) -> NodeId {
        let (_, _, content_parent, tree_tag) = self.resolved();
        NodeId::new(content_parent, tree_tag)
    }

    fn resolved(&self) -> (&Range<u32>, &Range<u32>, u32, TreeTag) {
        match &self.state {
            RegionState::Resolved { children, content, content_parent, tree_tag } => {
                (children, content, *content_parent, *tree_tag)
            }
            RegionState::Staged { .. } => panic!(
                "child region still staged: node-index ranges are minted by \
                 NodeTreeBuilder::finish() (two-phase record contract, node::arguments docs)"
            ),
        }
    }

    /// The staged form, if the region has not been resolved yet: the `children` offset
    /// range into the callable's child list and the [`ContentNodes`] designation,
    /// exactly as given to [`new`](ChildRegion::new) or
    /// [`single`](ChildRegion::single).
    ///
    /// Answers `None` on a resolved region, which includes every region read from a
    /// finished tree. This is the non-panicking companion of
    /// [`children`](ChildRegion::children),
    /// [`content_range`](ChildRegion::content_range) and
    /// [`content_parent`](ChildRegion::content_parent).
    pub fn staged(&self) -> Option<(&Range<u32>, &ContentNodes)> {
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
        tree_tag: TreeTag,
    ) {
        self.state = RegionState::Resolved { children, content, content_parent, tree_tag };
    }
}

/// One declared argument of one invocation: the spec it was parsed against, whether
/// and where it was provided, and per-argument ext data.
///
/// Entries exist for absent optionals too, so [`is_provided`](ParsedArgument::is_provided)
/// is what distinguishes an argument that was left out from one that was given.
pub struct ParsedArgument<L: Lang> {
    /// The spec this argument was parsed against (pylatexenc's `arguments_spec_list`
    /// entry) — always present, so names and introspection work for absent optionals too.
    pub spec: Arc<ArgumentSpec<L>>,
    /// The argument's child region, or `None` when the argument was not provided
    /// (pylatexenc's `None` in `argnlist`).
    ///
    /// An absent argument consumed nothing at all, not even noise. A provided argument
    /// always has a region, but that region's content may be empty: `\m{}` is provided
    /// with empty content.
    pub region: Option<ChildRegion>,
    /// Extension data attached to this argument (`Lang::NodeExts::ArgumentExt`) — for
    /// example a reference extension caching `{domain, key}` parsed out of the
    /// argument's content.
    ///
    /// The [`ArgumentParser`](crate::core::constructs::ArgumentParser) that provided
    /// the argument mints it and returns it on its
    /// [`ParsedArgumentNodes`](crate::core::constructs::ParsedArgumentNodes) output, so
    /// this is `Some` exactly when the argument was provided: an absent argument was
    /// never parsed, and nobody holds the knowledge to mint instance data for it
    /// (extension data is only ever populated at creation, see
    /// [`NodeExtTypes`](crate::core::NodeExtTypes)).
    pub ext: Option<ArgumentExt<L>>,
}

impl<L: Lang> ParsedArgument<L> {
    /// An argument parsed against `spec` occupying `region`, with the ext its parser
    /// minted (`()` for no-ext languages).
    pub fn provided(
        spec: Arc<ArgumentSpec<L>>,
        region: ChildRegion,
        ext: ArgumentExt<L>,
    ) -> ParsedArgument<L> {
        ParsedArgument { spec, region: Some(region), ext: Some(ext) }
    }

    /// An argument parsed against `spec` that was not provided. An absent argument has
    /// no ext (see [`ext`](ParsedArgument::ext)).
    pub fn absent(spec: Arc<ArgumentSpec<L>>) -> ParsedArgument<L> {
        ParsedArgument { spec, region: None, ext: None }
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

/// The parsed arguments of one callable invocation: one [`ParsedArgument`] per
/// declared argument, in invocation order (pylatexenc's `ParsedArguments`).
///
/// Read the record of a parsed node with
/// [`NodeRef::arguments`](super::NodeRef::arguments); to go straight to an argument's
/// nodes, use [`NodeRef::argument_nodes`](super::NodeRef::argument_nodes) or
/// [`NodeRef::argument_content_nodes`](super::NodeRef::argument_content_nodes) and
/// their by-name companions.
pub struct ParsedArguments<L: Lang> {
    /// The per-argument entries.
    pub arguments: Vec<ParsedArgument<L>>,
}

impl<L: Lang> ParsedArguments<L> {
    /// A record over the given per-argument entries, in invocation order.
    pub fn new(arguments: Vec<ParsedArgument<L>>) -> ParsedArguments<L> {
        ParsedArguments { arguments }
    }

    /// A record with no arguments (matches the no-argument default spec).
    pub fn empty() -> ParsedArguments<L> {
        ParsedArguments { arguments: Vec::new() }
    }

    /// The number of declared arguments, provided or not.
    pub fn len(&self) -> usize {
        self.arguments.len()
    }

    /// Whether the record has no argument entries.
    pub fn is_empty(&self) -> bool {
        self.arguments.is_empty()
    }

    /// The entry of argument `i`, or `None` if the invocation declares no such
    /// argument. An entry that exists may still be absent — see
    /// [`ParsedArgument::is_provided`].
    pub fn get(&self, i: usize) -> Option<&ParsedArgument<L>> {
        self.arguments.get(i)
    }

    /// The entry of the argument named `name`, per its spec.
    ///
    /// `None` means only "no argument of that name": every declared argument has an
    /// entry, absent ones included, so a hit still needs
    /// [`ParsedArgument::is_provided`] to tell whether the argument was given. The
    /// [`NodeRef`](super::NodeRef) accessors ending in `_named` report the miss as a
    /// [`NamedAccessError`](super::NamedAccessError) instead, which catches misspelled
    /// names.
    ///
    /// The lookup scans the entries' spec names; argument counts are small, and the
    /// specs are the single source of truth for names.
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

/// How a slot's content relates to its callable's source bytes — the recorded role of
/// one [`ParsedSlot`], declared by the parser that minted the record.
///
/// - [`Content`](SlotRole::Content) — constitutive: the node's meaning is incomplete
///   without it (an environment's body). The parent's source bytes cover it like they
///   cover any other child region.
/// - [`Attached`](SlotRole::Attached) — derived, and reconstructible from the
///   invocation itself. The example is `\input`'s resolved content, where the
///   invocation text *is* the recomposition. An attached slot is excluded from the
///   parent's byte accounting, because its children are located in their own source;
///   declaring the role is what replaces inferring it from a change of source.
/// - [`Hidden`](SlotRole::Hidden) — a framework- or callable-defined attachment that
///   the core ignores: no recomposition and no byte accounting, and nothing more. It
///   does not hide the slot from readers — readers, extraction helpers, and structural
///   walks treat every slot alike whatever its role, and debug output shows what is
///   really there. Any further meaning comes from the slot's name and the callable's
///   spec.
///
/// The enum is deliberately exhaustive rather than `#[non_exhaustive]`: consumers match
/// on roles in validators, recomposition strategies, and mappings to foreign-language
/// bindings, and a fourth role would change byte-accounting semantics — which must be a
/// conscious breaking change, not a silently ignored variant.
///
/// Whether a slot is *the body* is a separate axis, marked on the slot's ext through
/// [`BodySlotExt`]. A body slot is usually [`Content`](SlotRole::Content), but the two
/// are recorded independently.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub enum SlotRole {
    /// Constitutive content (the conceptual default): discarding it loses meaning.
    #[default]
    Content,
    /// Derived from the invocation itself; excluded from the parent's byte-tiling.
    Attached,
    /// Framework-defined; the core neither recomposes nor byte-accounts it, while
    /// readers treat it like any other slot (see the type documentation).
    Hidden,
}

/// Marks a slot ext type as able to designate its slot as **the body**.
///
/// This is the trait behind [`NodeRef::body`](super::NodeRef::body), and the mechanism
/// that lets code generic over the language mint a body slot's ext: the preset's
/// environment machinery calls [`make_body`](BodySlotExt::make_body) and never names a
/// concrete ext type.
///
/// Implementations must satisfy `Self::make_body().is_body() == true`.
///
/// A framework that replaces the ext bundle implements this trait on its own
/// [`SlotExt`](crate::core::NodeExtTypes::SlotExt), and every preset mechanism keeps
/// working. The implementation for `()` marks *every* slot as the body: a language
/// without ext data records no marking, so [`body()`](super::NodeRef::body) falls back
/// to the first slot.
pub trait BodySlotExt {
    /// Does this ext designate its slot as the body?
    fn is_body(&self) -> bool;

    /// Mint the ext of a body slot ([`is_body`](BodySlotExt::is_body) reports `true`
    /// on the result).
    fn make_body() -> Self;
}

/// The no-ext degenerate: every slot reports body, so
/// [`body()`](super::NodeRef::body) selects the first slot.
impl BodySlotExt for () {
    fn is_body(&self) -> bool {
        true
    }

    fn make_body() -> Self {}
}

/// One content region ("slot") of one invocation — an environment's body, and other
/// content a callable's own parser reads.
///
/// A slot always has a region, unlike an optional argument, which may be absent; the
/// region's content can still be empty. For the standard environment shape the region
/// holds the body `List` node, whose children are the content.
///
/// Slots are record-level vocabulary only: nothing declares a slot on the spec side.
/// The invocation parser that reads a callable's body (the composition the spec's
/// `make_invocation_parser` returns) mints these records directly, with whatever
/// parsers it drives internally. Self-description therefore means storing the `name`
/// on the record itself — a deliberate asymmetry with [`ParsedArgument`], which points
/// at its `Arc<ArgumentSpec>`: an argument spec holds a parser, a name and a state
/// delta worth pointing at, while a slot record has no spec-side counterpart.
pub struct ParsedSlot<L: Lang> {
    /// Optional name for by-name access (an environment's `"body"`; a fence-block
    /// multi-slot construct may name several). Owned — slots are few per node.
    pub name: Option<Box<str>>,
    /// The slot's child region, always present — a slot is never "absent" the way an
    /// optional argument can be, though its content may be empty.
    pub region: ChildRegion,
    /// The slot's [`SlotRole`]: how its content relates to the callable's source
    /// bytes ([`Content`](SlotRole::Content) for ordinary in-source regions).
    pub role: SlotRole,
    /// Extension data attached to this slot (`Lang::NodeExts::SlotExt`) — for example a
    /// tabular extension caching the cell structure derived from a body slot's content,
    /// or the latexlike preset's body marker ([`BodySlotExt`]).
    ///
    /// The invocation composition that mints the record mints this too; there is no
    /// default value, since extension data is only ever populated at creation (see
    /// [`NodeExtTypes`](crate::core::NodeExtTypes)).
    pub ext: SlotExt<L>,
}

impl<L: Lang> ParsedSlot<L> {
    /// A named slot occupying `region`.
    ///
    /// Naming a slot is the encouraged spelling, and the parameters are payload-first,
    /// following [`ArgumentSpec::new`](crate::core::specs::ArgumentSpec::new).
    pub fn new(
        region: ChildRegion,
        name: impl Into<Box<str>>,
        role: SlotRole,
        ext: SlotExt<L>,
    ) -> ParsedSlot<L> {
        ParsedSlot { name: Some(name.into()), region, role, ext }
    }

    /// An unnamed slot occupying `region` — the deliberately longer spelling, since
    /// [`new`](ParsedSlot::new) names the slot.
    pub fn new_unnamed(region: ChildRegion, role: SlotRole, ext: SlotExt<L>) -> ParsedSlot<L> {
        ParsedSlot { name: None, region, role, ext }
    }

    /// The slot's name, if it has one — what [`ParsedSlots::get_named`] matches
    /// against (the counterpart of [`ParsedArgument::name`]).
    pub fn name(&self) -> Option<&str> {
        self.name.as_deref()
    }
}

/// The parsed slots of one callable invocation: one [`ParsedSlot`] per content region,
/// in source order.
///
/// Read the record of a parsed node with [`NodeRef::slots`](super::NodeRef::slots); to
/// go straight to a slot's content, use
/// [`NodeRef::slot_content_nodes`](super::NodeRef::slot_content_nodes), its by-name
/// companion, or [`NodeRef::body`](super::NodeRef::body) for the body slot.
pub struct ParsedSlots<L: Lang> {
    /// The per-slot entries.
    pub slots: Vec<ParsedSlot<L>>,
}

impl<L: Lang> ParsedSlots<L> {
    /// A record over the given per-slot entries, in source order.
    pub fn new(slots: Vec<ParsedSlot<L>>) -> ParsedSlots<L> {
        ParsedSlots { slots }
    }

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

    /// The entry of slot `i`, or `None` if the invocation recorded no such slot.
    pub fn get(&self, i: usize) -> Option<&ParsedSlot<L>> {
        self.slots.get(i)
    }

    /// The entry of the slot named `name`.
    ///
    /// `None` means only "no slot of that name" — every recorded slot has content, even
    /// if that content is empty. The [`NodeRef`](super::NodeRef) accessor ending in
    /// `_named` reports the miss as a
    /// [`NamedAccessError`](super::NamedAccessError) instead, which catches misspelled
    /// names.
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
            role: self.role,
            ext: self.ext.clone(),
        }
    }
}

impl<L: Lang> fmt::Debug for ParsedSlot<L> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ParsedSlot")
            .field("name", &self.name)
            .field("region", &self.region)
            .field("role", &self.role)
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
