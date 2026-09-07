//! The structural node taxonomy: [`NodeKind`], and the payloads it stores —
//! [`GroupData`], [`CallableData`], and [`CommentData`] for group, callable-invocation,
//! and comment nodes.

use alloc::boxed::Box;
use alloc::sync::Arc;
use core::fmt;

use crate::source::{Source, SourceOrigin, TextContent};
use crate::spec::CallableSpec;
use crate::state::{InvocationSyntax, Lang};

use super::arguments::{ParsedArguments, ParsedSlots};

/// What a node structurally *is*: the closed set of node shapes a tree can contain.
///
/// There is no `Custom` variant and no variant per invocation form. Macro,
/// environment, and specials invocations differ by invocation *form*, not by parsed
/// shape, so all three are [`Callable`](NodeKind::Callable) nodes, and "is this an
/// environment" is a question about the recorded form — for the preset, whether
/// `callable_type` is
/// [`CallableType::Environment`](crate::latexlike::CallableType::Environment). `$…$`
/// parses as a [`Group`](NodeKind::Group) whose class (`Lang::GroupTypeId`) is the
/// preset's math-group class, under the preset's math-mode state ext.
///
/// The kind describes structure only. Language-specific per-node data is stored in the
/// uniform node ext ([`NodeExt`](super::NodeExt), minted by
/// [`Lang::make_node_ext`](crate::core::Lang::make_node_ext)), so a group carrying
/// custom data is still a group to all generic tooling; data that differs by kind is an
/// enum inside the ext.
///
/// Callable names are always owned, since they carry identity. Textual content is
/// [`TextContent`]: span-backed when parsed, owned when synthesized or normalized.
pub enum NodeKind<L: Lang> {
    /// A run of ordinary content characters, including whitespace-only runs (as in
    /// pylatexenc, whitespace is content and becomes a chars node).
    Chars {
        /// The characters.
        content: TextContent,
    },
    /// A delimited group; the children range holds its contents. The payload is boxed
    /// so that the enum stays small, as `Chars` nodes dominate a tree.
    Group(Box<GroupData<L>>),
    /// A callable invocation — macro-, environment-, or specials-formed; the payload is
    /// [`CallableData`], boxed to keep the enum small.
    Callable(Box<CallableData<L>>),
    /// A comment; the payload ([`CommentData`], boxed) records its start delimiter,
    /// content, and syntactic post-space.
    Comment(Box<CommentData>),
    /// A plain sequence of nodes (the children range): the tree root, a slot body
    /// (an environment's content), or a multi-node argument value.
    List,
}

impl<L: Lang> NodeKind<L> {
    /// A [`Chars`](NodeKind::Chars) kind.
    pub fn chars(content: impl Into<TextContent>) -> NodeKind<L> {
        NodeKind::Chars { content: content.into() }
    }

    /// A [`Group`](NodeKind::Group) kind.
    pub fn group(data: GroupData<L>) -> NodeKind<L> {
        NodeKind::Group(Box::new(data))
    }

    /// A [`Callable`](NodeKind::Callable) kind.
    pub fn callable(data: CallableData<L>) -> NodeKind<L> {
        NodeKind::Callable(Box::new(data))
    }

    /// A [`Comment`](NodeKind::Comment) kind. Arguments in source
    /// order: start delimiter, content, post-space.
    pub fn comment(
        start: impl Into<TextContent>,
        content: impl Into<TextContent>,
        post_space: impl Into<TextContent>,
    ) -> NodeKind<L> {
        NodeKind::Comment(Box::new(CommentData {
            start: start.into(),
            content: content.into(),
            post_space: post_space.into(),
        }))
    }

    /// A [`List`](NodeKind::List) kind.
    pub fn list() -> NodeKind<L> {
        NodeKind::List
    }

    /// The variant's static name: `"Chars"`, `"Group"`, `"Callable"`, `"Comment"`, or
    /// `"List"`.
    ///
    /// This is the kind label used in messages and in
    /// [`display_tree`](super::display_tree) renderings. For a one-line description
    /// that includes the payload, use [`NodeRef::summary`](super::NodeRef::summary).
    pub const fn as_str(&self) -> &'static str {
        match self {
            NodeKind::Chars { .. } => "Chars",
            NodeKind::Group(_) => "Group",
            NodeKind::Callable(_) => "Callable",
            NodeKind::Comment(_) => "Comment",
            NodeKind::List => "List",
        }
    }

    /// A copy with every [`TextContent`] owned; `source` is the carrying node's
    /// own source (the `Spanned` invariant).
    pub(crate) fn materialized(&self, source: &Source<L::SourceOrigin>) -> NodeKind<L> {
        match self {
            NodeKind::Chars { content } => NodeKind::Chars {
                content: content.materialized(source),
            },
            NodeKind::Group(data) => {
                NodeKind::Group(Box::new(data.materialized(source)))
            }
            NodeKind::Callable(data) => {
                NodeKind::Callable(Box::new(data.materialized(source)))
            }
            NodeKind::Comment(data) => {
                NodeKind::Comment(Box::new(data.materialized(source)))
            }
            NodeKind::List => NodeKind::List,
        }
    }
}

/// The payload of a [`Group`](NodeKind::Group) node: the delimiters as written, and
/// the group's class.
///
/// **The delimiters are stored on the node**, following pylatexenc's
/// `LatexGroupNode.delimiters`, so that recomposition and delimiter-sensitive consumers
/// need no registry lookup: reading a tree back must not depend on cooperation from the
/// `Lang` that produced it. The same rule keeps per-instance argument syntax as
/// ordinary nodes in the argument's child region (see [`ParsedArguments`]).
///
/// The typed `group_type` *class* is kept alongside the strings, so "is this a math
/// group?" needs no string comparison; where the spelling matters within one class
/// (`$…$` versus `$$…$$`), the stored delimiters answer.
pub struct GroupData<L: Lang> {
    /// The group's class, when the group came from tokenization (the class of its
    /// [`GroupRule`](crate::core::token::GroupRule)) or was synthesized as a specific
    /// class. `None` for internal synthesized groups that exist structurally but
    /// correspond to no language group class.
    pub group_type: Option<L::GroupTypeId>,
    /// The opening delimiter as written — span-backed when parsed, owned when
    /// synthesized.
    pub open: TextContent,
    /// The closing delimiter as written; empty when the close was never found
    /// (tolerant-parsing recovery).
    pub close: TextContent,
}

impl<L: Lang> GroupData<L> {
    /// A group of the given class with the given delimiters.
    pub fn new(
        group_type: L::GroupTypeId,
        open: impl Into<TextContent>,
        close: impl Into<TextContent>,
    ) -> GroupData<L> {
        GroupData {
            group_type: Some(group_type),
            open: open.into(),
            close: close.into(),
        }
    }

    /// A synthesized group with no language group class.
    pub fn untyped(
        open: impl Into<TextContent>,
        close: impl Into<TextContent>,
    ) -> GroupData<L> {
        GroupData { group_type: None, open: open.into(), close: close.into() }
    }

    fn materialized(&self, source: &Source<L::SourceOrigin>) -> GroupData<L> {
        GroupData {
            group_type: self.group_type,
            open: self.open.materialized(source),
            close: self.close.materialized(source),
        }
    }
}

/// The payload of a [`Callable`](NodeKind::Callable) node: the **invocation facts** —
/// everything specific to this one invocation.
///
/// What is shared by every invocation of the same callable is stored once on its
/// [spec]; the keys it is resolved under belong to the scope stack's providers, and the
/// surrounding context is the parsing state recorded on the node.
///
/// [spec]: CallableSpec
pub struct CallableData<L: Lang> {
    /// The invocation form (latexlike: macro / environment / specials).
    pub callable_type: L::CallableTypeId,
    /// The invocation spelling. Identity-bearing, therefore always owned (provider
    /// keys hold the *normalized* name; this is the name as written).
    pub name: Box<str>,
    /// The behavior spec, shared through an `Arc` and never absent: an unknown callable
    /// resolves to the fallback spec of its callable type. The spec holds no lookup key
    /// of its own — the name it was found under is the `name` field above.
    pub spec: Arc<dyn CallableSpec<L>>,
    /// The parsed arguments: which are provided, where their region/content nodes are,
    /// and which [`ArgumentSpec`](crate::core::specs::ArgumentSpec) each was parsed against.
    pub arguments: ParsedArguments<L>,
    /// The parsed slots: each slot's region and content nodes.
    pub slots: ParsedSlots<L>,
    /// The language's recorded **invocation syntax**
    /// ([`Lang::InvocationSyntax`](crate::core::Lang::InvocationSyntax)): the
    /// trigger-spelling facts of *this* invocation — what was written to invoke it,
    /// such as the escape character, the trigger token's syntactic post-space, or an
    /// environment's begin/end syntax — in the language's own payload type and logical
    /// canonical form. It is a parse-level syntax channel, separate from the node ext,
    /// which holds preset-logic data. The invocation parser that staged the node mints
    /// it (the standard sites through
    /// [`FromInvocation`](crate::core::constructs::FromInvocation)).
    ///
    /// What a language records here decides how accurately its trees can be re-emitted,
    /// since recomposition reads raw node payload only: byte-exact, up to noise, or
    /// loose. `()` records nothing.
    ///
    /// The latexlike preset records, in its
    /// [`Macro`](crate::latexlike::InvocationSyntaxData::Macro) arm, the trigger
    /// token's own syntactic post-space — the name-terminating whitespace of a
    /// multi-character command, pylatexenc's `macro_post_space`, which under span
    /// tiling ([`Lang::OBEYS_SPAN_TILING`](crate::core::Lang::OBEYS_SPAN_TILING)) is a
    /// sub-range of the node's span lying *outside* the argument and slot regions —
    /// and, in its
    /// [`Environment`](crate::latexlike::InvocationSyntaxData::Environment) arm, the
    /// begin/end syntax facts of environment-shaped callables.
    pub invocation_syntax: L::InvocationSyntax,
}

impl<L: Lang> CallableData<L> {
    fn materialized(&self, source: &Source<L::SourceOrigin>) -> CallableData<L> {
        CallableData {
            callable_type: self.callable_type,
            name: self.name.clone(),
            spec: Arc::clone(&self.spec),
            arguments: self.arguments.clone(),
            slots: self.slots.clone(),
            invocation_syntax: self.invocation_syntax.materialized(source),
        }
    }
}

/// The payload of a [`Comment`](NodeKind::Comment) node: start delimiter, content, and
/// syntactic post-space, each recorded on the node.
///
/// Recording them is the principle behind [`GroupData`]'s delimiters: with several
/// `CommentRule`s in scope, *which* delimiter fired and what post-space followed are
/// per-instance facts, and re-emitting a comment must not depend on cooperation from
/// the `Lang`. The node's span covers all three parts.
///
/// The fields are in source order. Unlike its [`GroupData`] and [`CallableData`]
/// siblings, the payload stores nothing language-specific, so the struct has no `Lang`
/// type parameter.
#[derive(Clone, Debug)]
pub struct CommentData {
    /// The start delimiter as written (`%` in LaTeX).
    pub start: TextContent,
    /// The comment text (sans delimiter and newline).
    pub content: TextContent,
    /// Syntactic whitespace consumed after the content: the terminating newline plus
    /// following indentation — empty when the comment ran to end of input or
    /// bordered a paragraph break (the comment token's own syntactic
    /// post-space; a callable trigger's counterpart is recorded in the
    /// Lang-owned [`CallableData::invocation_syntax`] payload).
    pub post_space: TextContent,
}

impl CommentData {
    fn materialized<O: SourceOrigin>(&self, source: &Source<O>) -> CommentData {
        CommentData {
            start: self.start.materialized(source),
            content: self.content.materialized(source),
            post_space: self.post_space.materialized(source),
        }
    }
}

// Manual impls: derives would demand `L: Clone`/`L: Debug` although only associated
// types (already bounded via `NodeExtTypes`) are stored.

impl<L: Lang> Clone for NodeKind<L> {
    fn clone(&self) -> Self {
        match self {
            NodeKind::Chars { content } => NodeKind::Chars { content: content.clone() },
            NodeKind::Group(data) => NodeKind::Group(data.clone()),
            NodeKind::Callable(data) => NodeKind::Callable(data.clone()),
            NodeKind::Comment(data) => NodeKind::Comment(data.clone()),
            NodeKind::List => NodeKind::List,
        }
    }
}

impl<L: Lang> fmt::Debug for NodeKind<L> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            NodeKind::Chars { content } => {
                f.debug_struct("Chars").field("content", content).finish()
            }
            NodeKind::Group(data) => f.debug_tuple("Group").field(data).finish(),
            NodeKind::Callable(data) => f.debug_tuple("Callable").field(data).finish(),
            NodeKind::Comment(data) => f.debug_tuple("Comment").field(data).finish(),
            NodeKind::List => f.debug_struct("List").finish(),
        }
    }
}

impl<L: Lang> Clone for GroupData<L> {
    fn clone(&self) -> Self {
        GroupData {
            group_type: self.group_type,
            open: self.open.clone(),
            close: self.close.clone(),
        }
    }
}

impl<L: Lang> fmt::Debug for GroupData<L> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("GroupData")
            .field("group_type", &self.group_type)
            .field("open", &self.open)
            .field("close", &self.close)
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
            invocation_syntax: self.invocation_syntax.clone(),
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
            .field("invocation_syntax", &self.invocation_syntax)
            .finish()
    }
}
