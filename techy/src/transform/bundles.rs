//! One restaged argument or slot of a callable: [`RestagedArgument`] and
//! [`RestagedSlot`].
//!
//! These are what the region operations of
//! [`RestageContext`](super::RestageContext) return and what
//! [`restage_invocation`](super::RestageContext::restage_invocation) takes to
//! assemble the new callable.
//!
//! A bundle is the output-side counterpart of a
//! [`ParsedArgument`](crate::core::node::ParsedArgument) or
//! [`ParsedSlot`](crate::core::node::ParsedSlot) record, with the same parts —
//! spec, name, presence, role, the nodes, a [`ContentNodes`] content designation,
//! and the record ext. The difference is what the numbers mean: node offsets are
//! counted within the bundle's own node list rather than within a finished tree,
//! and an `InChildrenOf` designation names a staged [`BuildId`].
//!
//! The fields are private, but a bundle can be both built and read. The
//! constructors take every part, so a pass can build by hand what no ready-made
//! operation produces, and the accessors return every part a constructor takes, so
//! a pass can rebuild a bundle it was given with one part changed — a node
//! prepended to a slot's nodes, say.

use core::fmt;

use alloc::boxed::Box;
use alloc::sync::Arc;
use alloc::vec::Vec;

use crate::node::{ArgumentExt, BuildId, ContentNodes, SlotExt, SlotRole};
use crate::spec::ArgumentSpec;
use crate::state::Lang;

/// One restaged argument of a callable: the [`ArgumentSpec`] it stays recorded
/// against and, when the argument was provided, its restaged nodes.
///
/// Produced by [`restage_argument`](super::RestageContext::restage_argument) and
/// its siblings, or built directly with
/// [`provided`](RestagedArgument::provided) / [`absent`](RestagedArgument::absent);
/// consumed by
/// [`restage_invocation`](super::RestageContext::restage_invocation).
///
/// An absent argument and an empty one mean different things, and the distinction
/// is preserved here. A provided argument with no nodes is a region that was
/// emptied — [`provided`](RestagedArgument::provided) with an empty node list.
/// True absence is [`absent`](RestagedArgument::absent): the argument consumed
/// nothing, and records neither nodes nor an ext.
pub struct RestagedArgument<L: Lang> {
    pub(super) spec: Arc<ArgumentSpec<L>>,
    pub(super) provided: Option<ProvidedRegion<L>>,
}

/// The nodes and record data of a provided [`RestagedArgument`] (internal).
pub(super) struct ProvidedRegion<L: Lang> {
    pub(super) nodes: Vec<BuildId>,
    pub(super) content: ContentNodes,
    /// `Some` on every coherently built record, since
    /// [`provided`](RestagedArgument::provided) requires the ext. It is `None`
    /// only when [`restage_argument`](super::RestageContext::restage_argument)
    /// read an input record that itself broke the rule that a provided argument
    /// records an ext — reproduced as it was found, rather than repaired or
    /// panicked over.
    pub(super) ext: Option<ArgumentExt<L>>,
}

impl<L: Lang> RestagedArgument<L> {
    /// Builds a provided argument from its parts.
    ///
    /// `nodes` are the staged nodes of the argument's full syntactic extent in
    /// order — leading whitespace and comments, wrapper syntax, and content alike;
    /// `content` designates which of them are the argument's content, with offsets
    /// counted within `nodes`; `ext` is the record ext.
    pub fn provided(
        spec: Arc<ArgumentSpec<L>>,
        nodes: Vec<BuildId>,
        content: ContentNodes,
        ext: ArgumentExt<L>,
    ) -> RestagedArgument<L> {
        RestagedArgument {
            spec,
            provided: Some(ProvidedRegion { nodes, content, ext: Some(ext) }),
        }
    }

    /// Builds an argument that is recorded against `spec` but was not provided:
    /// no nodes and no ext.
    ///
    /// This mirrors
    /// [`ParsedArgument::absent`](crate::core::node::ParsedArgument::absent) on the
    /// input side.
    pub fn absent(spec: Arc<ArgumentSpec<L>>) -> RestagedArgument<L> {
        RestagedArgument { spec, provided: None }
    }

    /// The spec this argument stays recorded against.
    pub fn spec(&self) -> &Arc<ArgumentSpec<L>> {
        &self.spec
    }

    /// Whether the bundle carries a provided region.
    pub fn is_provided(&self) -> bool {
        self.provided.is_some()
    }

    /// The staged nodes of the argument's region.
    ///
    /// Empty for an absent argument, and also for a provided argument whose region
    /// was emptied; [`is_provided`](RestagedArgument::is_provided) tells the two
    /// apart.
    pub fn nodes(&self) -> &[BuildId] {
        self.provided.as_ref().map_or(&[], |region| &region.nodes)
    }

    /// Which of [`nodes`](RestagedArgument::nodes) are the argument's content,
    /// with offsets counted within that list; `None` for an absent argument.
    pub fn content(&self) -> Option<&ContentNodes> {
        self.provided.as_ref().map(|region| &region.content)
    }

    /// The record ext of the provided region.
    ///
    /// `None` for an absent argument, and also for a region that
    /// [`restage_argument`](super::RestageContext::restage_argument) read off an
    /// input record which itself recorded no ext, reproduced as it was found.
    pub fn ext(&self) -> Option<&ArgumentExt<L>> {
        self.provided.as_ref().and_then(|region| region.ext.as_ref())
    }
}

impl<L: Lang> fmt::Debug for RestagedArgument<L> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut s = f.debug_struct("RestagedArgument");
        s.field("spec", &self.spec);
        match &self.provided {
            Some(region) => s
                .field("nodes", &region.nodes)
                .field("content", &region.content)
                .field("ext", &region.ext),
            None => s.field("provided", &false),
        };
        s.finish()
    }
}

/// One restaged slot of a callable: its name, its [`SlotRole`], its staged nodes,
/// which of those nodes are its content, and the slot ext.
///
/// Produced by [`restage_slot`](super::RestageContext::restage_slot) and its
/// siblings, or built directly with [`new`](RestagedSlot::new) /
/// [`new_unnamed`](RestagedSlot::new_unnamed); consumed by
/// [`restage_invocation`](super::RestageContext::restage_invocation).
///
/// Unlike an argument, a slot always has a region — possibly one whose content is
/// empty — so there is no absent form.
pub struct RestagedSlot<L: Lang> {
    pub(super) name: Option<Box<str>>,
    pub(super) role: SlotRole,
    pub(super) nodes: Vec<BuildId>,
    pub(super) content: ContentNodes,
    pub(super) ext: SlotExt<L>,
}

impl<L: Lang> RestagedSlot<L> {
    /// Builds a named slot; `content` designates which of `nodes` are the slot's
    /// content, with offsets counted within that list.
    ///
    /// Naming slots is the recommended practice, which is why this is the shorter
    /// of the two constructors; [`new_unnamed`](RestagedSlot::new_unnamed) builds
    /// the unnamed form.
    pub fn new(
        name: impl Into<Box<str>>,
        role: SlotRole,
        nodes: Vec<BuildId>,
        content: ContentNodes,
        ext: SlotExt<L>,
    ) -> RestagedSlot<L> {
        RestagedSlot { name: Some(name.into()), role, nodes, content, ext }
    }

    /// Builds an unnamed slot, otherwise like [`new`](RestagedSlot::new).
    pub fn new_unnamed(
        role: SlotRole,
        nodes: Vec<BuildId>,
        content: ContentNodes,
        ext: SlotExt<L>,
    ) -> RestagedSlot<L> {
        RestagedSlot { name: None, role, nodes, content, ext }
    }

    /// The slot's name.
    pub fn name(&self) -> Option<&str> {
        self.name.as_deref()
    }

    /// The slot's role.
    pub fn role(&self) -> SlotRole {
        self.role
    }

    /// The staged nodes of the slot's region.
    pub fn nodes(&self) -> &[BuildId] {
        &self.nodes
    }

    /// Which of [`nodes`](RestagedSlot::nodes) are the slot's content, with
    /// offsets counted within that list.
    pub fn content(&self) -> &ContentNodes {
        &self.content
    }

    /// The slot ext.
    pub fn ext(&self) -> &SlotExt<L> {
        &self.ext
    }
}

impl<L: Lang> fmt::Debug for RestagedSlot<L> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RestagedSlot")
            .field("name", &self.name)
            .field("role", &self.role)
            .field("nodes", &self.nodes)
            .field("content", &self.content)
            .field("ext", &self.ext)
            .finish()
    }
}
