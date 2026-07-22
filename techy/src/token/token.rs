//! The token types: [`Token`] and [`TokenKind`].

use alloc::sync::Arc;
use core::fmt;

use crate::source::Span;
use crate::spec::CallableSpec;
use crate::state::Lang;

use super::rules::GroupRule;

/// What a token *is* — structural and minimal: it identifies *what to parse next*.
///
/// Design invariants:
///
/// - **No invocation-form knowledge on tokens whose resolution happens at parse time.**
///   There is no macro/environment/specials taxonomy on [`Command`](TokenKind::Command)
///   tokens and no `CallableTypeId`: `\begin` tokenizes exactly like `\foobar`; which
///   names are macros or environments is decided at parse time by the preset.
///   [`Specials`](TokenKind::Specials) is the scoped exception:
///   there recognition *is* resolution, so the token carries the full resolved pair
///   (`callable_type`, `spec`). (Terminology: *command* is the token-level syntactic
///   form; *callable* the parse-level concept; *macro*/*environment* preset-level
///   flavors.)
/// - **Single-character content tokens.** [`Char`](TokenKind::Char) covers exactly one
///   character: a token is an atomic unit, and construct parsers may need char-by-char
///   reading (e.g. tabular preambles). Chars accumulate into nodes at the node level.
/// - **Two callable-trigger kinds, by mechanism.** [`Command`](TokenKind::Command) is
///   recognized from [`CommandRule`](super::CommandRule) *data*;
///   [`Specials`](TokenKind::Specials) is recognized by the `Lang::scan_specials` *hook*
///   and already carries its resolved spec.
/// - **Whole-comment tokens.** A [`Comment`](TokenKind::Comment) covers delimiter and
///   content (the parser does not care about comment interiors).
/// - **A terminal [`EndOfStream`](TokenKind::EndOfStream) token** (idempotent) instead of
///   an `Option` — its `pre_space` reports final whitespace so it can land in the node
///   tree.
///
/// The `'s` lifetime borrows the current source unit's content and is ephemeral — it never
/// enters the AST (nodes store Arc-based [`SourceSpan`](crate::source::SourceSpan)s).
pub enum TokenKind<'s, L: Lang> {
    /// A single ordinary content character. With whitespace handling disabled, whitespace
    /// characters appear as ordinary `Char` tokens too.
    Char(char),
    /// An opening group delimiter (`{`, `[`, `$`, `\(`, … — whatever the rules declare).
    GroupOpen {
        /// The delimiter as matched.
        delim: &'s str,
        /// The [`GroupRule`] that matched, as resolved by the tokenizer's priority order
        /// (expected close first, then longest match, earlier rules winning ties). It
        /// travels with the token — same principle as [`Specials`](TokenKind::Specials)
        /// carrying its resolved spec — so the parser learns the close delimiter to
        /// expect and the group's class without re-deriving the match.
        rule: Arc<GroupRule<L>>,
    },
    /// A closing group delimiter. Carries only the matched string: the parser knows
    /// which close it expects (it entered the group), and a stray close needs no more.
    GroupClose {
        /// The delimiter as matched.
        delim: &'s str,
    },
    /// A command: escape character followed by a name (`\textbf`, `\&`, `\begin`).
    /// Recognition is pure [`CommandRule`](super::CommandRule) data; resolution to a spec
    /// happens at parse time (preset lookup) — deliberately *not* during tokenization.
    Command {
        /// The command name, without the escape character.
        name: &'s str,
        /// The escape character that introduced the command — i.e. which
        /// [`CommandRule`](super::CommandRule) fired (a syntactic fact, not resolution
        /// output). Parse-time lookup needs it for disambiguation when several command
        /// syntaxes coexist (`CallableQuery { syntax: Command { escape_char } }`), and it
        /// is not otherwise recoverable from the token.
        escape_char: char,
        /// Syntactic whitespace consumed after a multi-character name (the whitespace
        /// that terminates the name is part of the invocation syntax, not content).
        /// Empty for single-character names (`\&`). Trailing sub-range of `span`.
        post_space: Span,
    },
    /// A specials trigger (`~`, `&`, `---`, …), recognized *and resolved* by the
    /// `Lang::scan_specials` hook — recognition is a lookup, so the token carries the
    /// full resolution: the invocation form *and* the spec (never absent; unknown-name
    /// fallback is the scan's business). Exactly a
    /// [`ResolvedCallable`](crate::engine::ResolvedCallable)'s pair — what the dispatch
    /// loop needs to build an `Invocation`.
    Specials {
        /// The invocation form the trigger resolved to (a preset's specials form; a
        /// fence-block form; …).
        callable_type: L::CallableTypeId,
        /// The specials name (usually the matched string itself).
        name: &'s str,
        /// The resolved behavior spec.
        spec: Arc<dyn CallableSpec<L>>,
    },
    /// A whole comment: start delimiter plus content, up to (not including) the
    /// terminating newline.
    Comment {
        /// The matched start delimiter's span — a leading sub-range of the token's
        /// `span`. Which delimiter fired is a per-instance fact (several comment
        /// syntaxes may coexist), and it pins down the content span as
        /// `start.end..post_space.start` *without* assuming `content` was sliced
        /// verbatim from the source — a custom [`TokenReader`](super::TokenReader) may
        /// normalize `content`, so consumers must never reconstruct spans from
        /// `content.len()`.
        start: Span,
        /// The comment text after the start delimiter, without the newline.
        content: &'s str,
        /// Syntactic whitespace consumed after the content: the terminating newline plus
        /// following indentation — unless that whitespace run forms a paragraph break, in
        /// which case the comment takes no post-space and a `ParagraphBreak` token
        /// follows. Trailing sub-range of `span`.
        post_space: Span,
    },
    /// A paragraph break: a whitespace run containing two or more newlines. The token's
    /// span runs from the first through the last newline (whitespace between them
    /// included); the content is recoverable from the span.
    ParagraphBreak,
    /// End of the token stream. Terminal and idempotent: every further read at the end
    /// yields it again. Its `pre_space` carries the final whitespace of the input (with
    /// no paragraph break — a trailing `…\n\n` yields its `ParagraphBreak` token first),
    /// so trailing whitespace reaches the node tree like any other whitespace.
    EndOfStream,
}

/// A token, with its byte [`Span`] and surrounding-whitespace information.
///
/// Span conventions (pylatexenc-compatible):
///
/// - `pre_space` is the whitespace immediately *before* the token; it lies **outside**
///   `span`, ending exactly at `span.start`. Pre-space is *content* whitespace: it belongs
///   to the document flow (whitespace-only chars nodes).
/// - Post-space, where a kind has it ([`Command`](TokenKind::Command),
///   [`Comment`](TokenKind::Comment) — see [`Token::post_space`]), is *syntactic*
///   whitespace consumed by the construct and ignored as content. It is a trailing
///   sub-range **inside** `span` (so `span.end` is past it), and never crosses a paragraph
///   break.
///
/// These are *token*-level conventions; node span semantics are a separate, deliberately
/// decoupled contract — tokens are transient engine internals.
///
/// Tokens are `Clone` but not `Copy` (a [`Specials`](TokenKind::Specials) token holds an
/// `Arc`); they are transient, `'s`-bound values internal to a parse.
pub struct Token<'s, L: Lang> {
    /// What the token is.
    pub kind: TokenKind<'s, L>,
    /// The token's byte range (see span conventions above).
    pub span: Span,
    /// Whitespace immediately preceding the token (empty span at `span.start` if none).
    pub pre_space: Span,
}

impl<'s, L: Lang> Token<'s, L> {
    /// Create a token.
    pub fn new(kind: TokenKind<'s, L>, span: Span, pre_space: Span) -> Token<'s, L> {
        debug_assert!(
            pre_space.end() == span.start(),
            "pre_space {:?} must end exactly at span start {:?}",
            pre_space,
            span
        );
        if let TokenKind::Command { post_space, .. } | TokenKind::Comment { post_space, .. } =
            &kind
        {
            debug_assert!(
                post_space.end() == span.end() && post_space.start() >= span.start(),
                "post_space {:?} must be a trailing sub-range of span {:?}",
                post_space,
                span
            );
        }
        if let TokenKind::Comment { start, post_space, .. } = &kind {
            debug_assert!(
                start.start() == span.start() && start.end() <= post_space.start(),
                "comment start {:?} must be a leading sub-range of span {:?} ending before post_space {:?}",
                start,
                span,
                post_space
            );
        }
        Token { kind, span, pre_space }
    }

    /// The token's post-space: syntactic whitespace consumed after the token proper.
    /// Non-empty only for [`Command`](TokenKind::Command) and
    /// [`Comment`](TokenKind::Comment) kinds; for all others, the empty span at
    /// `span.end`.
    pub fn post_space(&self) -> Span {
        match &self.kind {
            TokenKind::Command { post_space, .. } | TokenKind::Comment { post_space, .. } => {
                *post_space
            }
            _ => Span::empty(self.span.end()),
        }
    }
}

// Manual impls: derives would demand `L: Clone/Debug/PartialEq` bounds although no `L`
// value is stored (only `Arc<dyn CallableSpec<L>>`).

impl<L: Lang> Clone for TokenKind<'_, L> {
    fn clone(&self) -> Self {
        match self {
            TokenKind::Char(c) => TokenKind::Char(*c),
            TokenKind::GroupOpen { delim, rule } => {
                TokenKind::GroupOpen { delim, rule: Arc::clone(rule) }
            }
            TokenKind::GroupClose { delim } => TokenKind::GroupClose { delim },
            TokenKind::Command { name, escape_char, post_space } => TokenKind::Command {
                name,
                escape_char: *escape_char,
                post_space: *post_space,
            },
            TokenKind::Specials { callable_type, name, spec } => TokenKind::Specials {
                callable_type: *callable_type,
                name,
                spec: Arc::clone(spec),
            },
            TokenKind::Comment { start, content, post_space } => {
                TokenKind::Comment { start: *start, content, post_space: *post_space }
            }
            TokenKind::ParagraphBreak => TokenKind::ParagraphBreak,
            TokenKind::EndOfStream => TokenKind::EndOfStream,
        }
    }
}

impl<L: Lang> Clone for Token<'_, L> {
    fn clone(&self) -> Self {
        Token { kind: self.kind.clone(), span: self.span, pre_space: self.pre_space }
    }
}

/// Equality note: two `Specials` kinds are equal when their names match and they carry
/// *the same* spec (`Arc` pointer identity) — specs are shared behavior objects without
/// their own equality. `GroupOpen` rules, by contrast, are plain data and compare
/// structurally.
impl<L: Lang> PartialEq for TokenKind<'_, L> {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (TokenKind::Char(a), TokenKind::Char(b)) => a == b,
            (
                TokenKind::GroupOpen { delim: d1, rule: r1 },
                TokenKind::GroupOpen { delim: d2, rule: r2 },
            ) => d1 == d2 && r1 == r2,
            (TokenKind::GroupClose { delim: d1 }, TokenKind::GroupClose { delim: d2 }) => {
                d1 == d2
            }
            (
                TokenKind::Command { name: n1, escape_char: e1, post_space: p1 },
                TokenKind::Command { name: n2, escape_char: e2, post_space: p2 },
            ) => n1 == n2 && e1 == e2 && p1 == p2,
            (
                TokenKind::Specials { callable_type: t1, name: n1, spec: s1 },
                TokenKind::Specials { callable_type: t2, name: n2, spec: s2 },
            ) => t1 == t2 && n1 == n2 && Arc::ptr_eq(s1, s2),
            (
                TokenKind::Comment { start: s1, content: c1, post_space: p1 },
                TokenKind::Comment { start: s2, content: c2, post_space: p2 },
            ) => s1 == s2 && c1 == c2 && p1 == p2,
            (TokenKind::ParagraphBreak, TokenKind::ParagraphBreak) => true,
            (TokenKind::EndOfStream, TokenKind::EndOfStream) => true,
            _ => false,
        }
    }
}

impl<L: Lang> Eq for TokenKind<'_, L> {}

impl<L: Lang> PartialEq for Token<'_, L> {
    fn eq(&self, other: &Self) -> bool {
        self.kind == other.kind && self.span == other.span && self.pre_space == other.pre_space
    }
}

impl<L: Lang> Eq for Token<'_, L> {}

impl<L: Lang> fmt::Debug for TokenKind<'_, L> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TokenKind::Char(c) => f.debug_tuple("Char").field(c).finish(),
            TokenKind::GroupOpen { delim, rule } => f
                .debug_struct("GroupOpen")
                .field("delim", delim)
                .field("rule", rule)
                .finish(),
            TokenKind::GroupClose { delim } => {
                f.debug_struct("GroupClose").field("delim", delim).finish()
            }
            TokenKind::Command { name, escape_char, post_space } => f
                .debug_struct("Command")
                .field("name", name)
                .field("escape_char", escape_char)
                .field("post_space", post_space)
                .finish(),
            TokenKind::Specials { callable_type, name, spec } => f
                .debug_struct("Specials")
                .field("callable_type", callable_type)
                .field("name", name)
                .field("spec", spec)
                .finish(),
            TokenKind::Comment { start, content, post_space } => f
                .debug_struct("Comment")
                .field("start", start)
                .field("content", content)
                .field("post_space", post_space)
                .finish(),
            TokenKind::ParagraphBreak => write!(f, "ParagraphBreak"),
            TokenKind::EndOfStream => write!(f, "EndOfStream"),
        }
    }
}

impl<L: Lang> fmt::Debug for Token<'_, L> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Token")
            .field("kind", &self.kind)
            .field("span", &self.span)
            .field("pre_space", &self.pre_space)
            .finish()
    }
}

/// Longest char-boundary-aligned prefix of `content` within `MAX_DISPLAY_CONTENT`
/// bytes, and whether anything was cut.
fn truncate_for_display(content: &str) -> (&str, bool) {
    const MAX_DISPLAY_CONTENT: usize = 24;
    if content.len() <= MAX_DISPLAY_CONTENT {
        return (content, false);
    }
    let mut end = MAX_DISPLAY_CONTENT;
    // Bounded and underflow-free without an explicit guard: a UTF-8 char is at most
    // four bytes, so a boundary exists within three steps (`end` never drops below
    // `MAX_DISPLAY_CONTENT - 3`); and `is_char_boundary(0)` is `true`, so `end` cannot
    // underflow even in principle. The loop must only ever exit *on* a boundary —
    // exiting early would make the slice below panic.
    while !content.is_char_boundary(end) {
        end -= 1;
    }
    (&content[..end], true)
}

/// The `Display` form shows each kind's *written* spelling where the token carries it:
/// `Command(\foo)` renders the escape character that actually fired (so `\foo` and
/// `@foo` are distinguishable), delimiters and specials appear as matched, and comment
/// content is truncated to a preview.
impl<L: Lang> fmt::Display for TokenKind<'_, L> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TokenKind::Char(c) => write!(f, "Char({:?})", c),
            TokenKind::GroupOpen { delim, rule } => {
                write!(f, "GroupOpen({}, {:?})", delim, rule.group_type)
            }
            TokenKind::GroupClose { delim } => write!(f, "GroupClose({})", delim),
            TokenKind::Command { name, escape_char, .. } => {
                write!(f, "Command({}{})", escape_char, name)
            }
            TokenKind::Specials { name, .. } => write!(f, "Specials({})", name),
            TokenKind::Comment { content, .. } => {
                let (preview, truncated) = truncate_for_display(content);
                write!(f, "Comment({:?}{})", preview, if truncated { "…" } else { "" })
            }
            TokenKind::ParagraphBreak => write!(f, "ParagraphBreak"),
            TokenKind::EndOfStream => write!(f, "EndOfStream"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::truncate_for_display;
    use alloc::format;

    #[test]
    fn truncate_for_display_stops_at_char_boundaries() {
        // Short content passes through untouched.
        assert_eq!(truncate_for_display("short"), ("short", false));
        // ASCII: cut exactly at the 24-byte limit.
        let ascii = "abcdefghijklmnopqrstuvwxyz";
        assert_eq!(truncate_for_display(ascii), ("abcdefghijklmnopqrstuvwx", true));
        // A multi-byte char straddling the limit: back up to its start — at most three
        // bytes (a UTF-8 char is at most four), never past the limit minus three.
        let straddling = format!("{}🦀!", "x".repeat(22)); // '🦀' occupies bytes 22..26
        let (prefix, cut) = truncate_for_display(&straddling);
        assert_eq!(prefix, "x".repeat(22));
        assert!(cut);
    }
}
