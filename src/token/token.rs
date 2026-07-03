//! The token types: [`Token`] and [`TokenKind`].

use core::fmt;

use super::rules::GroupTypeId;
use super::span::Span;

/// What a token *is* — structural and minimal: it identifies what to parse next, nothing
/// more. Closed core enum per the naming rule (`…Kind`), exhaustively matchable.
///
/// Notably there is **no begin/end-environment token**: `\begin` is an ordinary macro
/// token, and environment recognition is a construct-parser concern (the latexlike preset
/// registers `\begin`/`\end` specs). This is a deliberate departure from pylatexenc.
/// Likewise a comment token covers only the comment-*start* delimiter; the comment's
/// content is read by the comment construct parser.
///
/// The `'s` lifetime borrows the current source unit's content and is ephemeral — it never
/// enters the AST (nodes store Arc-based [`SourceSpan`](crate::source::SourceSpan)s).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokenKind<'s> {
    /// A run of ordinary content characters, maximal up to the next whitespace or
    /// potential token start. Never contains whitespace (whitespace between tokens is
    /// `pre_space`; whitespace-only *nodes* are a nodes-parser concern, Phase 6).
    Chars(&'s str),
    /// A macro invocation: escape character followed by the macro name. The name is either
    /// a greedy run of name characters or a single non-name character (`\textbf` / `\&`).
    Macro {
        /// The macro name, without the escape character.
        name: &'s str,
    },
    /// An opening group delimiter (`{`, `[`, `$`, `\(`, … — whatever the rules declare).
    GroupOpen {
        /// The delimiter as matched.
        delim: &'s str,
        /// Which registered group type it opens.
        group_type: GroupTypeId,
    },
    /// A closing group delimiter.
    GroupClose {
        /// The delimiter as matched.
        delim: &'s str,
        /// Which registered group type it closes.
        group_type: GroupTypeId,
    },
    /// A comment-start delimiter (`%`). Only the delimiter — the comment content up to
    /// end-of-line is the comment construct parser's business.
    CommentStart {
        /// The delimiter as matched.
        delim: &'s str,
    },
    /// A specials string (`~`, `&`, `#`, …). Semantics live in libraries (Phase 4);
    /// the tokenizer only recognizes the string.
    Specials {
        /// The specials string as matched.
        chars: &'s str,
    },
    /// A paragraph break: a whitespace run containing two or more newlines. The token's
    /// span runs from the first through the last newline (whitespace between them
    /// included); the content is recoverable from the span.
    ParagraphBreak,
}

/// A token, with its byte [`Span`] and surrounding-whitespace information.
///
/// Span conventions (pylatexenc-compatible):
///
/// - `pre_space` is the whitespace immediately *before* the token; it lies **outside**
///   `span`, ending exactly at `span.start`.
/// - `post_space` is the whitespace consumed *after* the token — non-empty only for
///   [`TokenKind::Macro`] tokens with multi-character (name-chars) names, and never
///   reaching into a paragraph break. It is a trailing sub-range **inside** `span`
///   (so `span.end` is past the post-space); for all other kinds it is the empty span
///   at `span.end`.
///
/// These are *token*-level conventions; node span semantics are a separate, deliberately
/// decoupled contract (ARCHITECTURE.md §nodes) — tokens are transient engine internals.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Token<'s> {
    /// What the token is.
    pub kind: TokenKind<'s>,
    /// The token's byte range (see span conventions above).
    pub span: Span,
    /// Whitespace immediately preceding the token (empty span at `span.start` if none).
    pub pre_space: Span,
    /// Whitespace consumed after the token (empty span at `span.end` if none).
    pub post_space: Span,
}

impl<'s> Token<'s> {
    /// Create a token with no post-space.
    pub fn new(kind: TokenKind<'s>, span: Span, pre_space: Span) -> Token<'s> {
        Token { kind, span, pre_space, post_space: Span::empty(span.end) }
    }
}

impl fmt::Display for TokenKind<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TokenKind::Chars(content) => write!(f, "Chars({:?})", content),
            TokenKind::Macro { name } => write!(f, "Macro({:?})", name),
            TokenKind::GroupOpen { delim, group_type } => {
                write!(f, "GroupOpen({:?}, {:?})", delim, group_type)
            }
            TokenKind::GroupClose { delim, group_type } => {
                write!(f, "GroupClose({:?}, {:?})", delim, group_type)
            }
            TokenKind::CommentStart { delim } => write!(f, "CommentStart({:?})", delim),
            TokenKind::Specials { chars } => write!(f, "Specials({:?})", chars),
            TokenKind::ParagraphBreak => write!(f, "ParagraphBreak"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn token_new_has_empty_post_space() {
        let token = Token::new(TokenKind::Chars("abc"), Span::new(2, 5), Span::empty(2));
        assert_eq!(token.post_space, Span::empty(5));
        assert!(token.post_space.is_empty());
    }

    #[test]
    fn token_kind_display() {
        assert_eq!(
            format!("{}", TokenKind::Macro { name: "textbf" }),
            "Macro(\"textbf\")"
        );
        assert_eq!(format!("{}", TokenKind::ParagraphBreak), "ParagraphBreak");
    }
}
