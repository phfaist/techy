//! [`NodeKind`]: the closed structural node taxonomy; [`GroupData`] and [`CallableData`]:
//! the payloads of group and callable-invocation nodes.

use alloc::boxed::Box;
use alloc::sync::Arc;
use core::fmt;

use crate::source::TextContent;
use crate::spec::CallableSpec;
use crate::state::Lang;

use super::arguments::{ParsedArguments, ParsedSlots};
use super::{CallableNodeExt, CharsNodeExt, CommentNodeExt, GroupNodeExt, ListNodeExt};

/// What a node structurally *is* — the closed core enum of ARCHITECTURE.md §nodes
/// (Decision 3): exactly the structural shapes, no `Custom` variant, no invocation-form
/// variants.
///
/// - **No `Macro`/`Environment`/`Specials`/`Math` kinds.** Macro, environment, and
///   specials invocations differ by invocation *form*, not by parsed shape — all are
///   [`Callable`](NodeKind::Callable) nodes; "is this an environment" is
///   `callable_type == latexlike::CT_ENVIRONMENT` (honest two-level dispatch). `$…$`
///   parses as a [`Group`](NodeKind::Group) whose `$`-delimited group type is a
///   `Lang::GroupTypeId` under the preset's math-mode state ext.
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
    /// A delimited group; the children range holds its contents. Boxed like
    /// [`Callable`](NodeKind::Callable): the payload records both delimiter strings, and
    /// `Chars` should keep dominating the enum size.
    Group(Box<GroupData<L>>),
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

    /// A [`Group`](NodeKind::Group) kind.
    pub fn group(data: GroupData<L>) -> NodeKind<L> {
        NodeKind::Group(Box::new(data))
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
            NodeKind::Group(data) => {
                NodeKind::Group(Box::new(data.materialized(source_content)))
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

/// The payload of a [`Group`](NodeKind::Group) node.
///
/// **Delimiters are stored on the node** (decided July 2026, following pylatexenc's
/// `LatexGroupNode.delimiters`): recomposition and delimiter-sensitive consumers must not
/// depend on a registry lookup — the same "recomposability must not depend on `Lang`
/// cooperation" rule that puts marker spellings in [`ParsedArguments`]. The typed
/// `group_type` identity is kept *alongside* the strings ("is this a math group?" without
/// string comparison; `$…$` vs `$$…$$` share spellings, not identity).
pub struct GroupData<L: Lang> {
    /// The group type, when the group came from tokenization (or was synthesized as a
    /// specific type). `None` for internal synthesized groups that exist structurally
    /// but correspond to no language group type.
    pub group_type: Option<L::GroupTypeId>,
    /// The opening delimiter as written — span-backed when parsed, owned when
    /// synthesized.
    pub open: TextContent,
    /// The closing delimiter as written; empty when the close was never found
    /// (tolerant-parsing recovery).
    pub close: TextContent,
    /// Per-kind ext data.
    pub ext: GroupNodeExt<L>,
}

impl<L: Lang> GroupData<L> {
    /// A group of the given type with the given delimiters and default ext.
    pub fn new(
        group_type: L::GroupTypeId,
        open: impl Into<TextContent>,
        close: impl Into<TextContent>,
    ) -> GroupData<L> {
        GroupData {
            group_type: Some(group_type),
            open: open.into(),
            close: close.into(),
            ext: Default::default(),
        }
    }

    /// A synthesized group with no language group type.
    pub fn untyped(
        open: impl Into<TextContent>,
        close: impl Into<TextContent>,
    ) -> GroupData<L> {
        GroupData { group_type: None, open: open.into(), close: close.into(), ext: Default::default() }
    }

    fn materialized(&self, source_content: &str) -> GroupData<L> {
        GroupData {
            group_type: self.group_type,
            open: self.open.materialized(source_content),
            close: self.close.materialized(source_content),
            ext: self.ext.clone(),
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
    /// The invocation form (latexlike: macro / environment / specials).
    pub callable_type: L::CallableTypeId,
    /// The invocation spelling. Identity-bearing, therefore always owned (library keys
    /// hold the *normalized* name; this is the name as written).
    pub name: Box<str>,
    /// The behavior spec — shared, de-keyed, and never absent (unknown callables resolve
    /// to per-type fallback singletons, ARCHITECTURE.md §specs).
    pub spec: Arc<dyn CallableSpec<L>>,
    /// The parsed arguments: which are provided, where their nodes are, and which
    /// [`ArgumentSpec`](crate::spec::ArgumentSpec) each was parsed against.
    pub arguments: ParsedArguments<L>,
    /// The parsed slots: where each slot's `List` node is.
    pub slots: ParsedSlots<L>,
    /// Whitespace consumed after the invocation — reproduced verbatim in recomposition.
    /// Included in the node's span (a `Spanned` post-space is a trailing sub-range of it).
    pub post_space: TextContent,
    /// Per-kind ext data.
    pub ext: CallableNodeExt<L>,
}

impl<L: Lang> CallableData<L> {
    fn materialized(&self, source_content: &str) -> CallableData<L> {
        CallableData {
            callable_type: self.callable_type,
            name: self.name.clone(),
            spec: Arc::clone(&self.spec),
            arguments: self.arguments.materialized(source_content),
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
            NodeKind::Group(data) => NodeKind::Group(data.clone()),
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
            NodeKind::Group(data) => f.debug_tuple("Group").field(data).finish(),
            NodeKind::Callable(data) => f.debug_tuple("Callable").field(data).finish(),
            NodeKind::Comment { content, ext } => {
                f.debug_struct("Comment").field("content", content).field("ext", ext).finish()
            }
            NodeKind::List { ext } => f.debug_struct("List").field("ext", ext).finish(),
        }
    }
}

impl<L: Lang> Clone for GroupData<L> {
    fn clone(&self) -> Self {
        GroupData {
            group_type: self.group_type,
            open: self.open.clone(),
            close: self.close.clone(),
            ext: self.ext.clone(),
        }
    }
}

impl<L: Lang> fmt::Debug for GroupData<L> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("GroupData")
            .field("group_type", &self.group_type)
            .field("open", &self.open)
            .field("close", &self.close)
            .field("ext", &self.ext)
            .finish()
    }
}

impl<L: Lang> Clone for CallableData<L> {
    fn clone(&self) -> Self {
        CallableData {
            callable_type: self.callable_type,
            name: self.name.clone(),
            spec: Arc::clone(&self.spec),
            arguments: self.arguments.clone(),
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
            .field("arguments", &self.arguments)
            .field("slots", &self.slots)
            .field("post_space", &self.post_space)
            .field("ext", &self.ext)
            .finish()
    }
}
