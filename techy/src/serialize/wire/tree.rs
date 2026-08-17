//! The serialized shape of a [`NodeTree`](crate::node::NodeTree): [`WireTree`] and its
//! parts — the nodes in storage order, each a [`WireNode`] with its structural kind
//! ([`WireNodeKind`]), span, state position, ext, and children range, and the
//! per-node annotations (omitted for the unit annotation). Written and read by the
//! crate's tree driver ([`TreeSerdeDriver`](crate::serialize::TreeSerdeDriver)); the
//! language-typed parts (a group's or a callable's type, a node's ext, an invocation
//! syntax) are carried as [`SerialValue`]s produced by the language's own conversions,
//! and the argument/slot regions are stored in a builder-ready form the reader
//! re-resolves.
//!
//! The tree layout tag and the parent table are never written: the reader mints a
//! fresh tag and recomputes the parent table when it rebuilds the tree through the
//! node builder (see [`TreeSerdeDriver`](crate::serialize::TreeSerdeDriver)).
//! Language-typed parts carry no span-backed text: the writer materializes the
//! invocation syntax against the node's source before converting it, and
//! `TextContent`'s value conversion is owned-only — only the node's own text
//! payloads below (`Chars`, `Group`, `Comment`) are span-backed on the wire, and the
//! reader validates their ranges against the node's source.

use alloc::string::{String, ToString};
use alloc::vec::Vec;

use crate::node::SlotRole;
use crate::serialize::drivers::{SpecIndex, StateIndex};
use crate::serialize::error::SerialValueError;
use crate::serialize::value::SerialValue;
use crate::source::{Span, TextContent};

use super::source::WireSpan;
use super::{
    data_variant, expect_data_variant, expect_unit_variant, read_variant, unit_variant, FieldReader,
    FieldWriter, FromSerialValue, ToSerialValue,
};

// --- context-free core value shapes (Span, TextContent, SlotRole) ---------------------
//
// These are core types with no language parameter and no table reference, so their
// serialized shapes need no serialization context; the tree wire structs embed them
// directly — the node's own text payloads in either form (`spanned` ranges are
// validated against the node's source by the reader and the builder). The
// context-aware `SerializableValue`/`DeserializableValue` impls a language's own
// payload codecs use delegate to these (see the tree driver) — restricted, for
// `TextContent`, to the owned form (a payload's conversion receives no node to
// validate a range against); a bare `Span` has no value conversion for the same reason.

/// A byte range is `{start, end}` (both `usize`); a range with `start > end` is
/// rejected on reading (a [`Span`] cannot hold one).
impl ToSerialValue for Span {
    fn to_serial_value(&self) -> Result<SerialValue, SerialValueError> {
        let mut writer = FieldWriter::with_capacity(2);
        writer.field("start", &self.start())?;
        writer.field("end", &self.end())?;
        Ok(writer.finish())
    }
}

impl FromSerialValue for Span {
    fn from_serial_value(value: &SerialValue) -> Result<Self, SerialValueError> {
        let reader = FieldReader::new(value, "Span", &["start", "end"])?;
        let start: usize = reader.field("start")?;
        let end: usize = reader.field("end")?;
        if start > end {
            return Err(SerialValueError::Custom(alloc::format!(
                "byte range start {start} is after end {end}"
            )));
        }
        // Validated: `start <= end`, so the constructor's precondition holds.
        Ok(Span::new(start, end))
    }
}

/// Textual content is `{spanned: {start, end}}` for a byte range into the carrying
/// node's own source, or `{owned: "text"}` for stored text.
impl ToSerialValue for TextContent {
    fn to_serial_value(&self) -> Result<SerialValue, SerialValueError> {
        match self {
            TextContent::Spanned(span) => Ok(data_variant("spanned", span.to_serial_value()?)),
            TextContent::Owned(text) => Ok(data_variant("owned", SerialValue::Str(text.to_string()))),
        }
    }
}

impl FromSerialValue for TextContent {
    fn from_serial_value(value: &SerialValue) -> Result<Self, SerialValueError> {
        let (name, payload) = read_variant(value, "TextContent", &["spanned", "owned"])?;
        match name {
            "spanned" => Ok(TextContent::Spanned(Span::from_serial_value(expect_data_variant("spanned", payload)?)?)),
            "owned" => {
                let text = String::from_serial_value(expect_data_variant("owned", payload)?)?;
                Ok(TextContent::Owned(text.into_boxed_str()))
            }
            // `read_variant` accepts only the listed names.
            _ => unreachable!("read_variant rejects other names"),
        }
    }
}

/// A slot's role is `"content"`, `"attached"`, or `"hidden"`.
impl ToSerialValue for SlotRole {
    fn to_serial_value(&self) -> Result<SerialValue, SerialValueError> {
        let name = match self {
            SlotRole::Content => "content",
            SlotRole::Attached => "attached",
            SlotRole::Hidden => "hidden",
        };
        Ok(unit_variant(name))
    }
}

impl FromSerialValue for SlotRole {
    fn from_serial_value(value: &SerialValue) -> Result<Self, SerialValueError> {
        let (name, payload) = read_variant(value, "SlotRole", &["content", "attached", "hidden"])?;
        expect_unit_variant(
            match name {
                "content" => "content",
                "attached" => "attached",
                _ => "hidden",
            },
            payload,
        )?;
        Ok(match name {
            "content" => SlotRole::Content,
            "attached" => SlotRole::Attached,
            _ => SlotRole::Hidden,
        })
    }
}

// --- the tree wire structs ------------------------------------------------------------

/// One node tree. The nodes are in storage order (the root at index 0, then every
/// node's children as one contiguous block); the annotations, one per node in the
/// same order, are omitted for the unit annotation. Provisional wire names (the
/// vocabulary of the serialized form is finalized before the schema is frozen).
#[derive(Debug, Clone, ToSerialValue, FromSerialValue)]
pub(crate) struct WireTree {
    /// The nodes in storage order.
    #[serial(name = "nodes")]
    pub(crate) nodes: Vec<WireNode>,
    /// The per-node annotations, in storage order — one value per node. Omitted (the
    /// unit annotation), or one value per node.
    #[serial(name = "annotations")]
    pub(crate) annotations: Option<Vec<SerialValue>>,
}

/// One stored node.
#[derive(Debug, Clone, ToSerialValue, FromSerialValue)]
pub(crate) struct WireNode {
    /// The structural kind and its payload.
    #[serial(name = "kind")]
    pub(crate) kind: WireNodeKind,
    /// The provenance span (a source position and a byte range).
    #[serial(name = "span")]
    pub(crate) span: WireSpan,
    /// The parsing state the node was parsed or synthesized under.
    #[serial(name = "state")]
    pub(crate) state: StateIndex,
    /// The uniform node ext, in the language's own form (`NodeExt`'s value conversion).
    #[serial(name = "ext")]
    pub(crate) ext: SerialValue,
    /// The node's children as a storage-index range `[start, end)`.
    #[serial(name = "children")]
    pub(crate) children: WireRange,
}

/// The structural kind of a node (the closed [`NodeKind`](crate::node::NodeKind)
/// taxonomy) with its payload.
#[derive(Debug, Clone, ToSerialValue, FromSerialValue)]
pub(crate) enum WireNodeKind {
    /// A run of content characters.
    #[serial(name = "chars")]
    Chars {
        /// The characters.
        #[serial(name = "content")]
        content: TextContent,
    },
    /// A delimited group.
    #[serial(name = "group")]
    Group {
        /// The group class, in the language's own form (`GroupTypeId`'s value
        /// conversion), or null for an untyped group.
        #[serial(name = "group_type")]
        group_type: SerialValue,
        /// The opening delimiter as written.
        #[serial(name = "open")]
        open: TextContent,
        /// The closing delimiter as written.
        #[serial(name = "close")]
        close: TextContent,
    },
    /// A callable invocation.
    #[serial(name = "callable")]
    Callable {
        /// The invocation form, in the language's own form (`CallableTypeId`'s value
        /// conversion).
        #[serial(name = "callable_type")]
        callable_type: SerialValue,
        /// The invocation spelling.
        #[serial(name = "name")]
        name: String,
        /// The behavior spec's position in the specs table.
        #[serial(name = "spec")]
        spec: SpecIndex,
        /// The parsed arguments, in invocation order.
        #[serial(name = "arguments")]
        arguments: Vec<WireArgument>,
        /// The parsed slots, in source order.
        #[serial(name = "slots")]
        slots: Vec<WireSlot>,
        /// The recorded invocation syntax, in the language's own form
        /// (`InvocationSyntax`'s value conversion).
        #[serial(name = "invocation_syntax")]
        invocation_syntax: SerialValue,
    },
    /// A comment.
    #[serial(name = "comment")]
    Comment {
        /// The start delimiter as written.
        #[serial(name = "start")]
        start: TextContent,
        /// The comment text.
        #[serial(name = "content")]
        content: TextContent,
        /// The syntactic post-space.
        #[serial(name = "post_space")]
        post_space: TextContent,
    },
    /// A plain sequence of nodes.
    #[serial(name = "list")]
    List,
}

/// One parsed argument of a callable invocation. A provided argument carries its
/// region and its ext; an absent argument carries neither (only its `spec_payload`
/// when the callable spec serializes an out-of-band argument spec — normally none, the
/// argument spec being the callable's own declared one, referred to by its index).
#[derive(Debug, Clone, PartialEq, Eq, ToSerialValue, FromSerialValue)]
pub(crate) struct WireArgument {
    /// The argument's child region, or none for an argument that was not provided.
    #[serial(name = "region")]
    pub(crate) region: Option<WireRegion>,
    /// The argument's ext (`ArgumentExt`'s value conversion), present for a provided
    /// argument.
    #[serial(name = "ext")]
    pub(crate) ext: Option<SerialValue>,
    /// The argument spec's description, when the callable spec serializes one (see
    /// [`serialize_argument_spec`](crate::spec::CallableSpec::serialize_argument_spec));
    /// none under the index rule (the argument spec is the callable's declared one).
    #[serial(name = "spec_payload")]
    pub(crate) spec_payload: Option<SerialValue>,
}

/// One parsed slot of a callable invocation.
#[derive(Debug, Clone, PartialEq, Eq, ToSerialValue, FromSerialValue)]
pub(crate) struct WireSlot {
    /// The slot's name, for by-name access.
    #[serial(name = "name")]
    pub(crate) name: Option<String>,
    /// The slot's child region.
    #[serial(name = "region")]
    pub(crate) region: WireRegion,
    /// The slot's role.
    #[serial(name = "role")]
    pub(crate) role: SlotRole,
    /// The slot's ext (`SlotExt`'s value conversion).
    #[serial(name = "ext")]
    pub(crate) ext: SerialValue,
}

/// One argument's or slot's child region in the builder-ready form the reader
/// re-resolves: the region's node offsets within the callable's child list, the
/// content nodes' offsets, and the storage index of the node whose children hold the
/// content.
///
/// - `children` are offsets into the callable's own child list `[0, child_count)`.
/// - `content_parent` is the storage index of the node whose children hold the
///   content: the callable itself when the content sits directly among the callable's
///   children, otherwise a node inside the region (an argument's group, a slot's body
///   list).
/// - `content` are offsets: within the region's own node list when `content_parent` is
///   the callable, otherwise within `content_parent`'s children.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ToSerialValue, FromSerialValue)]
pub(crate) struct WireRegion {
    /// The region's node offsets within the callable's child list.
    #[serial(name = "children")]
    pub(crate) children: WireRange,
    /// The content nodes' offsets (see the type docs).
    #[serial(name = "content")]
    pub(crate) content: WireRange,
    /// The storage index of the content parent.
    #[serial(name = "content_parent")]
    pub(crate) content_parent: u32,
}

/// A half-open range `[start, end)` of `u32`s.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ToSerialValue, FromSerialValue)]
pub(crate) struct WireRange {
    /// The range's start (inclusive).
    #[serial(name = "start")]
    pub(crate) start: u32,
    /// The range's end (exclusive).
    #[serial(name = "end")]
    pub(crate) end: u32,
}
