//! [`NodeKind`]: the closed structural node taxonomy; [`CallableData`]: the payload of a
//! callable invocation.

use alloc::boxed::Box;
use alloc::sync::Arc;
use core::fmt;

use crate::source::TextContent;
use crate::spec::{CallableSpec, CallableTypeId};
use crate::state::Lang;
use crate::token::GroupTypeId;

use super::layout::{ArgLayout, ArgsLayout, SlotsLayout};
use super::{CallableNodeExt, CharsNodeExt, CommentNodeExt, GroupNodeExt, ListNodeExt};

/// What a node structurally *is* — the closed core enum of ARCHITECTURE.md §nodes
/// (Decision 3): exactly the structural shapes, no `Custom` variant, no invocation-form
/// variants.
///
/// - **No `Macro`/`Environment`/`Specials`/`Math` kinds.** Macro, environment, and
///   specials invocations differ by invocation *form*, not by parsed shape — all are
///   [`Callable`](NodeKind::Callable) nodes; "is this an environment" is
///   `callable_type == latexlike::CT_ENVIRONMENT` (honest two-level dispatch). `$…$`
///   parses as a [`Group`](NodeKind::Group) with a `$`-delimited
///   [`GroupTypeId`](crate::token::GroupTypeId) under the preset's math-mode state ext.
/// - **Custom data attaches through the per-kind ext types** (`Lang::NodeExts`),
///   orthogonal to structural identity: a group with custom data is still a group to all
///   generic tooling.
/// - **Ownership rule:** identity (callable names) is always owned; textual content is
///   [`TextContent`] — span-backed when parsed, owned when synthesized or normalized.
pub enum NodeKind<L: Lang> {
    /// A run of ordinary content characters (including whitespace-only runs — pylatexenc's
    /// whitespace-as-chars-nodes rule, pinned in Phase 6).
    Chars {
        /// The characters.
        content: TextContent,
        /// Per-kind ext data.
        ext: CharsNodeExt<L>,
    },
    /// A delimited group; the children range holds its contents.
    Group {
        /// The registered group type (delimiters recoverable through the `Language`
        /// registry; the node's span covers them verbatim).
        group_type: GroupTypeId,
        /// Per-kind ext data.
        ext: GroupNodeExt<L>,
    },
    /// A callable invocation (macro-, environment-, or specials-formed — see
    /// [`CallableData`]). Boxed: `Chars` dominates node vectors, and boxing the large
    /// payload keeps the enum small.
    Callable(Box<CallableData<L>>),
    /// A comment: the content after the start delimiter, without the terminating
    /// newline. (Whether level-2 recomposition needs delimiter/post-space fields here is
    /// pinned down with the whitespace/span invariants, Phase 6.)
    Comment {
        /// The comment text (sans delimiter and newline).
        content: TextContent,
        /// Per-kind ext data.
        ext: CommentNodeExt<L>,
    },
    /// A plain sequence of nodes (the children range): the tree root, a slot body
    /// (an environment's content), or a multi-node argument value.
    List {
        /// Per-kind ext data.
        ext: ListNodeExt<L>,
    },
}

impl<L: Lang> NodeKind<L> {
    /// A [`Chars`](NodeKind::Chars) kind with default ext.
    pub fn chars(content: impl Into<TextContent>) -> NodeKind<L> {
        NodeKind::Chars { content: content.into(), ext: Default::default() }
    }

    /// A [`Group`](NodeKind::Group) kind with default ext.
    pub fn group(group_type: GroupTypeId) -> NodeKind<L> {
        NodeKind::Group { group_type, ext: Default::default() }
    }

    /// A [`Callable`](NodeKind::Callable) kind.
    pub fn callable(data: CallableData<L>) -> NodeKind<L> {
        NodeKind::Callable(Box::new(data))
    }

    /// A [`Comment`](NodeKind::Comment) kind with default ext.
    pub fn comment(content: impl Into<TextContent>) -> NodeKind<L> {
        NodeKind::Comment { content: content.into(), ext: Default::default() }
    }

    /// A [`List`](NodeKind::List) kind with default ext.
    pub fn list() -> NodeKind<L> {
        NodeKind::List { ext: Default::default() }
    }

    /// A copy with every [`TextContent`] owned; `source_content` is the content of the
    /// carrying node's own source (the `Spanned` invariant).
    pub(crate) fn materialized(&self, source_content: &str) -> NodeKind<L> {
        match self {
            NodeKind::Chars { content, ext } => NodeKind::Chars {
                content: content.materialized(source_content),
                ext: ext.clone(),
            },
            NodeKind::Group { group_type, ext } => {
                NodeKind::Group { group_type: *group_type, ext: ext.clone() }
            }
            NodeKind::Callable(data) => {
                NodeKind::Callable(Box::new(data.materialized(source_content)))
            }
            NodeKind::Comment { content, ext } => NodeKind::Comment {
                content: content.materialized(source_content),
                ext: ext.clone(),
            },
            NodeKind::List { ext } => NodeKind::List { ext: ext.clone() },
        }
    }
}

/// The payload of a [`Callable`](NodeKind::Callable) node: the **invocation facts** — the
/// division-of-labor rule of ARCHITECTURE.md §nodes puts shared behavior in the [spec]
/// (stored once), resolution keys in the library, context in the parsing state, and
/// *here* everything specific to this one invocation.
///
/// [spec]: CallableSpec
pub struct CallableData<L: Lang> {
    /// The invocation form (latexlike: `MACRO` / `ENVIRONMENT` / `SPECIALS`).
    pub callable_type: CallableTypeId,
    /// The invocation spelling. Identity-bearing, therefore always owned (library keys
    /// hold the *normalized* name; this is the name as written).
    pub name: Box<str>,
    /// The behavior spec — shared, de-keyed, and never absent (unknown callables resolve
    /// to per-type fallback singletons, ARCHITECTURE.md §specs).
    pub spec: Arc<dyn CallableSpec<L>>,
    /// Which arguments are present and where (one node per region — see [`ArgsLayout`]).
    pub args: ArgsLayout,
    /// Where each slot's `List` node is.
    pub slots: SlotsLayout,
    /// Whitespace consumed after the invocation — reproduced verbatim in recomposition.
    /// Included in the node's span (a `Spanned` post-space is a trailing sub-range of it).
    pub post_space: TextContent,
    /// Per-kind ext data.
    pub ext: CallableNodeExt<L>,
}

impl<L: Lang> CallableData<L> {
    fn materialized(&self, source_content: &str) -> CallableData<L> {
        let args = ArgsLayout {
            args: self
                .args
                .args
                .iter()
                .map(|arg| match arg {
                    ArgLayout::Marker { text } => {
                        ArgLayout::Marker { text: text.materialized(source_content) }
                    }
                    other => other.clone(),
                })
                .collect(),
        };
        CallableData {
            callable_type: self.callable_type,
            name: self.name.clone(),
            spec: Arc::clone(&self.spec),
            args,
            slots: self.slots.clone(),
            post_space: self.post_space.materialized(source_content),
            ext: self.ext.clone(),
        }
    }
}

// Manual impls: derives would demand `L: Clone`/`L: Debug` although only associated
// types (already bounded via `NodeExtTypes`) are stored.

impl<L: Lang> Clone for NodeKind<L> {
    fn clone(&self) -> Self {
        match self {
            NodeKind::Chars { content, ext } => {
                NodeKind::Chars { content: content.clone(), ext: ext.clone() }
            }
            NodeKind::Group { group_type, ext } => {
                NodeKind::Group { group_type: *group_type, ext: ext.clone() }
            }
            NodeKind::Callable(data) => NodeKind::Callable(data.clone()),
            NodeKind::Comment { content, ext } => {
                NodeKind::Comment { content: content.clone(), ext: ext.clone() }
            }
            NodeKind::List { ext } => NodeKind::List { ext: ext.clone() },
        }
    }
}

impl<L: Lang> fmt::Debug for NodeKind<L> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            NodeKind::Chars { content, ext } => {
                f.debug_struct("Chars").field("content", content).field("ext", ext).finish()
            }
            NodeKind::Group { group_type, ext } => f
                .debug_struct("Group")
                .field("group_type", group_type)
                .field("ext", ext)
                .finish(),
            NodeKind::Callable(data) => f.debug_tuple("Callable").field(data).finish(),
            NodeKind::Comment { content, ext } => {
                f.debug_struct("Comment").field("content", content).field("ext", ext).finish()
            }
            NodeKind::List { ext } => f.debug_struct("List").field("ext", ext).finish(),
        }
    }
}

impl<L: Lang> Clone for CallableData<L> {
    fn clone(&self) -> Self {
        CallableData {
            callable_type: self.callable_type,
            name: self.name.clone(),
            spec: Arc::clone(&self.spec),
            args: self.args.clone(),
            slots: self.slots.clone(),
            post_space: self.post_space.clone(),
            ext: self.ext.clone(),
        }
    }
}

impl<L: Lang> fmt::Debug for CallableData<L> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CallableData")
            .field("callable_type", &self.callable_type)
            .field("name", &self.name)
            .field("spec", &self.spec)
            .field("args", &self.args)
            .field("slots", &self.slots)
            .field("post_space", &self.post_space)
            .field("ext", &self.ext)
            .finish()
    }
}
