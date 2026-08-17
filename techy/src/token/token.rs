//! The token types: the [`Token`] marker contract, the parser-facing [`TokenKind`]
//! view, and the standard token value [`StdToken`].

use alloc::sync::Arc;
use core::fmt;

use crate::source::Span;
use crate::spec::CallableSpec;
use crate::state::Lang;

use super::reader::TokenEdge;
use super::rules::GroupRule;

/// The marker contract of a language's token type
/// ([`Lang::Token`](crate::state::Lang::Token)).
///
/// A token is a **transient, opaque value**: a
/// [`TokenReader`](super::TokenReader) produces it, construct parsers hold it and
/// pass it around, and only a reader interprets it — what the token *is* comes from
/// [`token_kind`](super::TokenReader::token_kind), where it is from the reader's
/// position and span answers. Nothing else may read anything off a token, which is
/// what lets a reader serve tokens from more than one source during one parse
/// without its callers knowing.
///
/// The bounds are what the machinery needs of any token: `Clone` (parsers keep a
/// token while they read on), `Debug` (diagnostics and test failures), `PartialEq`
/// (equality compares the facts the reader recorded — test harnesses compare tokens
/// produced by two readers over the same content), and `Send + Sync` (a token may
/// travel with a parse that crosses threads).
///
/// The implementation this crate provides is [`StdToken`]; a language that tokenizes
/// with [`StdTokenReader`](super::StdTokenReader) uses it as its `Lang::Token`.
pub trait Token<L: Lang>: Clone + fmt::Debug + PartialEq + Send + Sync {}

/// The parser-facing view of a token: **what a token is**, and nothing about where it
/// is.
///
/// A construct parser never reads a token itself; it asks the reader that produced it
/// ([`TokenReader::token_kind`](super::TokenReader::token_kind)) and matches on the
/// answer. *Where* the token sits is a separate family of reader answers
/// ([`source_span_of`](super::TokenReader::source_span_of),
/// [`source_span_between`](super::TokenReader::source_span_between),
/// [`position_at`](super::TokenReader::position_at)) — which is why this type has no
/// span fields: only the reader knows which source a token came from, so only the
/// reader can name a location.
///
/// The view borrows from the token, and — for a reader that scans borrowed content —
/// from that content; it never borrows the reader. A parser may therefore hold a view
/// while it goes on reading and moving the stream. The view is `Copy`: pass it by
/// value.
///
/// The variants carry the written spellings the reader resolved (`delim`, `name`,
/// `start_delim`, `content` are text), where the reader's own token records whatever
/// it finds convenient.
///
/// # Design invariants of the token taxonomy
///
/// - **No invocation-form knowledge on tokens whose resolution happens at parse
///   time.** There is no macro/environment/specials taxonomy on
///   [`Command`](TokenKind::Command) tokens and no `CallableTypeId`: `\begin`
///   tokenizes exactly like `\foobar`; which names are macros or environments is
///   decided at parse time by the preset. [`Specials`](TokenKind::Specials) is the
///   scoped exception: there recognition *is* resolution, so the token carries the
///   full resolved pair (`callable_type`, `spec`). (Terminology: *command* is the
///   token-level syntactic form; *callable* the parse-level concept;
///   *macro*/*environment* preset-level flavors.)
/// - **Single-character content tokens.** [`Char`](TokenKind::Char) covers exactly one
///   character: a token is an atomic unit, and construct parsers may need
///   char-by-char reading (e.g. tabular preambles). Chars accumulate into nodes at the
///   node level.
/// - **Two callable-trigger kinds, by mechanism.** [`Command`](TokenKind::Command) is
///   recognized from [`CommandRule`](super::CommandRule) *data*;
///   [`Specials`](TokenKind::Specials) is recognized by the `Lang::scan_specials`
///   *hook* and already carries its resolved spec.
/// - **Whole-comment tokens.** A [`Comment`](TokenKind::Comment) covers delimiter and
///   content (the parser does not care about comment interiors).
/// - **A terminal [`EndOfStream`](TokenKind::EndOfStream) token** (idempotent) instead
///   of an `Option`, so that final whitespace — reported as that token's pre-space by
///   the reader — can land in the node tree.
pub enum TokenKind<'t, L: Lang> {
    /// A single ordinary content character. With whitespace handling disabled,
    /// whitespace characters appear as ordinary `Char` tokens too.
    Char(char),
    /// An opening group delimiter (`{`, `[`, `$`, `\(`, … — whatever the rules
    /// declare).
    GroupOpen {
        /// The delimiter as matched.
        delim: &'t str,
        /// The [`GroupRule`] that matched, as resolved by the tokenizer's priority
        /// order. It travels with the token so the parser learns the close delimiter
        /// to expect and the group's class without re-deriving the match.
        rule: &'t Arc<GroupRule<L>>,
    },
    /// A closing group delimiter. Carries only the matched string: the parser knows
    /// which close it expects (it entered the group), and a stray close needs no more.
    GroupClose {
        /// The delimiter as matched.
        delim: &'t str,
    },
    /// A command: escape character followed by a name (`\textbf`, `\&`, `\begin`).
    /// Resolution to a spec happens at parse time
    /// ([`ParseDriver::resolve_command`](crate::engine::ParseDriver::resolve_command)).
    Command {
        /// The command name, without the escape character.
        name: &'t str,
        /// The escape character that introduced the command — a syntactic fact
        /// parse-time lookup needs when several command syntaxes coexist.
        escape_char: char,
    },
    /// A specials trigger (`~`, `&`, `---`, …), recognized *and* resolved by the
    /// `Lang::scan_specials` hook, so the view carries the full resolution: the
    /// invocation form *and* the spec — exactly a
    /// [`ResolvedCallable`](crate::engine::ResolvedCallable)'s pair.
    Specials {
        /// The invocation form the trigger resolved to.
        callable_type: L::CallableTypeId,
        /// The specials name (the matched text).
        name: &'t str,
        /// The resolved behavior spec.
        spec: &'t Arc<dyn CallableSpec<L>>,
    },
    /// A whole comment: start delimiter plus content, up to (not including) the
    /// terminating newline. Where the two lie is a reader answer:
    /// `Start..ContentStart` for the delimiter, `ContentStart..End` for the text (see
    /// [`source_span_between`](super::TokenReader::source_span_between)).
    Comment {
        /// The start delimiter as matched (`%` in LaTeX).
        start_delim: &'t str,
        /// The comment text after the start delimiter, without the newline.
        content: &'t str,
    },
    /// A paragraph break: a whitespace run containing two or more newlines. The
    /// token runs from the first through the last newline (whitespace between them
    /// included); the text is recoverable from the reader's span.
    ParagraphBreak,
    /// End of the token stream. Terminal and idempotent: every further read at the end
    /// yields it again.
    EndOfStream,
}

impl<L: Lang> TokenKind<'_, L> {
    /// The variant's static name — the bare name without the variant's data
    /// (`"Char"`, `"GroupOpen"`, `"GroupClose"`, `"Command"`, `"Specials"`,
    /// `"Comment"`, `"ParagraphBreak"`, or `"EndOfStream"`), following
    /// [`NodeKind::as_str`](crate::node::NodeKind::as_str): for log labels and
    /// name-keyed tables. Independent of the language parameter; the data is on the
    /// variants themselves.
    pub const fn as_str(&self) -> &'static str {
        match self {
            TokenKind::Char(_) => "Char",
            TokenKind::GroupOpen { .. } => "GroupOpen",
            TokenKind::GroupClose { .. } => "GroupClose",
            TokenKind::Command { .. } => "Command",
            TokenKind::Specials { .. } => "Specials",
            TokenKind::Comment { .. } => "Comment",
            TokenKind::ParagraphBreak => "ParagraphBreak",
            TokenKind::EndOfStream => "EndOfStream",
        }
    }
}

// Manual impls: derives would demand `L: Clone/Copy/Debug/PartialEq` bounds although no
// `L` value is stored (only its `CallableTypeId` and borrowed spec/rule handles).

impl<L: Lang> Clone for TokenKind<'_, L> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<L: Lang> Copy for TokenKind<'_, L> {}

/// Equality note: two `Specials` views are equal when their names match and they carry
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
                TokenKind::Command { name: n1, escape_char: e1 },
                TokenKind::Command { name: n2, escape_char: e2 },
            ) => n1 == n2 && e1 == e2,
            (
                TokenKind::Specials { callable_type: t1, name: n1, spec: s1 },
                TokenKind::Specials { callable_type: t2, name: n2, spec: s2 },
            ) => t1 == t2 && n1 == n2 && Arc::ptr_eq(s1, s2),
            (
                TokenKind::Comment { start_delim: d1, content: c1 },
                TokenKind::Comment { start_delim: d2, content: c2 },
            ) => d1 == d2 && c1 == c2,
            (TokenKind::ParagraphBreak, TokenKind::ParagraphBreak) => true,
            (TokenKind::EndOfStream, TokenKind::EndOfStream) => true,
            _ => false,
        }
    }
}

impl<L: Lang> Eq for TokenKind<'_, L> {}

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
            TokenKind::Command { name, escape_char } => f
                .debug_struct("Command")
                .field("name", name)
                .field("escape_char", escape_char)
                .finish(),
            TokenKind::Specials { callable_type, name, spec } => f
                .debug_struct("Specials")
                .field("callable_type", callable_type)
                .field("name", name)
                .field("spec", spec)
                .finish(),
            TokenKind::Comment { start_delim, content } => f
                .debug_struct("Comment")
                .field("start_delim", start_delim)
                .field("content", content)
                .finish(),
            TokenKind::ParagraphBreak => write!(f, "ParagraphBreak"),
            TokenKind::EndOfStream => write!(f, "EndOfStream"),
        }
    }
}

/// The `Display` form shows each kind's *written* spelling: `Command(\foo)` renders the
/// escape character that actually fired (so `\foo` and `@foo` are distinguishable),
/// delimiters and specials appear as matched, and comment content is truncated to a
/// preview.
impl<L: Lang> fmt::Display for TokenKind<'_, L> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TokenKind::Char(c) => write!(f, "Char({:?})", c),
            TokenKind::GroupOpen { delim, rule } => {
                write!(f, "GroupOpen({}, {:?})", delim, rule.group_type)
            }
            TokenKind::GroupClose { delim } => write!(f, "GroupClose({})", delim),
            TokenKind::Command { name, escape_char } => {
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

/// What an [`StdToken`] records about its own kind: the facts that are *not*
/// recoverable from the content the issuing reader scans. Everything textual
/// (delimiters, names, comment text) is left to the reader, which slices its content
/// at the token's edges when asked for the [`TokenKind`] view.
///
/// In-crate only: the two readers of this crate build their views from it. Outside the
/// crate a token is opaque — a reader over standard tokens delegates interpretation to
/// an inner [`StdTokenReader`](super::StdTokenReader) over the same content.
pub(crate) enum StdTokenKindData<L: Lang> {
    /// A single ordinary content character.
    Char(char),
    /// An opening group delimiter; the matched rule travels with the token.
    GroupOpen {
        /// The [`GroupRule`] that matched.
        rule: Arc<GroupRule<L>>,
    },
    /// A closing group delimiter.
    GroupClose,
    /// A command: escape character followed by a name.
    Command {
        /// The escape character that introduced the command.
        escape_char: char,
        /// Syntactic whitespace consumed after a multi-character name; a trailing
        /// sub-range of the token's span, empty for single-character names.
        post_space: Span,
    },
    /// A specials trigger, recognized *and* resolved by the `Lang::scan_specials` hook.
    Specials {
        /// The invocation form the trigger resolved to.
        callable_type: L::CallableTypeId,
        /// The resolved behavior spec.
        spec: Arc<dyn CallableSpec<L>>,
    },
    /// A whole comment: start delimiter, content, and the syntactic whitespace after.
    Comment {
        /// The matched start delimiter's span — a leading sub-range of the token's
        /// span, so its end is where the comment's own text begins.
        start: Span,
        /// Syntactic whitespace consumed after the content (the terminating newline
        /// plus following indentation); a trailing sub-range of the token's span.
        post_space: Span,
    },
    /// A paragraph break: a whitespace run containing two or more newlines.
    ParagraphBreak,
    /// End of the token stream.
    EndOfStream,
}

/// The standard token value: what [`StdTokenReader`](super::StdTokenReader) produces,
/// and the [`Lang::Token`](crate::state::Lang::Token) of every language this crate
/// defines.
///
/// # Opaque by construction
///
/// A token records what its issuing reader found — a kind, and reader-relative byte
/// ranges for the token, its preceding whitespace, and its syntactic post-space — and
/// exposes **none** of it. A construct parser holds a token and passes it back to
/// `cx.tokens`; only a [`TokenReader`](super::TokenReader) interprets one:
///
/// - what the token is: [`token_kind`](super::TokenReader::token_kind) → a
///   [`TokenKind`] view with the written spellings;
/// - where it is: [`source_span_of`](super::TokenReader::source_span_of),
///   [`source_span_between`](super::TokenReader::source_span_between) at a
///   [`TokenEdge`], [`position_at`](super::TokenReader::position_at).
///
/// A reader outside this crate that produces `StdToken`s interprets them the same way:
/// it keeps an inner [`StdTokenReader`](super::StdTokenReader) over the same content
/// and delegates every interpretive method to it (the pattern
/// [`TokenReader`](super::TokenReader) documents). It never reads fields — there are
/// none to read — which is what keeps a token's meaning tied to the reader that
/// issued it.
///
/// # Minting tokens
///
/// The constructors below are public precisely so such a reader can mint tokens: one
/// per kind, each taking the spans the reader determined. The spans are
/// *reader-relative* — offsets into the content the issuing reader scans, which for a
/// [`StdTokenReader`](super::StdTokenReader) are offsets into its
/// [`Source`](crate::source::Source)'s content, so a wrapping reader can obtain them
/// from its inner reader's
/// [`source_span_between`](super::TokenReader::source_span_between).
///
/// Zero-copy: a token holds no string and no lifetime. Tokens are `Clone` but not
/// `Copy` (a [`Specials`](TokenKind::Specials) or [`GroupOpen`](TokenKind::GroupOpen)
/// token holds an `Arc`).
///
/// # Span conventions (pylatexenc-compatible)
///
/// - `pre_space` is the whitespace immediately *before* the token; it lies **outside**
///   the token's span, ending exactly where the span starts. Pre-space is *content*
///   whitespace: it belongs to the document flow (whitespace-only chars nodes).
/// - Post-space, where a kind has it ([`Command`](TokenKind::Command),
///   [`Comment`](TokenKind::Comment)), is *syntactic* whitespace consumed by the
///   construct and ignored as content. It is a trailing sub-range **inside** the span
///   (so the span's end is past it), and never crosses a paragraph break.
///
/// These are *token*-level conventions; node span semantics are a separate,
/// deliberately decoupled contract — tokens are transient engine internals.
pub struct StdToken<L: Lang> {
    kind: StdTokenKindData<L>,
    span: Span,
    pre_space: Span,
}

/// Coherence check shared by every constructor: pre-space ends exactly where the token
/// begins.
fn assert_pre_space(span: Span, pre_space: Span) {
    assert!(
        pre_space.end() == span.start(),
        "pre_space {:?} must end exactly at span start {:?}",
        pre_space,
        span
    );
}

/// Coherence check for the two kinds that carry syntactic post-space.
fn assert_post_space(span: Span, post_space: Span) {
    assert!(
        post_space.end() == span.end() && post_space.start() >= span.start(),
        "post_space {:?} must be a trailing sub-range of span {:?}",
        post_space,
        span
    );
}

impl<L: Lang> StdToken<L> {
    /// A single-character content token.
    ///
    /// # Panics
    ///
    /// If `pre_space` does not end exactly where `span` starts. Span coherence is the
    /// caller's contract; a violation panics in all builds — one of the crate's few
    /// deliberate panics (see the [Panics list](techy::guide::panics)).
    pub fn char(c: char, span: Span, pre_space: Span) -> StdToken<L> {
        assert_pre_space(span, pre_space);
        StdToken { kind: StdTokenKindData::Char(c), span, pre_space }
    }

    /// An opening group delimiter token, carrying the [`GroupRule`] that matched.
    ///
    /// # Panics
    ///
    /// If `pre_space` does not end exactly where `span` starts. Span coherence is the
    /// caller's contract; a violation panics in all builds — one of the crate's few
    /// deliberate panics (see the [Panics list](techy::guide::panics)).
    pub fn group_open(rule: Arc<GroupRule<L>>, span: Span, pre_space: Span) -> StdToken<L> {
        assert_pre_space(span, pre_space);
        StdToken { kind: StdTokenKindData::GroupOpen { rule }, span, pre_space }
    }

    /// A closing group delimiter token.
    ///
    /// # Panics
    ///
    /// If `pre_space` does not end exactly where `span` starts. Span coherence is the
    /// caller's contract; a violation panics in all builds — one of the crate's few
    /// deliberate panics (see the [Panics list](techy::guide::panics)).
    pub fn group_close(span: Span, pre_space: Span) -> StdToken<L> {
        assert_pre_space(span, pre_space);
        StdToken { kind: StdTokenKindData::GroupClose, span, pre_space }
    }

    /// A command token. `span` runs from the escape character through the syntactic
    /// `post_space`; the name is what lies between the escape character and the
    /// post-space, so a reader answers it by slicing its content.
    ///
    /// # Panics
    ///
    /// If `pre_space` does not end exactly where `span` starts, or if `post_space` is
    /// not a trailing sub-range of `span`. Span coherence is the caller's contract; a
    /// violation panics in all builds — one of the crate's few deliberate panics (see
    /// the [Panics list](techy::guide::panics)).
    pub fn command(
        escape_char: char,
        span: Span,
        pre_space: Span,
        post_space: Span,
    ) -> StdToken<L> {
        assert_pre_space(span, pre_space);
        assert_post_space(span, post_space);
        StdToken {
            kind: StdTokenKindData::Command { escape_char, post_space },
            span,
            pre_space,
        }
    }

    /// A specials-trigger token, carrying the resolution the `Lang::scan_specials` hook
    /// returned. The specials name is the matched text, i.e. `span`'s content.
    ///
    /// # Panics
    ///
    /// If `pre_space` does not end exactly where `span` starts. Span coherence is the
    /// caller's contract; a violation panics in all builds — one of the crate's few
    /// deliberate panics (see the [Panics list](techy::guide::panics)).
    pub fn specials(
        callable_type: L::CallableTypeId,
        spec: Arc<dyn CallableSpec<L>>,
        span: Span,
        pre_space: Span,
    ) -> StdToken<L> {
        assert_pre_space(span, pre_space);
        StdToken {
            kind: StdTokenKindData::Specials { callable_type, spec },
            span,
            pre_space,
        }
    }

    /// A whole-comment token. `start` is the matched start delimiter's span (a leading
    /// sub-range of `span`), `post_space` the syntactic whitespace after the content
    /// (a trailing sub-range); the comment's text is what lies between them.
    ///
    /// # Panics
    ///
    /// If `pre_space` does not end exactly where `span` starts, if `post_space` is not
    /// a trailing sub-range of `span`, or if `start` is not a leading sub-range of
    /// `span` ending no later than `post_space` begins. Span coherence is the caller's
    /// contract; a violation panics in all builds — one of the crate's few deliberate
    /// panics (see the [Panics list](techy::guide::panics)).
    pub fn comment(
        start: Span,
        span: Span,
        pre_space: Span,
        post_space: Span,
    ) -> StdToken<L> {
        assert_pre_space(span, pre_space);
        assert_post_space(span, post_space);
        assert!(
            start.start() == span.start() && start.end() <= post_space.start(),
            "comment start {:?} must be a leading sub-range of span {:?} ending before post_space {:?}",
            start,
            span,
            post_space
        );
        StdToken { kind: StdTokenKindData::Comment { start, post_space }, span, pre_space }
    }

    /// A paragraph-break token. `span` runs from the first through the last newline of
    /// the whitespace run.
    ///
    /// # Panics
    ///
    /// If `pre_space` does not end exactly where `span` starts. Span coherence is the
    /// caller's contract; a violation panics in all builds — one of the crate's few
    /// deliberate panics (see the [Panics list](techy::guide::panics)).
    pub fn paragraph_break(span: Span, pre_space: Span) -> StdToken<L> {
        assert_pre_space(span, pre_space);
        StdToken { kind: StdTokenKindData::ParagraphBreak, span, pre_space }
    }

    /// The terminal end-of-stream token: an empty span at the end of `pre_space`, which
    /// carries the input's final whitespace.
    ///
    /// Takes no `span` — there is only one coherent choice — so it cannot violate the
    /// span coherence the other constructors assert.
    pub fn end_of_stream(pre_space: Span) -> StdToken<L> {
        StdToken {
            kind: StdTokenKindData::EndOfStream,
            span: Span::empty(pre_space.end()),
            pre_space,
        }
    }

    /// What the token is, as the issuing reader recorded it — the facts a reader turns
    /// into a [`TokenKind`] view, together with the content it scans.
    pub(crate) fn kind_data(&self) -> &StdTokenKindData<L> {
        &self.kind
    }

    /// The token's byte range, in the coordinates of the content the issuing reader
    /// scans (pre-space excluded, post-space included).
    // The crate's readers answer spans and positions through `edge_offset`; the two
    // direct readings serve the token module's own tests and the test list reader.
    #[allow(dead_code)]
    pub(crate) fn span(&self) -> Span {
        self.span
    }

    /// The whitespace immediately preceding the token (an empty span at the token's
    /// start if there is none).
    #[allow(dead_code)]
    pub(crate) fn pre_space(&self) -> Span {
        self.pre_space
    }

    /// The token's post-space: syntactic whitespace consumed after the token proper.
    /// Non-empty only for [`Command`](TokenKind::Command) and
    /// [`Comment`](TokenKind::Comment) kinds; for all others, the empty span at the
    /// token's end.
    pub(crate) fn post_space(&self) -> Span {
        match &self.kind {
            StdTokenKindData::Command { post_space, .. }
            | StdTokenKindData::Comment { post_space, .. } => *post_space,
            _ => Span::empty(self.span.end()),
        }
    }

    /// The byte offset of one of the token's five boundaries, in the coordinates of the
    /// content the issuing reader scans — the primitive behind the reader's position
    /// and span answers.
    pub(crate) fn edge_offset(&self, edge: TokenEdge) -> usize {
        match edge {
            TokenEdge::StartBeforePreSpace => self.pre_space.start(),
            TokenEdge::Start => self.span.start(),
            // Past the kind's leading marker, where it has one: a comment's start
            // delimiter (a leading sub-range of `span`, so its end is the content's
            // start) or a command's escape character. Every other kind starts its own
            // content at `span.start`.
            TokenEdge::ContentStart => match &self.kind {
                StdTokenKindData::Comment { start, .. } => start.end(),
                StdTokenKindData::Command { escape_char, .. } => {
                    self.span.start() + escape_char.len_utf8()
                }
                _ => self.span.start(),
            },
            // Post-space is a trailing sub-range of `span`, so its start is the end of
            // the token proper — for every kind (an empty post-space sits at `span.end`).
            TokenEdge::End => self.post_space().start(),
            TokenEdge::EndPastPostSpace => self.span.end(),
        }
    }

    /// The same token with its pre-space narrowed to `pre_space` — what a reader
    /// serving a pre-scanned token from a position inside its pre-space run reports
    /// (in-crate: the test list reader).
    ///
    /// Panics on the same span coherence as the constructors.
    #[allow(dead_code)]
    pub(crate) fn with_pre_space(&self, pre_space: Span) -> StdToken<L> {
        assert_pre_space(self.span, pre_space);
        StdToken { kind: self.kind.clone(), span: self.span, pre_space }
    }
}

impl<L: Lang> Token<L> for StdToken<L> {}

// Manual impls: derives would demand `L: Clone/Debug/PartialEq` bounds although no `L`
// value is stored (only `L::CallableTypeId` and `Arc` handles).

impl<L: Lang> Clone for StdTokenKindData<L> {
    fn clone(&self) -> Self {
        match self {
            StdTokenKindData::Char(c) => StdTokenKindData::Char(*c),
            StdTokenKindData::GroupOpen { rule } => {
                StdTokenKindData::GroupOpen { rule: Arc::clone(rule) }
            }
            StdTokenKindData::GroupClose => StdTokenKindData::GroupClose,
            StdTokenKindData::Command { escape_char, post_space } => {
                StdTokenKindData::Command {
                    escape_char: *escape_char,
                    post_space: *post_space,
                }
            }
            StdTokenKindData::Specials { callable_type, spec } => {
                StdTokenKindData::Specials {
                    callable_type: *callable_type,
                    spec: Arc::clone(spec),
                }
            }
            StdTokenKindData::Comment { start, post_space } => {
                StdTokenKindData::Comment { start: *start, post_space: *post_space }
            }
            StdTokenKindData::ParagraphBreak => StdTokenKindData::ParagraphBreak,
            StdTokenKindData::EndOfStream => StdTokenKindData::EndOfStream,
        }
    }
}

impl<L: Lang> Clone for StdToken<L> {
    fn clone(&self) -> Self {
        StdToken { kind: self.kind.clone(), span: self.span, pre_space: self.pre_space }
    }
}

/// Equality note: two `Specials` kinds are equal when they carry *the same* spec (`Arc`
/// pointer identity) — specs are shared behavior objects without their own equality.
/// `GroupOpen` rules, by contrast, are plain data and compare structurally.
impl<L: Lang> PartialEq for StdTokenKindData<L> {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (StdTokenKindData::Char(a), StdTokenKindData::Char(b)) => a == b,
            (
                StdTokenKindData::GroupOpen { rule: r1 },
                StdTokenKindData::GroupOpen { rule: r2 },
            ) => r1 == r2,
            (StdTokenKindData::GroupClose, StdTokenKindData::GroupClose) => true,
            (
                StdTokenKindData::Command { escape_char: e1, post_space: p1 },
                StdTokenKindData::Command { escape_char: e2, post_space: p2 },
            ) => e1 == e2 && p1 == p2,
            (
                StdTokenKindData::Specials { callable_type: t1, spec: s1 },
                StdTokenKindData::Specials { callable_type: t2, spec: s2 },
            ) => t1 == t2 && Arc::ptr_eq(s1, s2),
            (
                StdTokenKindData::Comment { start: s1, post_space: p1 },
                StdTokenKindData::Comment { start: s2, post_space: p2 },
            ) => s1 == s2 && p1 == p2,
            (StdTokenKindData::ParagraphBreak, StdTokenKindData::ParagraphBreak) => true,
            (StdTokenKindData::EndOfStream, StdTokenKindData::EndOfStream) => true,
            _ => false,
        }
    }
}

impl<L: Lang> Eq for StdTokenKindData<L> {}

/// Equality compares the facts the issuing reader recorded: the kind data, the token's
/// span, and its pre-space. Two readers over the same content therefore produce equal
/// tokens — what the lockstep test harnesses rest on.
impl<L: Lang> PartialEq for StdToken<L> {
    fn eq(&self, other: &Self) -> bool {
        self.kind == other.kind && self.span == other.span && self.pre_space == other.pre_space
    }
}

impl<L: Lang> Eq for StdToken<L> {}

impl<L: Lang> fmt::Debug for StdTokenKindData<L> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            StdTokenKindData::Char(c) => f.debug_tuple("Char").field(c).finish(),
            StdTokenKindData::GroupOpen { rule } => {
                f.debug_struct("GroupOpen").field("rule", rule).finish()
            }
            StdTokenKindData::GroupClose => write!(f, "GroupClose"),
            StdTokenKindData::Command { escape_char, post_space } => f
                .debug_struct("Command")
                .field("escape_char", escape_char)
                .field("post_space", post_space)
                .finish(),
            StdTokenKindData::Specials { callable_type, spec } => f
                .debug_struct("Specials")
                .field("callable_type", callable_type)
                .field("spec", spec)
                .finish(),
            StdTokenKindData::Comment { start, post_space } => f
                .debug_struct("Comment")
                .field("start", start)
                .field("post_space", post_space)
                .finish(),
            StdTokenKindData::ParagraphBreak => write!(f, "ParagraphBreak"),
            StdTokenKindData::EndOfStream => write!(f, "EndOfStream"),
        }
    }
}

impl<L: Lang> fmt::Debug for StdToken<L> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("StdToken")
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

#[cfg(test)]
mod tests {
    use super::{StdToken, StdTokenKindData, Token, TokenEdge, TokenKind, truncate_for_display};
    use crate::source::Span;
    use crate::spec::{CallableSpec, StdCallableSpec};
    use crate::state::{Lang, TrivialLang};
    use crate::token::GroupRule;
    use alloc::format;
    use alloc::sync::Arc;

    #[derive(Debug, Clone, Copy)]
    struct PlainLang;
    impl TrivialLang for PlainLang {}

    fn rule() -> Arc<GroupRule<PlainLang>> {
        Arc::new(GroupRule { group_type: 0, open: "{".into(), close: "}".into() })
    }

    fn spec() -> Arc<dyn CallableSpec<PlainLang>> {
        Arc::new(StdCallableSpec::default())
    }

    #[test]
    fn a_trivial_lang_gets_the_standard_token_from_the_blanket_impl() {
        // `Lang::Token` is the marker contract [`Token`], and the blanket
        // `impl<T: TrivialLang> Lang for T` names `StdToken<Self>` — both checked by
        // compilation here.
        fn assert_token_contract<L: Lang, T: Token<L>>() {}
        assert_token_contract::<PlainLang, <PlainLang as Lang>::Token>();

        let token: <PlainLang as Lang>::Token =
            StdToken::char('a', Span::new(0, 1), Span::empty(0));
        assert_eq!(token, StdToken::char('a', Span::new(0, 1), Span::empty(0)));
    }

    #[test]
    fn every_constructor_records_its_kind_and_spans() {
        // The happy path of all eight constructors, each read back through the
        // in-crate accessors (the only readers of a token's data).
        let tokens: [(StdToken<PlainLang>, &str, Span, Span); 8] = [
            (
                StdToken::char('a', Span::new(1, 2), Span::empty(1)),
                "Char",
                Span::new(1, 2),
                Span::empty(2),
            ),
            (
                StdToken::group_open(rule(), Span::new(0, 1), Span::empty(0)),
                "GroupOpen",
                Span::new(0, 1),
                Span::empty(1),
            ),
            (
                StdToken::group_close(Span::new(3, 4), Span::empty(3)),
                "GroupClose",
                Span::new(3, 4),
                Span::empty(4),
            ),
            (
                StdToken::command('\\', Span::new(0, 6), Span::empty(0), Span::new(4, 6)),
                "Command",
                Span::new(0, 6),
                Span::new(4, 6),
            ),
            (
                StdToken::specials(0, spec(), Span::new(2, 3), Span::new(1, 2)),
                "Specials",
                Span::new(2, 3),
                Span::empty(3),
            ),
            (
                StdToken::comment(
                    Span::new(0, 1),
                    Span::new(0, 8),
                    Span::empty(0),
                    Span::new(7, 8),
                ),
                "Comment",
                Span::new(0, 8),
                Span::new(7, 8),
            ),
            (
                StdToken::paragraph_break(Span::new(2, 4), Span::empty(2)),
                "ParagraphBreak",
                Span::new(2, 4),
                Span::empty(4),
            ),
            (
                StdToken::end_of_stream(Span::new(1, 3)),
                "EndOfStream",
                Span::empty(3),
                Span::empty(3),
            ),
        ];
        for (token, kind_name, span, post_space) in &tokens {
            assert_eq!(token.span(), *span, "{kind_name}");
            assert_eq!(token.post_space(), *post_space, "{kind_name}");
            assert_eq!(
                token.edge_offset(TokenEdge::StartBeforePreSpace),
                token.pre_space().start(),
                "{kind_name}"
            );
            assert_eq!(token.edge_offset(TokenEdge::Start), span.start(), "{kind_name}");
            assert_eq!(token.edge_offset(TokenEdge::End), post_space.start(), "{kind_name}");
            assert_eq!(
                token.edge_offset(TokenEdge::EndPastPostSpace),
                span.end(),
                "{kind_name}"
            );
            // A clone compares equal to its original, kind data included.
            assert_eq!(token.clone(), *token, "{kind_name}");
        }

        // `ContentStart` is past the leading marker where a kind has one.
        assert_eq!(tokens[3].0.edge_offset(TokenEdge::ContentStart), 1); // past `\`
        assert_eq!(tokens[5].0.edge_offset(TokenEdge::ContentStart), 1); // past `%`
        assert_eq!(tokens[0].0.edge_offset(TokenEdge::ContentStart), 1); // = Start
    }

    #[test]
    fn the_kind_data_of_each_constructor_is_its_own_variant() {
        use alloc::vec;

        // Exhaustive (no `_` arm): a new variant fails compilation here.
        fn name(data: &StdTokenKindData<PlainLang>) -> &'static str {
            match data {
                StdTokenKindData::Char(_) => "Char",
                StdTokenKindData::GroupOpen { .. } => "GroupOpen",
                StdTokenKindData::GroupClose => "GroupClose",
                StdTokenKindData::Command { .. } => "Command",
                StdTokenKindData::Specials { .. } => "Specials",
                StdTokenKindData::Comment { .. } => "Comment",
                StdTokenKindData::ParagraphBreak => "ParagraphBreak",
                StdTokenKindData::EndOfStream => "EndOfStream",
            }
        }

        let one = Span::new(0, 1);
        let none = Span::empty(0);
        let tokens: vec::Vec<StdToken<PlainLang>> = vec![
            StdToken::char('a', one, none),
            StdToken::group_open(rule(), one, none),
            StdToken::group_close(one, none),
            StdToken::command('\\', Span::new(0, 4), none, Span::empty(4)),
            StdToken::specials(0, spec(), one, none),
            StdToken::comment(one, Span::new(0, 5), none, Span::empty(5)),
            StdToken::paragraph_break(Span::new(0, 2), none),
            StdToken::end_of_stream(none),
        ];
        let expected = [
            "Char",
            "GroupOpen",
            "GroupClose",
            "Command",
            "Specials",
            "Comment",
            "ParagraphBreak",
            "EndOfStream",
        ];
        for (token, kind_name) in tokens.iter().zip(expected) {
            assert_eq!(name(token.kind_data()), kind_name);
        }
    }

    #[test]
    fn tokens_compare_specs_by_identity_and_rules_structurally() {
        let one = spec();
        let same = Arc::clone(&one);
        let other = spec();
        let token =
            |spec| StdToken::<PlainLang>::specials(0, spec, Span::new(0, 1), Span::empty(0));
        assert_eq!(token(Arc::clone(&one)), token(same));
        assert_ne!(token(one), token(other));

        // Rules are plain data: two equal rules in different allocations compare equal.
        let group = |rule| StdToken::<PlainLang>::group_open(rule, Span::new(0, 1), Span::empty(0));
        assert_eq!(group(rule()), group(rule()));
    }

    #[test]
    #[should_panic(expected = "must end exactly at span start")]
    fn a_token_with_incoherent_pre_space_panics_in_all_builds() {
        // The approved always-on precondition assert ([§dd-dr:panic-policy] rule 3).
        let _ = StdToken::<PlainLang>::char('a', Span::new(5, 6), Span::empty(3));
    }

    #[test]
    #[should_panic(expected = "must be a trailing sub-range of span")]
    fn a_command_token_with_a_detached_post_space_panics_in_all_builds() {
        let _ = StdToken::<PlainLang>::command(
            '\\',
            Span::new(0, 4),
            Span::empty(0),
            Span::new(4, 5),
        );
    }

    #[test]
    #[should_panic(expected = "must be a leading sub-range of span")]
    fn a_comment_token_whose_start_is_not_leading_panics_in_all_builds() {
        let _ = StdToken::<PlainLang>::comment(
            Span::new(1, 2),
            Span::new(0, 5),
            Span::empty(0),
            Span::empty(5),
        );
    }

    #[test]
    fn the_views_as_str_answers_the_bare_variant_name() {
        use alloc::vec;

        // The duplicate name table is deliberate and exhaustive (no `_` arm): adding or
        // renaming a variant fails compilation here, keeping `as_str` in step.
        fn expected(kind: &TokenKind<'_, PlainLang>) -> &'static str {
            match kind {
                TokenKind::Char(_) => "Char",
                TokenKind::GroupOpen { .. } => "GroupOpen",
                TokenKind::GroupClose { .. } => "GroupClose",
                TokenKind::Command { .. } => "Command",
                TokenKind::Specials { .. } => "Specials",
                TokenKind::Comment { .. } => "Comment",
                TokenKind::ParagraphBreak => "ParagraphBreak",
                TokenKind::EndOfStream => "EndOfStream",
            }
        }

        let rule = rule();
        let spec = spec();
        let kinds: vec::Vec<TokenKind<'_, PlainLang>> = vec![
            TokenKind::Char('a'),
            TokenKind::GroupOpen { delim: "{", rule: &rule },
            TokenKind::GroupClose { delim: "}" },
            TokenKind::Command { name: "frac", escape_char: '\\' },
            TokenKind::Specials { callable_type: 0, name: "~", spec: &spec },
            TokenKind::Comment { start_delim: "%", content: " note" },
            TokenKind::ParagraphBreak,
            TokenKind::EndOfStream,
        ];
        for kind in &kinds {
            assert_eq!(kind.as_str(), expected(kind));
        }
    }

    #[test]
    fn views_compare_specs_by_identity_and_rules_structurally() {
        let one = spec();
        let same = Arc::clone(&one);
        let other = spec();
        let view = |spec| TokenKind::<PlainLang>::Specials {
            callable_type: 0,
            name: "~",
            spec,
        };
        assert_eq!(view(&one), view(&same));
        assert_ne!(view(&one), view(&other));

        // Rules are plain data: two equal rules in different allocations compare equal.
        let one_rule = rule();
        let twin = rule();
        assert_eq!(
            TokenKind::<PlainLang>::GroupOpen { delim: "{", rule: &one_rule },
            TokenKind::GroupOpen { delim: "{", rule: &twin }
        );
    }

    #[test]
    fn the_views_display_renders_the_written_spelling() {
        use alloc::string::ToString;
        assert_eq!(
            TokenKind::<PlainLang>::Command { name: "vec", escape_char: '@' }
                .to_string(),
            "Command(@vec)"
        );
        assert_eq!(
            TokenKind::<PlainLang>::Comment {
                start_delim: "%",
                content: "0123456789012345678901234567890",
            }
            .to_string(),
            "Comment(\"012345678901234567890123\"…)"
        );
    }

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
