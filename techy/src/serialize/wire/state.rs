//! The serialized shape of a [`ParsingState`](crate::core::ParsingState):
//! [`WireState`] and its parts — the seven token-rules sections, every one optional
//! (a language declares each feature present or absent at compile time; only the
//! present features' sections are written), the mode and ext (carried as
//! [`SerialValue`]s produced by the language's own conversions), and the scope stack
//! as provider positions. Written and read by the crate's state driver
//! ([`StateSerdeDriver`](crate::serialize::StateSerdeDriver)). The derived caches of a
//! state (the delimiter prefix table, the specials trigger characters) are never
//! written: the reading side rebuilds them.

use alloc::string::String;
use alloc::vec::Vec;

use crate::serialize::drivers::ProviderIndex;
use crate::serialize::value::SerialValue;

use super::{FromSerialValue, ToSerialValue};

/// One parsing state. Wire names not yet frozen (see the
/// `techy::serialize` module documentation, "Stability of the serialized form").
#[derive(Debug, Clone, PartialEq, Eq, ToSerialValue, FromSerialValue)]
pub(crate) struct WireState {
    /// The tokenization rules.
    #[serial(name = "token_rules")]
    pub(crate) rules: WireTokenRules,
    /// The parsing mode, in the language's own form (`ModeId`'s value conversion).
    #[serial(name = "mode")]
    pub(crate) mode: SerialValue,
    /// The language-specific state ext, in the language's own form (`StateExt`'s
    /// value conversion).
    #[serial(name = "ext")]
    pub(crate) ext: SerialValue,
    /// The scope stack: the providers, outermost first.
    #[serial(name = "scopes")]
    pub(crate) scopes: Vec<ProviderIndex>,
}

/// The tokenization rules: one optional section per feature.
#[derive(Debug, Clone, PartialEq, Eq, ToSerialValue, FromSerialValue)]
pub(crate) struct WireTokenRules {
    /// Whitespace handling.
    #[serial(name = "whitespace")]
    pub(crate) whitespace: Option<WireWhitespaceRules>,
    /// Paragraph-break detection.
    #[serial(name = "paragraphs")]
    pub(crate) paragraphs: Option<WireParagraphRules>,
    /// Group delimiters.
    #[serial(name = "groups")]
    pub(crate) groups: Option<WireGroupRules>,
    /// Command syntax.
    #[serial(name = "commands")]
    pub(crate) commands: Option<WireCommandRules>,
    /// Comment syntax.
    #[serial(name = "comments")]
    pub(crate) comments: Option<WireCommentRules>,
    /// The specials scan.
    #[serial(name = "specials")]
    pub(crate) specials: Option<WireSpecialsRules>,
    /// The forbidden-character check.
    #[serial(name = "forbidden_chars")]
    pub(crate) forbidden_chars: Option<WireForbiddenCharsRules>,
}

/// [`WhitespaceRules`](crate::core::WhitespaceRules).
#[derive(Debug, Clone, PartialEq, Eq, ToSerialValue, FromSerialValue)]
pub(crate) struct WireWhitespaceRules {
    /// The gate.
    #[serial(name = "enabled")]
    pub(crate) enabled: bool,
    /// The whitespace characters.
    #[serial(name = "chars")]
    pub(crate) chars: String,
}

/// [`ParagraphRules`](crate::core::ParagraphRules).
#[derive(Debug, Clone, PartialEq, Eq, ToSerialValue, FromSerialValue)]
pub(crate) struct WireParagraphRules {
    /// The gate.
    #[serial(name = "enabled")]
    pub(crate) enabled: bool,
}

/// [`GroupRules`](crate::core::GroupRules).
#[derive(Debug, Clone, PartialEq, Eq, ToSerialValue, FromSerialValue)]
pub(crate) struct WireGroupRules {
    /// The gate.
    #[serial(name = "enabled")]
    pub(crate) enabled: bool,
    /// The delimiter rules, in order.
    #[serial(name = "rules")]
    pub(crate) rules: Vec<WireGroupRule>,
    /// The scoped-lifecycle rules, in order.
    #[serial(name = "temporary")]
    pub(crate) temporary: Vec<WireGroupRule>,
    /// The rule whose close delimiter takes precedence, if one is expected.
    #[serial(name = "expecting_close")]
    pub(crate) expecting_close: Option<WireGroupRule>,
}

/// [`GroupRule`](crate::core::GroupRule).
#[derive(Debug, Clone, PartialEq, Eq, ToSerialValue, FromSerialValue)]
pub(crate) struct WireGroupRule {
    /// The group class, in the language's own form (`GroupTypeId`'s value
    /// conversion).
    #[serial(name = "group_type")]
    pub(crate) group_type: SerialValue,
    /// The opening delimiter.
    #[serial(name = "open")]
    pub(crate) open: String,
    /// The closing delimiter.
    #[serial(name = "close")]
    pub(crate) close: String,
}

/// [`CommandRules`](crate::core::CommandRules).
#[derive(Debug, Clone, PartialEq, Eq, ToSerialValue, FromSerialValue)]
pub(crate) struct WireCommandRules {
    /// The gate.
    #[serial(name = "enabled")]
    pub(crate) enabled: bool,
    /// The command syntaxes, in order.
    #[serial(name = "rules")]
    pub(crate) rules: Vec<WireCommandRule>,
}

/// [`CommandRule`](crate::core::CommandRule).
#[derive(Debug, Clone, PartialEq, Eq, ToSerialValue, FromSerialValue)]
pub(crate) struct WireCommandRule {
    /// The escape character (a one-character string).
    #[serial(name = "escape_char")]
    pub(crate) escape_char: char,
    /// The characters that form multi-character command names.
    #[serial(name = "name_chars")]
    pub(crate) name_chars: String,
}

/// [`CommentRules`](crate::core::CommentRules).
#[derive(Debug, Clone, PartialEq, Eq, ToSerialValue, FromSerialValue)]
pub(crate) struct WireCommentRules {
    /// The gate.
    #[serial(name = "enabled")]
    pub(crate) enabled: bool,
    /// The comment syntaxes, in order.
    #[serial(name = "rules")]
    pub(crate) rules: Vec<WireCommentRule>,
}

/// [`CommentRule`](crate::core::CommentRule).
#[derive(Debug, Clone, PartialEq, Eq, ToSerialValue, FromSerialValue)]
pub(crate) struct WireCommentRule {
    /// The comment-start delimiter.
    #[serial(name = "start")]
    pub(crate) start: String,
}

/// [`SpecialsRules`](crate::core::SpecialsRules).
#[derive(Debug, Clone, PartialEq, Eq, ToSerialValue, FromSerialValue)]
pub(crate) struct WireSpecialsRules {
    /// The gate.
    #[serial(name = "enabled")]
    pub(crate) enabled: bool,
}

/// [`ForbiddenCharsRules`](crate::core::ForbiddenCharsRules).
#[derive(Debug, Clone, PartialEq, Eq, ToSerialValue, FromSerialValue)]
pub(crate) struct WireForbiddenCharsRules {
    /// The forbidden characters.
    #[serial(name = "chars")]
    pub(crate) chars: String,
}
