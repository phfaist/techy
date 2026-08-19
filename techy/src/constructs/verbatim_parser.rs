//! The verbatim family: parsers whose content
//! is **raw text** — no commands, no groups, no comments — read per the pinned recipe:
//! a features-disabled derived state whose
//! [`expecting_group_close`](crate::token::TokenRules::expecting_group_close) is
//! **replaced** by a rule whose close string is the verbatim terminator. The expected
//! close is ungated by the groups gate
//! ([`GroupRules::enabled`](crate::token::GroupRules::enabled)) and overrides any
//! close expectation inherited
//! from an enclosing group, so it is the single recognizer left active: the body
//! arrives as pure [`Char`](TokenKind::Char) tokens and the terminator —
//! multi-character strings included — as one
//! [`GroupClose`](TokenKind::GroupClose). These parsers
//! read through the ordinary [`TokenReader`](crate::token::TokenReader) protocol;
//! they need a **scanning** reader (a pre-scanned token list cannot re-tokenize under
//! the verbatim state — `TokenListReader`'s documented fidelity limit).
//!
//! - [`verbatim_state_delta`] — the recipe as data: the base piece custom raw-content
//!   parsers start from (`LatexVerbatimBaseParser`'s reusable core).
//! - [`VerbatimArgumentParser`] — delimited verbatim in argument position
//!   (`\verb|…|`; `LatexDelimitedVerbatimParser`): auto-matched or fixed delimiters,
//!   depth counter for paired delimiters.
//! - [`VerbatimBodyParser`] — a verbatim environment body: raw content up to its
//!   terminator ([`VerbatimBodyTerminator`] — a literal string, or a stop command
//!   back-referencing the invocation name, `\end{verbatim}`), the single newline after
//!   the opening scaffolding gobbled out of the designated content.
//!
//! # Node shapes
//!
//! The delimited form stages a [`Group`](crate::node::NodeKind::Group) node — the
//! delimiters recorded as written, class = the configured
//! [`GroupTypeId`](crate::state::Lang::GroupTypeId) — holding one `Chars` child with
//! the raw content (omitted when empty: techy never stages empty chars nodes); the
//! argument's content designation is the group's children. The environment form
//! stages the standard body `List` holding the raw-content `Chars` node; a gobbled
//! newline is **kept as a leading whitespace `Chars` node but designated out of the
//! content** ([`EnvironmentBody::content`]) — techy trees keep every byte.
//!
//! The raw-content `Chars` nodes record the **verbatim state** they were read
//! under; the group/list wrappers record the surrounding state. Their content is the
//! exact span slice for a language that obeys span tiling
//! ([`Lang::OBEYS_SPAN_TILING`](crate::state::Lang::OBEYS_SPAN_TILING)) and the text
//! the reader answered, token by token, for a language with
//! `OBEYS_SPAN_TILING = false` — where the tokens the raw content is read from need
//! not form one contiguous stretch of one source.

use alloc::boxed::Box;
use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec;
use alloc::vec::Vec;
use core::fmt;

use crate::error::DiagnosticInfo;
use crate::node::{ArgumentExt, ContentNodes, GroupData, NodeKind};
use crate::engine::{Frame, FrameTitle};
use crate::source::{SourceSpan, TextContent};
use crate::spec::{ArgumentParser, ArgumentSpec, ParsedArgumentNodes};
use crate::state::{
    CommandOverrides, CommentOverrides, FeaturePresence, GroupOverrides, Lang,
    LangFeatures, LangHasGroups, ParagraphOverrides, ParsingState, ParsingStateDelta,
    SpecialsOverrides, TokenRulesOverrides,
};
use crate::token::{GroupRule, StreamPosition, TokenEdge, TokenKind};

use super::argument_parsers::stage_pre_space;
use super::environment_parser::{
    EnvironmentBody, EnvironmentTerminatorSyntaxData, MissingEnvironmentTerminator,
    MissingTerminatorFound, NameGroup,
};
use super::{node_text_content, push_pre_space_text, push_token_text, ConstructParser, ConstructParserResult, ParseContext};

/// Condition: the input (or a tolerated unreadable token) ended inside a delimited
/// verbatim region before its closing delimiter appeared. Tolerant recovery keeps the
/// content read so far and records an **empty** close on the group node (the
/// [`GroupData::close`] never-found convention).
#[derive(Debug, Clone, PartialEq, Eq, DiagnosticInfo)]
#[non_exhaustive]
#[diagnostic(
    id = "core.verbatim.unterminated-verbatim",
    message = "missing closing delimiter ‘{close}’ of the verbatim content"
)]
pub struct UnterminatedVerbatim {
    /// The closing delimiter that never appeared.
    pub close: String,
}

/// Condition: no verbatim opening delimiter could be read at a mandatory delimited
/// verbatim argument's position — end of input, or (with fixed delimiters) a different
/// character. Recovery reports the argument absent, consuming nothing.
#[derive(Debug, Clone, PartialEq, Eq, DiagnosticInfo)]
#[non_exhaustive]
#[diagnostic(id = "core.verbatim.expected-verbatim-delimiter")]
pub struct ExpectedVerbatimDelimiter {
    /// The expected opening delimiter, when the parser prescribes one (`None` = any
    /// character would have done, but none was there).
    pub expected: Option<char>,
}

// Hand-written wording: the expected delimiter is quoted only when the parser
// prescribes one (a match, which the message format string cannot express).
impl fmt::Display for ExpectedVerbatimDelimiter {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.expected {
            Some(open) => {
                write!(f, "expected opening delimiter ‘{open}’ for verbatim content")
            }
            None => write!(f, "expected a verbatim opening delimiter"),
        }
    }
}

/// The verbatim reading recipe as a [`ParsingStateDelta`]: every tokenization
/// feature the language has off
/// ([`TokenRulesOverrides::disable_all`](crate::state::TokenRulesOverrides::disable_all))
/// and [`expecting_group_close`](crate::token::TokenRules::expecting_group_close)
/// **replaced** by `terminator`. Under the derived state the
/// content arrives as pure [`Char`](TokenKind::Char) tokens and the terminator —
/// `terminator`'s `close` string, which must be non-empty to ever match — as one
/// [`GroupClose`](TokenKind::GroupClose) token.
///
/// The base building block for custom raw-content parsers; [`VerbatimArgumentParser`]
/// and [`VerbatimBodyParser`] derive their reading states through it.
/// `disable_all()` clears the forbidden-character set with everything else: a
/// character the language outlaws elsewhere reads as ordinary raw content inside
/// a verbatim region.
///
/// Requires a language with the groups feature ([`LangHasGroups`]): the terminator is
/// installed as the expected group close, which is groups data.
pub fn verbatim_state_delta<L: LangHasGroups>(
    terminator: Arc<GroupRule<L>>,
) -> ParsingStateDelta<L> {
    // The groups literal spreads from the *disabled* block, not from its default: a
    // whole-block field literal replaces everything `disable_all()` set up for that
    // block (the struct-update note on [`TokenRulesOverrides`]).
    ParsingStateDelta::new().rules(TokenRulesOverrides {
        groups: GroupOverrides {
            expecting_close: Some(Some(terminator)),
            ..GroupOverrides::disable()
        },
        ..TokenRulesOverrides::disable_all()
    })
}

/// The result of the shared raw-content loop: where the content ended and whether the
/// terminator was actually consumed (with its span).
struct RawContentEnd<L: Lang> {
    /// End of the raw content (= the terminator's start when one was found).
    content_end: StreamPosition<L>,
    /// The raw content's text as the reader answered it, token by token: `Some`
    /// exactly when the language does not obey span tiling
    /// ([`Lang::OBEYS_SPAN_TILING`](crate::state::Lang::OBEYS_SPAN_TILING) `= false`),
    /// where the tokens it was read from need not form one contiguous stretch of one
    /// source and a span could not describe their text. Covers exactly the stretch from
    /// where the loop started to [`content_end`](RawContentEnd::content_end).
    content_text: Option<String>,
    /// The consumed terminator's span, or `None` when the region ended without one
    /// (end of input, or a tolerated unreadable token).
    terminator: Option<SourceSpan<L::SourceOrigin>>,
    /// Just past the consumed terminator — equal to `content_end` when the region
    /// ended without one.
    end: StreamPosition<L>,
}

/// The raw content as node data on a `Chars` node spanning `content_span` — or `None`
/// when there is no content to stage (techy stages no empty chars node).
///
/// The text the loop accumulated where it accumulated one (a language with
/// [`OBEYS_SPAN_TILING`](crate::state::Lang::OBEYS_SPAN_TILING) `= false`), the exact
/// span slice otherwise — and *emptiness is decided the same way*: for such a language
/// the span is only what the reader described for the stretch the content was read
/// from, so whether there is content is the accumulated text's answer, not the span's.
fn raw_content_text<L: Lang, O>(
    raw_end: &RawContentEnd<L>,
    content_span: &SourceSpan<O>,
) -> Option<TextContent>
where
    O: crate::source::SourceOrigin,
{
    match &raw_end.content_text {
        Some(text) if text.is_empty() => None,
        Some(text) => Some(TextContent::Owned(text.as_str().into())),
        None if content_span.is_empty() => None,
        None => Some(TextContent::Spanned(content_span.span())),
    }
}

/// Read raw content under `state` (a [`verbatim_state_delta`]-derived state) until the
/// expected-close terminator, end of input, or a tolerated unreadable token, calling
/// `on_char` for every consumed content char (the delimited form's depth counter; it
/// returns `true` to treat a would-be terminator [`GroupClose`] as ordinary content —
/// see [`VerbatimArgumentParser`]'s pairing rule). The terminator, when found, is
/// consumed. Diagnosing the terminator-less endings is the caller's business.
///
/// For a language with
/// [`OBEYS_SPAN_TILING`](crate::state::Lang::OBEYS_SPAN_TILING) `= false` the loop also
/// accumulates the content's text from what the reader says about each token it
/// consumes — the pre-space, the spelling (a character, or a delimiter read as
/// content), and the syntactic post-space — plus the pre-space of the token that ends
/// the content, which lies before `content_end` and is content too.
fn read_raw_content<L: Lang>(
    cx: &mut ParseContext<'_, '_, L>,
    state: &Arc<ParsingState<L>>,
    mut consume_close_as_content: impl FnMut(TokenKind<'_, L>) -> bool,
    mut on_char: impl FnMut(char),
) -> ConstructParserResult<L, RawContentEnd<L>> {
    let mut text: Option<String> = (!L::OBEYS_SPAN_TILING).then(String::new);
    loop {
        let Some(token) = cx.probe_token(state)? else {
            // A tolerated unreadable token: the standard reader has nothing left to
            // reject under the recipe state (the forbidden set is cleared with the
            // other features), but a custom reader may still fail. The verbatim
            // region ends here; the enclosing content loop re-reads the error and
            // applies its own token recovery (the probe protocol,
            // DESIGN_RATIONALE.md [§dd-dr:errors]).
            let here = cx.tokens.position_here();
            return Ok(RawContentEnd {
                content_end: here.clone(),
                content_text: text,
                terminator: None,
                end: here,
            });
        };
        let kind = cx.tokens.token_kind(&token);
        match kind {
            TokenKind::Char(c) => {
                on_char(c);
                push_token_text(&mut text, cx, &token, c.encode_utf8(&mut [0u8; 4]));
                cx.tokens.move_to(&token, TokenEdge::EndPastPostSpace);
            }
            TokenKind::GroupClose { delim } => {
                cx.tokens.move_to(&token, TokenEdge::EndPastPostSpace);
                if consume_close_as_content(kind) {
                    // A nested close read as ordinary content (the pairing rule):
                    // its bytes are part of the raw content.
                    push_token_text(&mut text, cx, &token, delim);
                    continue;
                }
                // The content ends at the terminator's `Start` edge, so whatever the
                // reader reports as the terminator's pre-space is content.
                push_pre_space_text(&mut text, cx, &token);
                return Ok(RawContentEnd {
                    content_end: cx.tokens.position_at(&token, TokenEdge::Start),
                    content_text: text,
                    terminator: Some(cx.tokens.source_span_of(&token)),
                    end: cx.tokens.position_at(&token, TokenEdge::EndPastPostSpace),
                });
            }
            TokenKind::EndOfStream => {
                // Same rule as for the terminator: the end-of-stream token's
                // pre-space (the input's trailing whitespace) is content.
                push_pre_space_text(&mut text, cx, &token);
                let end = cx.tokens.position_at(&token, TokenEdge::Start);
                return Ok(RawContentEnd {
                    content_end: end.clone(),
                    content_text: text,
                    terminator: None,
                    end,
                });
            }
            other => {
                // The verbatim state disables every other recognizer; only a
                // misbehaving custom reader can produce these kinds here.
                return Err(cx.implementation_error(
                    format_args!(
                        "token reader produced a {} token under a verbatim state",
                        other
                    ),
                    cx.tokens.source_span_of(&token),
                ));
            }
        }
    }
}

/// The default auto-matched delimiter pairs of [`VerbatimArgumentParser`]:
/// `{`→`}`, `[`→`]`, `<`→`>`, `(`→`)`; any other opening character closes with
/// itself.
fn default_auto_delimiters() -> Vec<(char, char)> {
    vec![('{', '}'), ('[', ']'), ('<', '>'), ('(', ')')]
}

/// The delimited-verbatim argument parser (the `v` argument code): the argument
/// is a raw-text region between two delimiter characters, `\verb|…|`-style.
///
/// The opening delimiter is the first character after optional whitespace — **any**
/// character, read raw under a derived state where only whitespace scanning is left
/// active (comments are *not* skipped: `%` is a perfectly good `\verb` delimiter; the
/// skipped whitespace is staged as region noise like every argument's). By default the
/// closing delimiter is auto-matched: the paired closer for `{ [ < (`
/// (customizable via [`with_auto_delimiters`](VerbatimArgumentParser::with_auto_delimiters)),
/// the same character otherwise. [`with_delimiters`](VerbatimArgumentParser::with_delimiters)
/// prescribes a fixed pair instead (the `v<c1><c2>` code): a different character at the
/// position diagnoses [`ExpectedVerbatimDelimiter`] and reports the argument absent,
/// consuming nothing.
///
/// **Pairing rule**: when the delimiters differ, nested occurrences
/// of the opening character deepen a depth counter and matching closers close it —
/// `\verb{a{b}c}` reads `a{b}c` whole; with identical delimiters the first closer ends
/// the region.
///
/// The content is read as raw text, under the reading state derived through
/// [`verbatim_state_delta`], and staged as a
/// group + chars shape: a [`Group`](crate::node::NodeKind::Group) node of the
/// configured class whose single `Chars` child (omitted when the content is empty) is
/// the raw text, recorded under the verbatim state; the content designation is the
/// group's children. At end of input before the closing delimiter,
/// [`UnterminatedVerbatim`] is diagnosed (strict: abort) and the group records an
/// empty close.
///
/// Requires a language with the groups feature ([`LangHasGroups`]): the parser
/// installs its closing delimiter as an expected group close and stages a group node.
pub struct VerbatimArgumentParser<L: Lang> {
    group_type: L::GroupTypeId,
    delimiters: Option<(char, char)>,
    auto_delimiters: Vec<(char, char)>,
}

impl<L: LangHasGroups> VerbatimArgumentParser<L> {
    /// An auto-delimited verbatim argument staging groups of class `group_type`
    /// (`\verb|…|`, `\verb+…+`, …; the bare `v` code).
    pub fn new(group_type: L::GroupTypeId) -> VerbatimArgumentParser<L> {
        VerbatimArgumentParser {
            group_type,
            delimiters: None,
            auto_delimiters: default_auto_delimiters(),
        }
    }

    /// Prescribe a fixed delimiter pair (the `v<c1><c2>` code): the argument is
    /// provided only when `open` itself comes next.
    pub fn with_delimiters(mut self, open: char, close: char) -> Self {
        self.delimiters = Some((open, close));
        self
    }

    /// Replace the auto-matched delimiter table (default: `{}`, `[]`, `<>`, `()`).
    /// An opening character not in the table closes with itself. Ignored when
    /// [`with_delimiters`](VerbatimArgumentParser::with_delimiters) prescribed a pair.
    pub fn with_auto_delimiters(
        mut self,
        pairs: impl IntoIterator<Item = (char, char)>,
    ) -> Self {
        self.auto_delimiters = pairs.into_iter().collect();
        self
    }

    /// The delta of the delimiter-discovery peek: the one raw character after optional
    /// whitespace — whitespace scanning stays as the base state has it, every other
    /// recognizer is off, and an inherited close expectation is **cleared** (a `}`
    /// must be readable as a delimiter char even inside a braces group).
    fn delimiter_probe_delta(&self) -> ParsingStateDelta<L> {
        // Only the groups store is known transparent under `L: LangHasGroups`; the
        // other blocks are built through their store projections — a feature the
        // language does not have needs no disabling (nothing exists to recognize).
        ParsingStateDelta::new().rules(TokenRulesOverrides {
            paragraphs: <L::Features as LangFeatures>::Paragraphs::store_with(
                ParagraphOverrides::disable,
            ),
            groups: GroupOverrides {
                expecting_close: Some(None),
                ..GroupOverrides::disable()
            },
            commands: <L::Features as LangFeatures>::Commands::store_with(
                CommandOverrides::disable,
            ),
            comments: <L::Features as LangFeatures>::Comments::store_with(
                CommentOverrides::disable,
            ),
            specials: <L::Features as LangFeatures>::Specials::store_with(
                SpecialsOverrides::disable,
            ),
            ..TokenRulesOverrides::default()
        })
    }
}

impl<L: LangHasGroups> ArgumentParser<L> for VerbatimArgumentParser<L>
where
    ArgumentExt<L>: Default,
{
    fn parse_argument(
        &self,
        cx: &mut ParseContext<'_, '_, L>,
        _spec: &ArgumentSpec<L>,
    ) -> ConstructParserResult<L, Option<ParsedArgumentNodes<L>>> {
        // Read the opening delimiter: one raw char after optional whitespace. Each
        // "argument absent" exit below consumes nothing: the probe never moves the
        // stream, so the reader is left exactly where this parse started.
        let probe_state = cx.derive_state(&self.delimiter_probe_delta())?;
        let Some(token) = cx.probe_token(&probe_state)? else {
            return Ok(None);
        };
        let open = match cx.tokens.token_kind(&token) {
            TokenKind::Char(c) => c,
            // Under the probe state only `Char` and `EndOfStream` exist; treat
            // anything else like end of input (a misbehaving reader is caught by the
            // content loop's implementation-error arm, not the recovery path).
            _ => {
                let span = cx.tokens.source_span_of(&token);
                cx.recover(
                    ExpectedVerbatimDelimiter::new(self.delimiters.map(|(open, _)| open)),
                    span,
                )?;
                return Ok(None);
            }
        };
        let close = match self.delimiters {
            Some((expected_open, close)) => {
                if open != expected_open {
                    let span = cx.tokens.source_span_of(&token);
                    cx.recover(ExpectedVerbatimDelimiter::new(expected_open), span)?;
                    return Ok(None);
                }
                close
            }
            None => self
                .auto_delimiters
                .iter()
                .find(|(auto_open, _)| *auto_open == open)
                .map(|(_, auto_close)| *auto_close)
                .unwrap_or(open),
        };

        // Committed: the whitespace becomes region noise, the delimiter is consumed.
        let mut nodes = Vec::new();
        stage_pre_space(cx, &mut nodes, &token)?;
        cx.tokens.move_to(&token, TokenEdge::EndPastPostSpace);
        let open_span = cx.tokens.source_span_of(&token);
        let open_start = cx.tokens.position_at(&token, TokenEdge::Start);
        let content_start = cx.tokens.position_here();

        // The raw-content read under the recipe state, with the pairing depth counter:
        // nested opens (paired delimiters only) deepen, matched closes surface.
        let close_rule = Arc::new(GroupRule {
            group_type: self.group_type,
            open: String::from(open),
            close: String::from(close),
        });
        let content_state = cx.derive_state(&verbatim_state_delta(Arc::clone(&close_rule)))?;
        let paired = open != close;
        // Shared by the two `read_raw_content` callbacks (each captures it by `&`).
        let depth = core::cell::Cell::new(1usize);
        let raw_end = read_raw_content(
            cx,
            &content_state,
            |_close_kind| {
                depth.set(depth.get() - 1);
                depth.get() > 0
            },
            |c| {
                if paired && c == open {
                    depth.set(depth.get() + 1);
                }
            },
        )?;
        if raw_end.terminator.is_none() {
            cx.recover(UnterminatedVerbatim::new(String::from(close)), open_span.clone())?;
        }

        // The decided group + chars shape (module docs): the chars node under the
        // verbatim state, the group under the surrounding state.
        let mut children = Vec::new();
        let content_span = cx.source_span_within(&content_start, &raw_end.content_end)?;
        if let Some(content) = raw_content_text(&raw_end, &content_span) {
            let id = cx
                .stage_node(
                    NodeKind::chars(content),
                    content_span.clone(),
                    Arc::clone(&content_state),
                    vec![],
                )
                .map_err(|error| cx.staging_error(error, content_span))?;
            children.push(id);
        }
        let child_count = children.len() as u32;
        let group_span = cx.source_span_within(&open_start, &raw_end.end)?;
        // The recorded delimiters are node data: a span only where the reader's answer
        // lies in the node's own source, the text itself otherwise.
        let data = GroupData {
            group_type: Some(self.group_type),
            open: node_text_content(&open_span, &group_span),
            close: raw_end
                .terminator
                .as_ref()
                .map(|span| node_text_content(span, &group_span))
                .unwrap_or_else(TextContent::empty),
        };
        let group = cx
            .stage_node(
                NodeKind::group(data),
                group_span.clone(),
                Arc::clone(&cx.state),
                children,
            )
            .map_err(|error| cx.staging_error(error, group_span))?;
        nodes.push(group);
        Ok(Some(ParsedArgumentNodes::new(
            nodes,
            ContentNodes::InChildrenOf(group, 0..child_count),
            Default::default(),
        )))
    }

    /// The delimited region is mandatory: absent is a diagnosed recovery, not a valid
    /// match.
    fn can_match_empty(&self) -> bool {
        false
    }
}

impl<L: Lang> fmt::Debug for VerbatimArgumentParser<L> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("VerbatimArgumentParser")
            .field("group_type", &self.group_type)
            .field("delimiters", &self.delimiters)
            .field("auto_delimiters", &self.auto_delimiters)
            .finish()
    }
}

/// Specification of upon which tokens the verbatim-body-parser should terminate.
/// One either specifies a full literal terminator syntax that is expected, or
/// a sequence of the type `<ESC-CHAR><END-COMMAND>{<NAME>}`, such as
/// `\end{verbatim}`.
///
/// Both shapes are **read** identically: [`VerbatimBodyParser`] composes one raw
/// terminator string out of the variant's fields and installs it as the body state's
/// expected group close ([`verbatim_state_delta`]), so the whole terminator arrives
/// as a single token — a raw body never tokenizes its terminator, whatever its shape.
/// The two shapes differ in the facts the parser reports back for the consumed
/// terminator ([`EnvironmentBody::terminator`]): a literal one has no structure to
/// report beyond its span
/// ([`Literal`](EnvironmentTerminatorSyntaxData::Literal)), whereas a stop-command
/// one was composed from known pieces and reports them as
/// [`Scanned`](EnvironmentTerminatorSyntaxData::Scanned) facts — the same shape
/// [`EnvironmentBodyParser`](super::EnvironmentBodyParser) reports after its
/// tokenized scan, so a recording consumer needs no separate raw-body arm.
pub enum VerbatimBodyTerminator<'p, L : Lang> {
    /// The terminator given as one raw string, with no further structure to it.
    Literal {
        /// The terminator, as literal raw text (e.g. `|END_VERBATIM_HERE|`)
        terminator : String
    },
    /// The terminator given as an environment-terminating command back-referencing
    /// the invocation's name — `\end{verbatim}`, spelled out piece by piece.
    StopEnvironmentCommand {
        /// The escape character the terminator command is written with (`\` for
        /// `\end{verbatim}`) — the canonical spelling the composing caller uses,
        /// ordinarily the one its own opening command was written with.
        escape_char: char,
        /// The invocation name the terminator must back-reference (`lstlisting` for
        /// `\begin{lstlisting} … \end{lstlisting}`), and the name diagnostics call
        /// the environment.
        invocation_name: &'p str,
        /// The terminator command's name (`end`), the body loop's stop condition.
        stop_command_name: &'p str,
        /// The group rule in which we demand the terminator environment name to be
        /// enclosed.  (In `\end{xyz}`, this is the `{`/`}` group rule.)
        name_group_rule: Arc<GroupRule<L>>,
    }
}

impl<L: Lang> VerbatimBodyTerminator<'_, L> {
    /// The raw terminator string the body is read up to: the literal as given, or
    /// the stop command's spelling composed piece by piece — escape character,
    /// command name, name group open delimiter, invocation name, name group close
    /// delimiter (`\end{verbatim}`). No whitespace is tolerated anywhere inside it:
    /// the composed string is matched byte for byte, so `\end {verbatim}` does not
    /// end the body.
    fn text(&self) -> String {
        match self {
            VerbatimBodyTerminator::Literal { terminator } => terminator.clone(),
            VerbatimBodyTerminator::StopEnvironmentCommand {
                escape_char,
                invocation_name,
                stop_command_name,
                name_group_rule,
            } => {
                let mut text = String::new();
                text.push(*escape_char);
                text.push_str(stop_command_name);
                text.push_str(&name_group_rule.open);
                text.push_str(invocation_name);
                text.push_str(&name_group_rule.close);
                text
            }
        }
    }

    /// The spelling reported for a consumed terminator occupying `span` and
    /// ending at the stream position `end` — the arm matching this terminator's own
    /// shape (the type docs). The stop-command arm's spans are the pieces
    /// [`text`](Self::text) composed the matched string from, laid out from `span`'s
    /// start in that same order; the post-space span is empty, the composed spelling
    /// having no gap to record.
    fn syntax_data(
        &self,
        span: SourceSpan<L::SourceOrigin>,
        end: StreamPosition<L>,
    ) -> EnvironmentTerminatorSyntaxData<L> {
        match self {
            VerbatimBodyTerminator::Literal { .. } => {
                EnvironmentTerminatorSyntaxData::Literal { span }
            }
            VerbatimBodyTerminator::StopEnvironmentCommand {
                escape_char,
                invocation_name,
                stop_command_name,
                name_group_rule,
            } => {
                // The matched string is the composed one byte for byte, so each
                // piece's extent follows from its length, in the source the whole
                // match was found in.
                let piece = |start: usize, len: usize| {
                    SourceSpan::new(span.source(), start..start + len)
                };
                let command_word_start = span.start() + escape_char.len_utf8();
                let command_word_end = command_word_start + stop_command_name.len();
                let name_start = command_word_end + name_group_rule.open.len();
                EnvironmentTerminatorSyntaxData::Scanned {
                    escape_char: *escape_char,
                    command_word: piece(command_word_start, stop_command_name.len()),
                    post_space: piece(command_word_end, 0),
                    // The pieces are sliced from the matched terminator's own
                    // single-token span, so the name span's content *is* the name
                    // under either span-tiling declaration: no name-as-read needed.
                    name_group: NameGroup::new(
                        piece(name_start, invocation_name.len()),
                        end,
                        Arc::clone(name_group_rule),
                    ),
                }
            }
        }
    }
}

// Manual impl: a derive would demand `L: Debug` although only an `Arc` to rule data
// is stored.
impl<L: Lang> fmt::Debug for VerbatimBodyTerminator<'_, L> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            VerbatimBodyTerminator::Literal { terminator } => {
                f.debug_struct("Literal").field("terminator", terminator).finish()
            }
            VerbatimBodyTerminator::StopEnvironmentCommand {
                escape_char,
                invocation_name,
                stop_command_name,
                name_group_rule,
            } => f
                .debug_struct("StopEnvironmentCommand")
                .field("escape_char", escape_char)
                .field("invocation_name", invocation_name)
                .field("stop_command_name", stop_command_name)
                .field("name_group_rule", name_group_rule)
                .finish(),
        }
    }
}

/// The verbatim environment-body parser: reads the body as **raw text** up to the
/// given `terminator` (cf. [`VerbatimBodyTerminator`]),
/// consumes the terminator, and stages the standard body `List`
/// with the content as one raw `Chars` node — a drop-in
/// [`EnvironmentBodyParser`](super::EnvironmentBodyParser) replacement for
/// `make_body_parser`-style spec hooks (it produces the same [`EnvironmentBody`]).
///
/// **Newline gobbling** (on by default): a newline immediately at
/// the body's start — the one right after `\begin{verbatim}` — is *staged* as a
/// leading whitespace `Chars` node but **designated out of the content**
/// ([`EnvironmentBody::content`]): trees keep every byte, content extraction starts at
/// the real first verbatim line. Disable via
/// [`with_gobble_leading_newline`](VerbatimBodyParser::with_gobble_leading_newline).
///
/// At end of input before the terminator, [`MissingEnvironmentTerminator`] is
/// diagnosed (anchored at the invocation trigger, like the tokenized body parser) and
/// the body closes at the input's end.
///
/// The consumed terminator's facts are reported back on
/// [`EnvironmentBody::terminator`] in the arm matching the terminator's own shape —
/// [`Literal`](EnvironmentTerminatorSyntaxData::Literal) for a literal string,
/// [`Scanned`](EnvironmentTerminatorSyntaxData::Scanned) for a stop command
/// ([`VerbatimBodyTerminator`]).
///
/// Requires a language with the groups feature ([`LangHasGroups`]): the terminator is
/// carried by a minted group rule installed as the expected group close.
pub struct VerbatimBodyParser<'p, L: Lang> {
    /// The invocation trigger's span (`\begin{verbatim}`'s command token), anchoring
    /// the missing-terminator diagnostic.
    trigger_span: SourceSpan<L::SourceOrigin>,
    /// The invocation name diagnostics call the environment.
    invocation_name: &'p str,
    /// The terminator specification.
    terminator: VerbatimBodyTerminator<'p, L>,
    /// The class of the minted expected-close rule (a language's verbatim group
    /// class); recorded nowhere — the body stages a `List`, not a group.
    group_type: L::GroupTypeId,
    /// Whether to gobble a single leading newline (default: `true`).
    gobble_leading_newline: bool,
    /// The span of the invocation name as written, for the body's traceback frame
    /// (mirrors [`EnvironmentBodyParser`](super::EnvironmentBodyParser)).
    invocation_name_span: Option<SourceSpan<L::SourceOrigin>>,
}

impl<'p, L: LangHasGroups> VerbatimBodyParser<'p, L> {
    /// A verbatim body parser for the environment invoked as `invocation_name`
    /// (trigger token span `trigger_span`), ended by `terminator`
    /// ([`VerbatimBodyTerminator`]), minting its expected-close rule under
    /// `group_type`.
    pub fn new(
        trigger_span: SourceSpan<L::SourceOrigin>,
        invocation_name: &'p str,
        terminator: VerbatimBodyTerminator<'p, L>,
        group_type: L::GroupTypeId,
    ) -> VerbatimBodyParser<'p, L> {
        VerbatimBodyParser {
            trigger_span,
            invocation_name,
            terminator,
            group_type,
            gobble_leading_newline: true,
            invocation_name_span: None,
        }
    }

    /// Set whether a single newline at the body's very start is designated out of the
    /// content (default: `true` — the `\begin{verbatim}`-line newline belongs to the
    /// environment's begin/end syntax, not to the verbatim text).
    pub fn with_gobble_leading_newline(mut self, gobble: bool) -> Self {
        self.gobble_leading_newline = gobble;
        self
    }

    /// Provide the span of the invocation name as written, so the body's traceback
    /// frame can quote it (`environment ‘verbatim’`).
    pub fn with_invocation_name_span(mut self, name_span: SourceSpan<L::SourceOrigin>) -> Self {
        self.invocation_name_span = Some(name_span);
        self
    }
}

impl<L: LangHasGroups> ConstructParser<L> for VerbatimBodyParser<'_, L> {
    type Output = EnvironmentBody<L>;

    fn parse(
        &mut self,
        cx: &mut ParseContext<'_, '_, L>,
    ) -> ConstructParserResult<L, (EnvironmentBody<L>, Option<Box<ParsingStateDelta<L>>>)> {
        // The same environment-body traceback frame as the tokenized parser.
        let title = match &self.invocation_name_span {
            Some(name_span) => {
                FrameTitle::Quoted { label: "environment", name: name_span.clone() }
            }
            None => FrameTitle::Static("environment body"),
        };
        let frame = Frame { title, span: self.trigger_span.clone() };
        cx.with_frame(frame, |cx| self.parse_body(cx))
    }
}

impl<L: LangHasGroups> VerbatimBodyParser<'_, L> {
    /// The body parse proper, run under the environment's traceback frame.
    // The return type is the `ConstructParser::parse` shape spelled out; an alias for
    // it would name one method's tuple, not a concept.
    #[allow(clippy::type_complexity)]
    fn parse_body(
        &mut self,
        cx: &mut ParseContext<'_, '_, L>,
    ) -> ConstructParserResult<L, (EnvironmentBody<L>, Option<Box<ParsingStateDelta<L>>>)> {
        let body_start = cx.tokens.position_here();
        // Whatever shape the terminator was given in, the body reads up to one raw
        // string ([`VerbatimBodyTerminator::text`]).
        let close_rule = Arc::new(GroupRule {
            group_type: self.group_type,
            // The rule exists solely as the expected-close carrier; the construct's
            // opener is the `\begin{name}` scaffolding the composition already read.
            open: String::new(),
            close: self.terminator.text(),
        });
        let verbatim_state = cx.derive_state(&verbatim_state_delta(close_rule))?;

        let mut children = Vec::new();

        // The gobble peek: a newline as the body's very first raw char.
        let mut content_designation_start = 0u32;
        if self.gobble_leading_newline {
            if let Some(token) = cx.probe_token(&verbatim_state)? {
                if matches!(cx.tokens.token_kind(&token), TokenKind::Char('\n')) {
                    cx.tokens.move_to(&token, TokenEdge::EndPastPostSpace);
                    let span = cx.tokens.source_span_of(&token);
                    let id = cx
                        .stage_node(
                            NodeKind::chars(span.span()),
                            span.clone(),
                            Arc::clone(&verbatim_state),
                            vec![],
                        )
                        .map_err(|error| cx.staging_error(error, span))?;
                    children.push(id);
                    content_designation_start = 1;
                }
            }
        }

        let content_start = cx.tokens.position_here();
        let raw_end = read_raw_content(cx, &verbatim_state, |_| false, |_| ())?;
        if raw_end.terminator.is_none() {
            cx.recover(
                MissingEnvironmentTerminator::new(
                    self.invocation_name,
                    MissingTerminatorFound::EndOfInput,
                ),
                self.trigger_span.clone(),
            )?;
        }

        let content_span = cx.source_span_within(&content_start, &raw_end.content_end)?;
        if let Some(content) = raw_content_text(&raw_end, &content_span) {
            let id = cx
                .stage_node(
                    NodeKind::chars(content),
                    content_span.clone(),
                    Arc::clone(&verbatim_state),
                    vec![],
                )
                .map_err(|error| cx.staging_error(error, content_span))?;
            children.push(id);
        }

        let child_count = children.len() as u32;
        let body_span = cx.source_span_within(&body_start, &raw_end.content_end)?;
        let body = cx
            .stage_node(
                NodeKind::list(),
                body_span.clone(),
                Arc::clone(&cx.state),
                children,
            )
            .map_err(|error| cx.staging_error(error, body_span))?;
        let end = raw_end.end.clone();
        Ok((
            EnvironmentBody {
                body,
                end: end.clone(),
                content: ContentNodes::InChildrenOf(body, content_designation_start..child_count),
                // The facts of the matched terminator, in the arm matching the shape
                // it was given in: a bare span for a literal, the standard
                // command-plus-name-group spelling for a stop command (no tokenized
                // scan exists either way — the pieces are the ones the terminator
                // string was composed from).
                terminator: raw_end
                    .terminator
                    .map(|span| self.terminator.syntax_data(span, end.clone())),
                // A raw body runs no content loop: no sibling construct parsed inside
                // it, so nothing evolved the state it was read under and nothing can
                // escape it. The honest facts are the entry state and an empty record.
                exit_state: Arc::clone(&cx.state),
                after_effects: None,
            },
            None,
        ))
    }
}

impl<L: Lang> fmt::Debug for VerbatimBodyParser<'_, L> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("VerbatimBodyParser")
            .field("trigger_span", &self.trigger_span)
            .field("invocation_name", &self.invocation_name)
            .field("terminator", &self.terminator)
            .field("group_type", &self.group_type)
            .field("gobble_leading_newline", &self.gobble_leading_newline)
            .field("invocation_name_span", &self.invocation_name_span)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use crate::source::Span;

    use super::super::{ChildStateSpec, NodesParser, StopCause, StopSpec};
    use super::*;
    use crate::engine::{
        CommandResolution, ParseDriver, ParseResult, ParserSession, ResolvedCallable,
    };
    use crate::error::{ParseError, Recovery};
    use crate::node::{check_tree_invariants, BuildId, NodeRef};
    use crate::scopes::{CallableQuery, CallableSyntax, Package, ScopeStack};
    use crate::constructs::tests::{relaxed_driver, RelaxedStdLang, RELAXED_MACRO};
    use crate::source::Source;
    use crate::spec::{CallableSpec, StdCallableSpec};
    use crate::state::StateData;
    use crate::token::{
        CommandRule, CommandRules, CommentRule, CommentRules, ForbiddenCharsRules, GroupRules,
        ParagraphRules, SpecialsRules, StdToken, StdTokenReader, TokenKind, TokenReader,
        TokenRules, WhitespaceRules,
    };
    use alloc::format;
    use alloc::string::ToString;
    use alloc::vec;

    const GT_BRACE: u32 = 0;
    const GT_VERB: u32 = 1;
    const CT_MACRO: u32 = 10;

    /// Test lang resolving `Command` tokens against the state's providers under the
    /// `CT_MACRO` form (the compact 6.4/6.5 suite pattern).
    #[derive(Debug, Clone, Copy)]
    struct VerbLang;
    impl Lang for VerbLang {
        type Features = crate::state::AllLangFeatures;
        type GroupTypeId = u32;
        type CallableTypeId = u32;
        type ModeId = ();
        type StateExt = ();
        type Event = ();
        type SessionExt = ();
        type SourceOrigin = Option<String>;
        type Tokenization = crate::token::StdTokenization;
        type NodeExts = ();
        type InvocationSyntax = ();
        type Driver = VerbDriver;
        fn make_node_ext(
            _kind: &crate::node::NodeKind<Self>,
            _span: &crate::source::SourceSpan<Self::SourceOrigin>,
            _state: &alloc::sync::Arc<crate::state::ParsingState<Self>>,
            _children: crate::node::StagedChildren<'_, Self>,
        ) -> Result<(), crate::node::NodeBuildError> {
            Ok(())
        }
    }

    #[derive(Debug, Clone, Copy)]
    struct VerbDriver {
        recovery: Recovery,
    }

    impl ParseDriver<VerbLang> for VerbDriver {
        fn recovery(&self) -> Recovery {
            self.recovery
        }

        fn resolve_command(
            &self,
            state: &ParsingState<VerbLang>,
            token: &StdToken<VerbLang>,
            tokens: &dyn TokenReader<'_, VerbLang>,
        ) -> Result<CommandResolution<VerbLang>, crate::error::ParseError> {
            let TokenKind::Command { name, escape_char } = tokens.token_kind(token) else {
                return Ok(CommandResolution::Unresolved { detail: None });
            };
            let query =
                CallableQuery::new(CT_MACRO, name, CallableSyntax::Command { escape_char });
            Ok(match state.scopes().retrieve_spec(&query, state) {
                Ok(resolved) => resolved
                    .map(|spec| ResolvedCallable { callable_type: CT_MACRO, spec })
                    .into(),
                Err(error) => {
                    CommandResolution::Unresolved { detail: Some(error.to_string()) }
                }
            })
        }
    }

    fn rules<L: Lang<GroupTypeId = u32, Features = crate::state::AllLangFeatures>>(
    ) -> TokenRules<L> {
        TokenRules {
            whitespace: WhitespaceRules { enabled: true, chars: " \t\n".into() },
            paragraphs: ParagraphRules { enabled: true },
            groups: GroupRules {
                enabled: true,
                rules: vec![Arc::new(GroupRule {
                    group_type: GT_BRACE,
                    open: "{".into(),
                    close: "}".into(),
                })],
                temporary: Vec::new(),
                expecting_close: None,
            },
            commands: CommandRules {
                enabled: true,
                rules: vec![Arc::new(CommandRule {
                    escape_char: '\\',
                    name_chars: "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ".into(),
                })],
            },
            comments: CommentRules {
                enabled: true,
                rules: vec![Arc::new(CommentRule { start: "%".into() })],
            },
            specials: SpecialsRules { enabled: true },
            forbidden_chars: ForbiddenCharsRules { chars: "".into() },
        }
    }

    fn plain_state() -> Arc<ParsingState<VerbLang>> {
        Arc::new(ParsingState::new(StateData {
            rules: rules(),
            scopes: ScopeStack::new(),
            mode: (),
            ext: (),
        }))
    }

    /// A state whose provider defines each named macro with the given argument specs.
    fn state_with(
        macros: &[(&str, Vec<Arc<ArgumentSpec<VerbLang>>>)],
    ) -> Arc<ParsingState<VerbLang>> {
        let mut package = Package::new("test-macros");
        for (name, arguments) in macros {
            let spec: Arc<dyn CallableSpec<VerbLang>> =
                Arc::new(StdCallableSpec { arguments: arguments.clone(), ..Default::default() });
            package.insert(CT_MACRO, *name, spec);
        }
        let mut scopes = ScopeStack::new();
        scopes.push(Arc::new(package));
        Arc::new(ParsingState::new(StateData { rules: rules(), scopes, mode: (), ext: () }))
    }

    fn verb_arg() -> Arc<ArgumentSpec<VerbLang>> {
        Arc::new(ArgumentSpec::new_unnamed(Arc::new(VerbatimArgumentParser::new(GT_VERB))))
    }

    fn verb_arg_with(parser: VerbatimArgumentParser<VerbLang>) -> Arc<ArgumentSpec<VerbLang>> {
        Arc::new(ArgumentSpec::new_unnamed(Arc::new(parser)))
    }

    fn brace_arg() -> Arc<ArgumentSpec<VerbLang>> {
        Arc::new(ArgumentSpec::new_unnamed(Arc::new(super::super::GroupArgumentParser::new(
            GT_BRACE,
        ))))
    }

    // --- harness: full content drive over `StdTokenReader` (verbatim re-tokenizes
    // --- under derived states, so only the scanning reader applies — [§dd-dr:tokens]) -----------

    fn try_parse(
        content: &str,
        state: &Arc<ParsingState<VerbLang>>,
        recovery: Recovery,
    ) -> Result<ParseResult<VerbLang>, ParseError> {
        let source: Arc<Source> = Arc::new(Source::new(content));
        let mut reader = StdTokenReader::new(&source);
        let mut session = ParserSession::new();
        let driver = VerbDriver { recovery };
        let mut cx = ParseContext::new(
            &mut reader,
            Arc::clone(state),
            &mut session,
            &driver);
        let mut parser = NodesParser::new(StopSpec::none())
            .with_child_states(ChildStateSpec::inherit());
        let (outcome, delta) = parser.parse(&mut cx)?;
        assert_eq!(outcome.stop, StopCause::EndOfInput);
        assert!(delta.is_none());
        let root_span = {
            let staged = session.builder.staged_nodes();
            match (outcome.nodes.first(), outcome.nodes.last()) {
                (Some(&first), Some(&last)) => Span::new(
                    staged.get(first).unwrap().span().start(),
                    staged.get(last).unwrap().span().end(),
                ),
                _ => Span::empty(0),
            }
        };
        let root = session
            .builder
            .add(
                NodeKind::list(),
                SourceSpan::new(&source, root_span),
                Arc::clone(state),
                outcome.nodes, (), (),
            )
            .unwrap();
        let result = session.finish(root).unwrap();
        check_tree_invariants(&result.tree);
        Ok(result)
    }

    fn parse(
        content: &str,
        state: &Arc<ParsingState<VerbLang>>,
        recovery: Recovery,
    ) -> ParseResult<VerbLang> {
        try_parse(content, state, recovery).expect("parse")
    }

    fn root_child(result: &ParseResult<VerbLang>, i: usize) -> NodeRef<'_, VerbLang> {
        result.tree.root().child(i).expect("root child")
    }

    /// The verbatim group node of `\verb`-style argument 0 plus its raw text.
    fn verb_group(node: NodeRef<'_, VerbLang>) -> NodeRef<'_, VerbLang> {
        let region: Vec<_> = node.argument_nodes(0).expect("provided argument").iter().collect();
        *region.last().expect("the group node ends the region")
    }

    fn verbatim_text<'t>(node: NodeRef<'t, VerbLang>) -> Option<&'t str> {
        let content: Vec<_> = node.argument_content_nodes(0)?.iter().collect();
        match content.as_slice() {
            [] => Some(""),
            [chars] => chars.chars(),
            _ => panic!("verbatim content is at most one chars node"),
        }
    }

    /// Pin of the recorded verbatim state: every tokenization feature off (the
    /// pylatexenc `vps.enable_*` assertions).
    fn assert_verbatim_state(node: NodeRef<'_, VerbLang>) {
        let rules = node.parsing_state().rules();
        assert!(!rules.whitespace_enabled());
        assert!(!rules.paragraphs_enabled());
        assert!(!rules.groups_enabled());
        assert!(!rules.commands_enabled());
        assert!(!rules.comments_enabled());
        assert!(!rules.specials_enabled());
        assert!(rules.expecting_group_close().is_some());
    }

    // --- the delimited form (`\verb`) — pylatexenc test_latexnodes_parsers_verbatim
    // --- TestLatexDelimitedVerbatimParser, adapted to invocation position ------------

    #[test]
    fn simple_delimiters() {
        let st = state_with(&[("verb", vec![verb_arg()])]);
        let result = parse(r"\verb|verbatim| x", &st, Recovery::Strict);
        assert!(result.diagnostics.is_empty());

        let verb = root_child(&result, 0);
        assert_eq!(verb.name(), Some("verb"));
        assert_eq!(verb.span().range(), 0..15);
        let group = verb_group(verb);
        assert_eq!(group.span().range(), 5..15);
        assert_eq!(group.group_type(), Some(GT_VERB));
        assert_eq!(group.group_delimiters(), Some(("|", "|")));
        assert_eq!(verbatim_text(verb), Some("verbatim"));
        let chars = group.child(0).unwrap();
        assert_eq!(chars.span().range(), 6..14);
        assert_verbatim_state(chars);
        // The group wrapper records the surrounding (fully enabled) state.
        assert!(group.parsing_state().rules().commands_enabled());
        assert_eq!(root_child(&result, 1).chars(), Some(" x"));
    }

    #[test]
    fn raw_content_ignores_every_recognizer() {
        // pylatexenc's test_special_contents: escapes, comments, a paragraph break,
        // specials — all raw chars between `<` and the auto-matched `>`.
        let text = "<\\$%*~+\n\n%\\)\nverbatim>";
        let st = state_with(&[("verb", vec![verb_arg()])]);
        let result = parse(&format!("\\verb{text}"), &st, Recovery::Strict);
        assert!(result.diagnostics.is_empty());

        let verb = root_child(&result, 0);
        let group = verb_group(verb);
        assert_eq!(group.group_delimiters(), Some(("<", ">")));
        assert_eq!(verbatim_text(verb), Some(&text[1..text.len() - 1]));
    }

    #[test]
    fn a_language_forbidden_char_is_ordinary_raw_content() {
        // Ruled 2026-08-10: `disable_all()` clears the forbidden set (a gateless
        // feature's off is its inactive data), so a character outlawed outside the
        // region reads as raw verbatim content instead of ending the region early
        // with a diagnostic.
        let st = {
            let spec: Arc<dyn CallableSpec<VerbLang>> =
                Arc::new(StdCallableSpec { arguments: vec![verb_arg()], ..Default::default() });
            let mut package = Package::new("test-macros");
            package.insert(CT_MACRO, "verb", spec);
            let mut scopes = ScopeStack::new();
            scopes.push(Arc::new(package));
            let mut rules = rules::<VerbLang>();
            rules.forbidden_chars.chars = "$".into();
            Arc::new(ParsingState::new(StateData { rules, scopes, mode: (), ext: () }))
        };
        let result = parse(r"\verb|a$b| x", &st, Recovery::Strict);
        assert!(result.diagnostics.is_empty());

        let verb = root_child(&result, 0);
        assert_eq!(verbatim_text(verb), Some("a$b"));
    }

    #[test]
    fn whitespace_before_the_delimiter_is_region_noise() {
        // In invocation position the trigger's own post-space swallows `\verb |x|`'s
        // gap, so exercise pre-delimiter whitespace behind a first argument.
        let st = state_with(&[("lst", vec![brace_arg(), verb_arg()])]);
        let result = parse(r"\lst{a} |xy|", &st, Recovery::Strict);
        assert!(result.diagnostics.is_empty());

        let lst = root_child(&result, 0);
        let region: Vec<_> = lst.argument_nodes(1).unwrap().iter().collect();
        assert_eq!(region.len(), 2);
        assert_eq!(region[0].chars(), Some(" "));
        assert_eq!(region[1].group_delimiters(), Some(("|", "|")));
        let content: Vec<_> = lst.argument_content_nodes(1).unwrap().iter().collect();
        assert_eq!(content.len(), 1);
        assert_eq!(content[0].chars(), Some("xy"));
    }

    #[test]
    fn a_paragraph_break_may_precede_the_delimiter() {
        // pylatexenc `skip_space_chars` parity: the pre-delimiter skip has no
        // paragraph rule — `\n\n` is plain noise ahead of the region.
        let st = state_with(&[("lst", vec![brace_arg(), verb_arg()])]);
        let result = parse("\\lst{a}\n\n|xy|", &st, Recovery::Strict);
        assert!(result.diagnostics.is_empty());
        let lst = root_child(&result, 0);
        let region: Vec<_> = lst.argument_nodes(1).unwrap().iter().collect();
        assert_eq!(region[0].chars(), Some("\n\n"));
        assert_eq!(verbatim_text_at(lst, 1), Some("xy"));
    }

    fn verbatim_text_at(node: NodeRef<'_, VerbLang>, i: usize) -> Option<&str> {
        let content: Vec<_> = node.argument_content_nodes(i)?.iter().collect();
        content.first().and_then(|chars| chars.chars())
    }

    #[test]
    fn auto_matched_pairs_and_the_depth_counter() {
        let st = state_with(&[("verb", vec![verb_arg()])]);

        // `{` auto-matches `}` (pylatexenc test_curlybrace_delimiters)...
        let result = parse(r"\verb{verbatim}", &st, Recovery::Strict);
        let verb = root_child(&result, 0);
        assert_eq!(verb_group(verb).group_delimiters(), Some(("{", "}")));
        assert_eq!(verbatim_text(verb), Some("verbatim"));

        // ...and nested pairs balance through the depth counter.
        let result = parse(r"\verb{a{b{c}}d}e", &st, Recovery::Strict);
        assert!(result.diagnostics.is_empty());
        let verb = root_child(&result, 0);
        assert_eq!(verbatim_text(verb), Some("a{b{c}}d"));
        assert_eq!(root_child(&result, 1).chars(), Some("e"));

        // Identical delimiters have no nesting: the first closer ends the region.
        let result = parse(r"\verb|a|b", &st, Recovery::Strict);
        assert_eq!(verbatim_text(root_child(&result, 0)), Some("a"));
    }

    #[test]
    fn verbatim_inside_an_enclosing_group() {
        // The probe delta *clears* the inherited close expectation, and the content
        // delta *replaces* it: `}` works both as delimiter and as raw content inside
        // a braces group; the group still finds its own close after the region.
        let st = state_with(&[("verb", vec![verb_arg()])]);

        let result = parse(r"{\verb|}|}", &st, Recovery::Strict);
        assert!(result.diagnostics.is_empty());
        let group = root_child(&result, 0);
        assert_eq!(group.group_delimiters(), Some(("{", "}")));
        let verb = group.child(0).unwrap();
        assert_eq!(verbatim_text(verb), Some("}"));

        let result = parse(r"{\verb}x}}", &st, Recovery::Strict);
        assert!(result.diagnostics.is_empty());
        let verb = root_child(&result, 0).child(0).unwrap();
        assert_eq!(verb_group(verb).group_delimiters(), Some(("}", "}")));
        assert_eq!(verbatim_text(verb), Some("x"));
    }

    #[test]
    fn empty_content_stages_no_chars_node() {
        let st = state_with(&[("verb", vec![verb_arg()])]);
        let result = parse(r"\verb||", &st, Recovery::Strict);
        assert!(result.diagnostics.is_empty());
        let verb = root_child(&result, 0);
        let group = verb_group(verb);
        assert_eq!(group.child_count(), 0);
        assert_eq!(verbatim_text(verb), Some(""));
    }

    #[test]
    fn fixed_delimiters() {
        // pylatexenc test_simple_delimiters_required_delims: `{`…`>` prescribed.
        let st = state_with(&[(
            "verb",
            vec![verb_arg_with(VerbatimArgumentParser::new(GT_VERB).with_delimiters('{', '>'))],
        )]);
        let result = parse(r"\verb{verbatim>", &st, Recovery::Strict);
        assert!(result.diagnostics.is_empty());
        let verb = root_child(&result, 0);
        assert_eq!(verb_group(verb).group_delimiters(), Some(("{", ">")));
        assert_eq!(verbatim_text(verb), Some("verbatim"));
    }

    #[test]
    fn custom_auto_delimiter_table() {
        // pylatexenc test_simple_delimiters_auto_delims: '`' auto-matches '\''.
        let st = state_with(&[(
            "verb",
            vec![verb_arg_with(
                VerbatimArgumentParser::new(GT_VERB)
                    .with_auto_delimiters([('{', '}'), ('<', '>'), ('`', '\'')]),
            )],
        )]);
        let result = parse(r"\verb`verbatim'", &st, Recovery::Strict);
        assert!(result.diagnostics.is_empty());
        let verb = root_child(&result, 0);
        assert_eq!(verb_group(verb).group_delimiters(), Some(("`", "'")));
        assert_eq!(verbatim_text(verb), Some("verbatim"));
    }

    #[test]
    fn wrong_fixed_delimiter_reports_the_argument_absent() {
        let st = state_with(&[(
            "verb",
            vec![verb_arg_with(VerbatimArgumentParser::new(GT_VERB).with_delimiters('{', '}'))],
        )]);

        let err = try_parse(r"\verb|x|", &st, Recovery::Strict).unwrap_err();
        assert!(err.to_string().contains("expected opening delimiter ‘{’"), "{err}");

        let result = parse(r"\verb|x|", &st, Recovery::Tolerant);
        assert_eq!(result.diagnostics.len(), 1);
        let verb = root_child(&result, 0);
        assert!(!verb.arguments().unwrap().get(0).unwrap().is_provided());
        // Nothing consumed: `|x|` re-parses as sibling content.
        assert_eq!(root_child(&result, 1).chars(), Some("|x|"));
    }

    #[test]
    fn missing_delimiter_at_end_of_input() {
        let st = state_with(&[("verb", vec![verb_arg()])]);

        let err = try_parse(r"\verb", &st, Recovery::Strict).unwrap_err();
        assert!(err.to_string().contains("expected a verbatim opening delimiter"), "{err}");

        let result = parse(r"\verb", &st, Recovery::Tolerant);
        assert_eq!(result.diagnostics.len(), 1);
        assert!(!root_child(&result, 0).arguments().unwrap().get(0).unwrap().is_provided());
    }

    #[test]
    fn unterminated_verbatim_recovers_with_an_empty_close() {
        let st = state_with(&[("verb", vec![verb_arg()])]);

        let err = try_parse(r"\verb|abc", &st, Recovery::Strict).unwrap_err();
        assert!(err.to_string().contains("missing closing delimiter ‘|’"), "{err}");

        let result = parse(r"\verb|abc", &st, Recovery::Tolerant);
        assert_eq!(result.diagnostics.len(), 1);
        let verb = root_child(&result, 0);
        let group = verb_group(verb);
        assert_eq!(group.group_delimiters(), Some(("|", "")));
        assert_eq!(group.span().range(), 5..9);
        assert_eq!(verbatim_text(verb), Some("abc"));
    }

    // --- the environment-contents form (`VerbatimBodyParser`), driven directly at the
    // --- body position — pylatexenc TestLatexVerbatimEnvironmentContentsParser -------

    struct BodyRun {
        result: ParseResult<VerbLang>,
        /// Where the environment ends, as the body parser reported it.
        end: crate::token::StdStreamPosition,
        /// The same place as a byte offset, for the numeric assertions.
        end_offset: usize,
        content: ContentNodes,
        body_id: BuildId,
        terminator: Option<EnvironmentTerminatorSyntaxData<VerbLang>>,
    }

    /// `\end{verbatim}` spelled out as one literal string.
    fn literal_terminator() -> VerbatimBodyTerminator<'static, VerbLang> {
        VerbatimBodyTerminator::Literal { terminator: "\\end{verbatim}".into() }
    }

    /// The same `\end{verbatim}` given piecewise, as the preset's environment
    /// composition gives it: `\` + `end` + the `{…}` name group around `verbatim`.
    fn command_terminator() -> VerbatimBodyTerminator<'static, VerbLang> {
        VerbatimBodyTerminator::StopEnvironmentCommand {
            escape_char: '\\',
            invocation_name: "verbatim",
            stop_command_name: "end",
            name_group_rule: Arc::new(GroupRule {
                group_type: GT_BRACE,
                open: "{".into(),
                close: "}".into(),
            }),
        }
    }

    fn run_body(
        content: &str,
        recovery: Recovery,
        gobble: bool,
    ) -> Result<BodyRun, ParseError> {
        run_body_with(content, recovery, gobble, literal_terminator())
    }

    fn run_body_with(
        content: &str,
        recovery: Recovery,
        gobble: bool,
        terminator: VerbatimBodyTerminator<'_, VerbLang>,
    ) -> Result<BodyRun, ParseError> {
        let source: Arc<Source> = Arc::new(Source::new(content));
        let state = plain_state();
        let mut reader = StdTokenReader::new(&source);
        let mut session = ParserSession::new();
        let driver = VerbDriver { recovery };
        let mut cx = ParseContext::new(
            &mut reader,
            Arc::clone(&state),
            &mut session,
            &driver);
        let mut parser =
            VerbatimBodyParser::new(
                SourceSpan::new(&source, 0..0),
                "verbatim",
                terminator,
                GT_VERB,
            )
            .with_gobble_leading_newline(gobble);
        let (body, delta) = parser.parse(&mut cx)?;
        assert!(delta.is_none());
        let end_offset = cx.tokens.source_position_at(&body.end).pos();
        let span = {
            let staged = session.builder.staged_nodes();
            staged.get(body.body).unwrap().span().range()
        };
        let root = session
            .builder
            .add(
                NodeKind::list(),
                SourceSpan::new(&source, span),
                Arc::clone(&state),
                vec![body.body], (), (),
            )
            .unwrap();
        let result = session.finish(root).unwrap();
        check_tree_invariants(&result.tree);
        Ok(BodyRun {
            result,
            end: body.end,
            end_offset,
            content: body.content,
            body_id: body.body,
            terminator: body.terminator,
        })
    }

    #[test]
    fn environment_contents_gobble_and_terminator() {
        // pylatexenc test_simple: the fragment right after `\begin{verbatim}`.
        let text = "\nHello world.\\\n\\macro, \\begin! This: % is not a comment; ~. all\n\\end{verbatim}\n";
        let evpos = text.find("\\end{verbatim}").unwrap();
        let run = run_body(text, Recovery::Strict, true).unwrap();
        assert!(run.result.diagnostics.is_empty());

        // The body list keeps every byte: the gobbled newline node, then the content.
        let list = run.result.tree.root().child(0).unwrap();
        assert_eq!(list.span().range(), 0..evpos);
        assert_eq!(list.child_count(), 2);
        assert_eq!(list.child(0).unwrap().chars(), Some("\n"));
        assert_eq!(list.child(1).unwrap().chars(), Some(&text[1..evpos]));
        assert_verbatim_state(list.child(1).unwrap());

        // The designation excludes the gobbled newline; `end` lies past the
        // terminator (the trailing "\n" stays enclosing content).
        assert_eq!(run.content, ContentNodes::InChildrenOf(run.body_id, 1..2));
        assert_eq!(run.end_offset, evpos + "\\end{verbatim}".len());
    }

    #[test]
    fn environment_contents_at_stream_end() {
        // pylatexenc test_simple_nofinaleol: EOF immediately after the terminator.
        let text = "\nHello.\n\\end{verbatim}";
        let evpos = text.find("\\end{verbatim}").unwrap();
        let run = run_body(text, Recovery::Strict, true).unwrap();
        assert!(run.result.diagnostics.is_empty());
        let list = run.result.tree.root().child(0).unwrap();
        assert_eq!(list.child(1).unwrap().chars(), Some(&text[1..evpos]));
        assert_eq!(run.end_offset, text.len());
    }

    #[test]
    fn environment_contents_without_a_leading_newline_gobble_nothing() {
        let text = "xyz\n\\end{verbatim}";
        let run = run_body(text, Recovery::Strict, true).unwrap();
        let list = run.result.tree.root().child(0).unwrap();
        assert_eq!(list.child_count(), 1);
        assert_eq!(list.child(0).unwrap().chars(), Some("xyz\n"));
        assert_eq!(run.content, ContentNodes::InChildrenOf(run.body_id, 0..1));
    }

    #[test]
    fn environment_contents_gobble_disabled() {
        let text = "\nxyz\\end{verbatim}";
        let run = run_body(text, Recovery::Strict, false).unwrap();
        let list = run.result.tree.root().child(0).unwrap();
        assert_eq!(list.child_count(), 1);
        assert_eq!(list.child(0).unwrap().chars(), Some("\nxyz"));
        assert_eq!(run.content, ContentNodes::InChildrenOf(run.body_id, 0..1));
    }

    #[test]
    fn environment_contents_missing_terminator() {
        let text = "\nno end in sight";
        let err = run_body(text, Recovery::Strict, true).err().expect("strict abort");
        assert!(err.to_string().contains("missing terminator of environment ‘verbatim’"), "{err}");

        let run = run_body(text, Recovery::Tolerant, true).unwrap();
        assert_eq!(run.result.diagnostics.len(), 1);
        let list = run.result.tree.root().child(0).unwrap();
        assert_eq!(list.child(1).unwrap().chars(), Some(&text[1..]));
        assert_eq!(run.end_offset, text.len());
        // Nothing was consumed, so no terminator facts are reported.
        assert!(run.terminator.is_none());
    }

    // --- the terminator's two shapes: same bytes read, different facts reported -----

    #[test]
    fn a_literal_terminator_reports_its_span_alone() {
        let text = "\nxyz\n\\end{verbatim}";
        let evpos = text.find("\\end{verbatim}").unwrap();
        let run = run_body_with(text, Recovery::Strict, true, literal_terminator()).unwrap();
        assert!(run.result.diagnostics.is_empty());
        match run.terminator.expect("a consumed terminator") {
            EnvironmentTerminatorSyntaxData::Literal { span } => {
                assert_eq!(span.range(), evpos..text.len());
            }
            other => panic!("expected literal terminator facts, got {other:?}"),
        }
    }

    #[test]
    fn a_stop_command_terminator_reads_the_same_bytes() {
        // The composed `\end{verbatim}` is matched exactly as the literal one is:
        // same content read, same end position.
        let text = "\nHello.\n\\end{verbatim}\n";
        let evpos = text.find("\\end{verbatim}").unwrap();
        let run = run_body_with(text, Recovery::Strict, true, command_terminator()).unwrap();
        assert!(run.result.diagnostics.is_empty());
        let list = run.result.tree.root().child(0).unwrap();
        assert_eq!(list.child(1).unwrap().chars(), Some(&text[1..evpos]));
        assert_eq!(run.end_offset, evpos + "\\end{verbatim}".len());
    }

    #[test]
    fn a_stop_command_terminator_reports_scanned_facts() {
        //                       0....5....1....1....2
        //                            0    5    0
        let text = "\nxyz\n\\end{verbatim}";
        let evpos = text.find("\\end{verbatim}").unwrap();
        let run = run_body_with(text, Recovery::Strict, true, command_terminator()).unwrap();
        assert!(run.result.diagnostics.is_empty());
        match run.terminator.expect("a consumed terminator") {
            EnvironmentTerminatorSyntaxData::Scanned {
                escape_char,
                command_word,
                post_space,
                name_group,
            } => {
                // The pieces the terminator string was composed from, laid back out
                // over the matched span: `\` `end` `{` `verbatim` `}`, no gap.
                assert_eq!(escape_char, '\\');
                assert_eq!(&text[command_word.range()], "end");
                assert_eq!(post_space.range(), command_word.end()..command_word.end());
                assert_eq!(name_group.name_text(), "verbatim");
                assert_eq!(*name_group.end(), run.end);
                assert_eq!(name_group.rule().group_type, GT_BRACE);
                assert_eq!((&*name_group.rule().open, &*name_group.rule().close), ("{", "}"));
                // The whole terminator is exactly the span the pieces tile.
                assert_eq!(
                    command_word.start() - escape_char.len_utf8(),
                    evpos
                );
            }
            other => panic!("expected scanned terminator facts, got {other:?}"),
        }
    }

    #[test]
    fn a_stop_command_terminator_tolerates_no_whitespace() {
        // The composed string is matched byte for byte: `\end {verbatim}` is not it.
        let text = "\nxyz\n\\end {verbatim}";
        let run = run_body_with(text, Recovery::Tolerant, true, command_terminator()).unwrap();
        assert_eq!(run.result.diagnostics.len(), 1);
        assert!(run.terminator.is_none());
        let list = run.result.tree.root().child(0).unwrap();
        assert_eq!(list.child(1).unwrap().chars(), Some(&text[1..]));
    }

    #[test]
    fn the_recipe_delta_is_reusable() {
        // `verbatim_state_delta` = the pinned recipe as data: derived states read raw
        // chars and the terminator as one GroupClose, whatever the base rules.
        let state = plain_state();
        let rule = Arc::new(GroupRule::<VerbLang> {
            group_type: GT_VERB,
            open: String::new(),
            close: "@@end".into(),
        });
        let derived = state.derived(&verbatim_state_delta(Arc::clone(&rule))).unwrap();
        assert!(!derived.rules().commands_enabled());
        assert_eq!(
            derived.rules().expecting_group_close().map(|r| r.close.as_str()),
            Some("@@end")
        );
    }
    // --- a language that does not obey span tiling (PLAN §1.5 R4) ---------------------

    // The shared relaxed language resolves commands under the same callable type id
    // this suite uses, so its macros can be defined the usual way.
    const _: () = assert!(CT_MACRO == RELAXED_MACRO);

    /// Raw verbatim content is multi-token content: for a language with
    /// `OBEYS_SPAN_TILING = false` it is recorded as the text the reader answered,
    /// token by token, and reads back exactly as the tiled parse's span slice does.
    #[test]
    fn verbatim_content_is_owned_where_the_language_does_not_obey_span_tiling() {
        // The tiled parse of the same input, for comparison.
        let tiled = parse(
            "\\verb|a b|",
            &state_with(&[("verb", vec![verb_arg()])]),
            Recovery::Strict,
        );
        let tiled_content = verb_content(root_child(&tiled, 0));
        assert_eq!(tiled_content.chars(), Some("a b"));
        assert!(matches!(
            tiled_content.kind(),
            NodeKind::Chars { content: TextContent::Spanned(_), .. }
        ));

        let spec: Arc<dyn CallableSpec<RelaxedStdLang>> =
            Arc::new(StdCallableSpec::new([ArgumentSpec::new_unnamed(Arc::new(
                VerbatimArgumentParser::new(GT_VERB),
            ))]));
        let mut package = Package::new("test-macros");
        package.insert(CT_MACRO, "verb", spec);
        let mut scopes = ScopeStack::new();
        scopes.push(Arc::new(package));
        let seed = Arc::new(ParsingState::new(StateData {
            rules: rules::<RelaxedStdLang>(),
            scopes,
            mode: (),
            ext: (),
        }));
        let language = crate::engine::Language::new(relaxed_driver(Recovery::Strict), seed);
        let relaxed = language.parse("\\verb|a b|").expect("the parse runs");

        let content = verb_content(relaxed.tree.root().child(0).expect("the callable"));
        assert_eq!(content.chars(), Some("a b"));
        assert!(
            matches!(content.kind(), NodeKind::Chars { content: TextContent::Owned(_), .. }),
            "the content of a relaxed verbatim parse is owned text"
        );
        // The group's delimiters are single-token facts: the node-data rule keeps them
        // as spans of the node's own source here (one source, so they lie in it).
        let group = content.parent().expect("the verbatim group");
        let data = group.group().expect("a group node");
        assert_eq!(data.open.resolve(group.source()), "|");
        crate::node::validate_tree(&relaxed.tree).expect("the all-trees law holds");
    }

    /// The same, for the two content arms the per-token recipe has to get right: a
    /// nested close read as content by the pairing rule, and characters that reach the
    /// terminator's own pre-space position. Both must read back as the tiled parse's
    /// span slice.
    #[test]
    fn relaxed_verbatim_content_covers_the_nested_close_and_trailing_arms() {
        let spec: Arc<dyn CallableSpec<RelaxedStdLang>> =
            Arc::new(StdCallableSpec::new([ArgumentSpec::new_unnamed(Arc::new(
                VerbatimArgumentParser::new(GT_VERB),
            ))]));
        let mut package = Package::new("test-macros");
        package.insert(CT_MACRO, "verb", spec);
        let mut scopes = ScopeStack::new();
        scopes.push(Arc::new(package));
        let seed = Arc::new(ParsingState::new(StateData {
            rules: rules::<RelaxedStdLang>(),
            scopes,
            mode: (),
            ext: (),
        }));
        let language = crate::engine::Language::new(relaxed_driver(Recovery::Strict), seed);

        for (input, expected) in [
            // Paired delimiters: the inner `}` is ordinary content (the pairing rule),
            // so the loop's group-close arm contributes its spelling to the text.
            (r"\verb{a{b}c}", "a{b}c"),
            // Characters up to the closing delimiter, whitespace included — the
            // verbatim state reads them as plain `Char` tokens.
            (r"\verb|ab  |", "ab  "),
        ] {
            let tiled = parse(
                input,
                &state_with(&[("verb", vec![verb_arg()])]),
                Recovery::Strict,
            );
            assert_eq!(verb_content(root_child(&tiled, 0)).chars(), Some(expected));

            let relaxed = language.parse(input).expect("the parse runs");
            let content = verb_content(relaxed.tree.root().child(0).expect("the callable"));
            assert_eq!(content.chars(), Some(expected), "relaxed content of {input:?}");
            assert!(
                matches!(content.kind(), NodeKind::Chars { content: TextContent::Owned(_), .. }),
                "the content of a relaxed verbatim parse is owned text"
            );
            crate::node::validate_tree(&relaxed.tree).expect("the all-trees law holds");
        }
    }

    /// The raw-content `Chars` child of a `\verb`-style callable's verbatim group.
    fn verb_content<L: Lang>(node: NodeRef<'_, L>) -> NodeRef<'_, L> {
        let region: Vec<_> = node.argument_nodes(0).expect("provided argument").iter().collect();
        let group = *region.last().expect("the group node ends the region");
        group.child(0).expect("the raw content")
    }
}
