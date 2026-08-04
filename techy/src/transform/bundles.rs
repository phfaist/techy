//! The restaged-region bundles: [`RestagedArgument`] / [`RestagedSlot`] — what
//! the region ops produce and [`restage_invocation`](super::RestageContext::restage_invocation)
//! consumes.
//!
//! A bundle is the write-side counterpart of a
//! [`ParsedArgument`](crate::core::node::ParsedArgument) /
//! [`ParsedSlot`](crate::core::node::ParsedSlot) record: the same field
//! vocabulary (spec/name/presence/role, staged nodes, a
//! [`ContentNodes`] content designation, the record ext) in **bundle-relative
//! staging coordinates** — node offsets count the bundle's own nodes, and
//! `InChildrenOf` designations name staged [`BuildId`]s. Bundles are opaque but
//! constructible: the constructors *are* the general take-both form, so a pass
//! can hand-build what no canned op produces.

use core::fmt;

use alloc::boxed::Box;
use alloc::sync::Arc;
use alloc::vec::Vec;

use crate::node::{ArgumentExt, BuildId, ContentNodes, SlotExt, SlotRole};
use crate::spec::ArgumentSpec;
use crate::state::Lang;

/// One restaged argument of a callable: the spec it stays recorded against,
/// and — when provided — its restaged region (see the [module docs](super)).
///
/// Presence transfers the parser semantics: **absent ≠ empty**. A provided
/// argument with no nodes is an emptied region
/// ([`provided`](RestagedArgument::provided) with an empty node list); true
/// absence is [`absent`](RestagedArgument::absent) — the argument consumed
/// nothing and records no region and no ext.
pub struct RestagedArgument<L: Lang> {
    pub(super) spec: Arc<ArgumentSpec<L>>,
    pub(super) provided: Option<ProvidedRegion<L>>,
}

/// The provided half of a [`RestagedArgument`] (internal).
pub(super) struct ProvidedRegion<L: Lang> {
    pub(super) nodes: Vec<BuildId>,
    pub(super) content: ContentNodes,
    /// `Some` on every coherently built record
    /// ([`provided`](RestagedArgument::provided) demands the ext); `None` only
    /// when [`restage_argument`](super::RestageContext::restage_argument) read
    /// an input record that itself violated the provided-arguments-carry-an-ext
    /// contract — reproduced verbatim rather than repaired or panicked over.
    pub(super) ext: Option<ArgumentExt<L>>,
}

impl<L: Lang> RestagedArgument<L> {
    /// A provided argument: the staged region nodes (the argument's full
    /// syntactic extent — noise, wrappers, content — in order), the content
    /// designation in bundle-relative staging coordinates, and the record ext.
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

    /// An argument recorded against `spec` that is not provided (no region, no
    /// ext — mirroring
    /// [`ParsedArgument::absent`](crate::core::node::ParsedArgument::absent)).
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

    /// The staged region nodes (empty for an absent argument — and also for a
    /// provided-but-emptied region; [`is_provided`](RestagedArgument::is_provided)
    /// distinguishes).
    pub fn nodes(&self) -> &[BuildId] {
        self.provided.as_ref().map_or(&[], |region| &region.nodes)
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

/// One restaged slot of a callable: name, [`SlotRole`], the staged region
/// nodes, the content designation in bundle-relative staging coordinates, and
/// the slot ext (see the [module docs](super)). Slots always have a region —
/// possibly with empty content — so there is no absent form.
pub struct RestagedSlot<L: Lang> {
    pub(super) name: Option<Box<str>>,
    pub(super) role: SlotRole,
    pub(super) nodes: Vec<BuildId>,
    pub(super) content: ContentNodes,
    pub(super) ext: SlotExt<L>,
}

impl<L: Lang> RestagedSlot<L> {
    /// A named slot (naming is the encouraged spelling; the unnamed form is the
    /// marked, longer [`new_unnamed`](RestagedSlot::new_unnamed)).
    pub fn new(
        name: impl Into<Box<str>>,
        role: SlotRole,
        nodes: Vec<BuildId>,
        content: ContentNodes,
        ext: SlotExt<L>,
    ) -> RestagedSlot<L> {
        RestagedSlot { name: Some(name.into()), role, nodes, content, ext }
    }

    /// An unnamed slot ([`new`](RestagedSlot::new) names the slot).
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

    /// The staged region nodes.
    pub fn nodes(&self) -> &[BuildId] {
        &self.nodes
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
