//! The parser for a mandatory argument that a fixed character sequence ends:
//! [`TerminatedArgumentParser`], with its two conditions [`ExpectedArgumentOpener`]
//! and [`MissingArgumentTerminator`].
//!
//! The argument's content is read by the standard content loop under the argument's
//! own parsing state, with the terminator installed as the expected group close — the
//! same mechanism a group interior uses to find its close delimiter, and the one the
//! verbatim parsers use to find their terminator. Whether the content is parsed or
//! read as raw text is therefore decided by the argument's state alone, never by this
//! parser.

use alloc::format;
use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec;
use core::fmt;

use crate::error::DiagnosticInfo;
use crate::node::{ArgumentExt, ContentNodes, GroupData, NodeKind};
use crate::source::{SourceSpan, TextContent};
use crate::spec::{ArgumentParser, ArgumentSpec, ParsedArgumentNodes};
use crate::state::{GroupOverrides, Lang, LangHasGroups, ParsingStateDelta, TokenRulesOverrides};
use crate::token::{GroupRule, StreamPosition, TokenEdge, TokenKind};

use super::child_state::ChildStateSpec;
use super::environment_parser::MissingTerminatorFound;
use super::nodes_parser::{StopCause, StopSpec, TokenStopKind};
use super::{node_text_content, ConstructParserResult, FromInvocation, ParseContext};

/// Condition: the beginning of a [`TerminatedArgumentParser`] argument was not
/// there.
///
/// Raised when the configured opener does not follow the run of repeat characters
/// (another character stands there, or the input ends), and when an argument whose
/// terminator consists of the repeated characters alone starts with none of them.
/// [`expected`](ExpectedArgumentOpener::expected) is the opener in the first case and
/// the repeat character in the second.
///
/// Recovery reports the argument absent, consuming nothing: what the parser read
/// while looking for the opener is parsed again as content of the enclosing
/// construct.
#[derive(Debug, Clone, PartialEq, Eq, DiagnosticInfo)]
#[non_exhaustive]
#[diagnostic(
    id = "core.arguments.expected-opener",
    message = "expected ‘{expected}’ to open the argument"
)]
pub struct ExpectedArgumentOpener {
    /// What was expected where the argument starts: the opener, or the repeat
    /// character.
    pub expected: String,
}

/// Condition: the terminator of a [`TerminatedArgumentParser`] argument never
/// appeared.
///
/// [`found`](MissingArgumentTerminator::found) says what ended the argument instead:
///
/// - [`EndOfInput`](MissingTerminatorFound::EndOfInput): the input ended inside the
///   argument;
/// - [`StrayGroupClose`](MissingTerminatorFound::StrayGroupClose): in parsed content,
///   a group close delimiter appeared that nothing inside the argument had opened.
///
/// This parser never reports
/// [`UnreadableToken`](MissingTerminatorFound::UnreadableToken): its content is read
/// by the standard content loop, which applies its own recovery to a token it cannot
/// read and carries on.
///
/// Tolerant recovery keeps the content read so far, closes the argument's group node
/// where the reading stopped, and records an **empty** close on it — the
/// [`GroupData::close`] convention for a delimiter that was never found. A stray close
/// delimiter is not consumed: it is left for the enclosing level to claim, or for the
/// root, which diagnoses it separately.
#[derive(Debug, Clone, PartialEq, Eq, DiagnosticInfo)]
#[non_exhaustive]
#[diagnostic(id = "core.arguments.missing-terminator")]
pub struct MissingArgumentTerminator {
    /// The terminator that never appeared, as composed for this argument (for
    /// instance `\endphf`, or two backticks for a code span opened with two).
    pub terminator: String,
    /// What ended the argument instead.
    pub found: MissingTerminatorFound,
}

// Hand-written wording: the message varies by what ended the argument (a match, which
// the message format string cannot express).
impl fmt::Display for MissingArgumentTerminator {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.found {
            MissingTerminatorFound::EndOfInput => write!(
                f,
                "missing terminator ‘{}’ of the argument before end of input",
                self.terminator
            ),
            MissingTerminatorFound::StrayGroupClose => {
                write!(
                    f,
                    "missing terminator ‘{}’ of the argument",
                    self.terminator
                )
            }
            MissingTerminatorFound::UnreadableToken => write!(
                f,
                "missing terminator ‘{}’ of the argument before a token that could not \
                 be read",
                self.terminator
            ),
        }
    }
}

/// Parses a mandatory argument that runs up to a fixed character sequence, its
/// **terminator**: `\phf … \endphf`, a Markdown-style code span `` `…` ``, a Rust-style
/// raw string `##"…"##`.
///
/// The argument is written as three consecutive parts:
///
/// 1. the **prefix**: a run of copies of the *repeat character* — as many as there are,
///    possibly none, and only when a repeat character is configured — followed by the
///    *opener*, a fixed string that is empty by default;
/// 2. the **content**;
/// 3. the **terminator**: the *terminator head* (a fixed string, empty by default), then
///    as many copies of the repeat character as the prefix had, then the *terminator
///    tail* — the string given to [`new`](TerminatedArgumentParser::new).
///
/// | syntax | configuration |
/// |---|---|
/// | `\phf … \endphf` | `new(gt, r"\endphf")` |
/// | `` `x` `` and ``` ``a`b`` ```, after a specials trigger that reads one backtick | ``new(gt, "`").with_repeat_char('`')`` |
/// | `\verbcode+++…+++` | `new(gt, "").with_repeat_char('+')` |
/// | `\raw###"…"###` | `new(gt, "").with_repeat_char('#').with_opener("\"").with_terminator_head("\"")` |
/// | `[==[…]==]`, after a specials trigger that reads the first `[` | `new(gt, "]").with_repeat_char('=').with_opener("[").with_terminator_head("]")` |
/// | `\q<<…>>` | `new(gt, ">>").with_opener("<<")` |
///
/// In the code span row the trigger has already read the first backtick, and the
/// argument counts the remaining ones: the terminator is the tail backtick plus one
/// backtick for each counted one, as many backticks as opened the span. In the
/// `\verbcode` row the run of repeat characters is the whole terminator, so at least one
/// `+` is required.
///
/// # Where the argument starts
///
/// Nothing is skipped before the prefix: no whitespace and no comment is read as noise
/// ahead of the argument, as other argument parsers do. With an empty prefix there is
/// nothing to skip to, and a space at the start may be content — in the code span
/// `` ` x` `` it is. Whitespace that the token reader counts as part of the preceding
/// token is not content, though: under the standard reader the spaces after a command
/// name belong to the command, so `\phf hello\endphf` has the content `hello`.
///
/// # Parsed or raw content
///
/// The content is read under the argument's own parsing state — the invocation's state
/// with the [`ArgumentSpec`]'s parsing-state delta applied — and the parser adds only
/// the terminator, as the state's expected group close. By default, then, commands,
/// groups and comments inside the content are parsed as usual. To read the content as
/// raw text instead, give the argument spec a delta that turns off every tokenization
/// feature:
/// [`with_state_delta`](crate::core::specs::ArgumentSpec::with_state_delta)`(ParsingStateDelta::new().rules(`[`TokenRulesOverrides::disable_all()`](crate::core::token::TokenRulesOverrides::disable_all)`))`.
/// The example below shows both.
///
/// # How the terminator is found
///
/// The terminator is installed as the expected group close
/// ([`TokenRules::expecting_group_close`](crate::core::token::TokenRules::expecting_group_close)).
/// The standard token reader looks for the expected close right after any whitespace,
/// before group delimiters, commands, comments and specials, and whether or not groups
/// are enabled. When it is found, the content ends and the terminator becomes the
/// group node's close delimiter; it produces no node of its own. This has the following
/// consequences:
///
/// - It works when every tokenization feature is off, so raw content needs nothing
///   further.
/// - It wins over a command with the same spelling (`\endphf` is never read as a
///   command inside the content) and over a specials trigger (a backtick that would
///   otherwise open another code span).
/// - In parsed content, entering a group (`{…}`) makes that group's own close delimiter
///   the expected close, so the terminator is not recognized inside the group:
///   `\phf a{b\endphf}c\endphf` ends at the second `\endphf`. Likewise, an argument of
///   the same kind inside the content expects its own terminator, so such arguments
///   nest: in `\phf a \phf b\endphf c\endphf`, the first `\endphf` ends the inner one.
/// - The terminator is matched literally, character for character, and wherever its
///   characters appear, even at the start of a longer word: `\endphfoo` ends a
///   `\endphf` argument and leaves `oo` as the content that follows. In the same way,
///   a run of three backticks ends a code span opened with two at its first two
///   backticks, and the third follows the argument. (CommonMark instead requires a
///   closing run of exactly the opening length; that rule is not implemented.)
/// - No whitespace is allowed inside the terminator.
/// - Nothing is removed from the content. In a code span the spaces next to the
///   backticks are content, and trimming them is up to the code that reads the tree.
/// - The content is read under a parsing state of its own, so the parser needs a token
///   reader that scans the source, such as
///   [`StdTokenReader`](crate::core::token::StdTokenReader). A reader that replays a
///   list of tokens scanned earlier cannot read the content under a different state.
///
/// # Node shape
///
/// The argument stages one [`Group`](crate::core::node::NodeKind::Group) node of the
/// configured class. Its open delimiter is the prefix as written — empty when the
/// prefix is empty — and its close delimiter the terminator as written; its children
/// are the content nodes, and they are the argument's content. The group node records
/// the argument's parsing state; the content nodes record the state they were read
/// under, which is the argument's state with the terminator as the expected close.
///
/// # Recovery
///
/// Both problems are reported through [`ParseContext::recover`], so a strict driver
/// aborts the parse and a tolerant one records the diagnostic and continues as
/// described:
///
/// - The prefix is not complete — the opener does not follow, or the repeat character
///   is missing where the run is the whole terminator: [`ExpectedArgumentOpener`], and
///   the argument is reported absent with nothing consumed.
/// - The input ends, or in parsed content a group close delimiter that nothing in the
///   argument opened appears, before the terminator: [`MissingArgumentTerminator`]; the
///   content read so far is kept and the group records an empty close.
///
/// Two configurations can never work, and are reported when the parser runs as an
/// [`ImplementationError`](crate::core::constructs::ImplementationError), which aborts
/// the parse under any recovery policy: a terminator that is always empty (no repeat
/// character, and an empty head and tail), and an opener that begins with the repeat
/// character (the run of repeat characters would always read that character first, so
/// the opener could never match).
///
/// Requires a language with the groups feature ([`LangHasGroups`]): the terminator is
/// installed as an expected group close, and the argument stages a group node.
///
/// # Example
///
/// ```
/// use std::sync::Arc;
/// use techy::core::constructs::TerminatedArgumentParser;
/// use techy::core::specs::{ArgumentSpec, Package};
/// use techy::core::token::TokenRulesOverrides;
/// use techy::core::{Language, ParsingState, ParsingStateDelta};
/// use techy::error::Recovery;
/// use techy::latexlike::{CallableType, GroupType, Latexlike, LatexlikeDriver, MacroSpec};
///
/// let mut package: Package<Latexlike> = Package::new("mydefs");
/// // `\phf … \endphf`, with the content parsed as usual.
/// package.insert(
///     CallableType::Macro,
///     "phf",
///     MacroSpec::new(vec![Arc::new(ArgumentSpec::new(
///         TerminatedArgumentParser::new(GroupType::Content, r"\endphf"),
///         "content",
///     ))]),
/// );
/// // `\raw##"…"##`, with the content read as raw text.
/// package.insert(
///     CallableType::Macro,
///     "raw",
///     MacroSpec::new(vec![Arc::new(
///         ArgumentSpec::new(
///             TerminatedArgumentParser::new(GroupType::Verbatim, "")
///                 .with_repeat_char('#')
///                 .with_opener("\"")
///                 .with_terminator_head("\""),
///             "text",
///         )
///         .with_state_delta(
///             ParsingStateDelta::new().rules(TokenRulesOverrides::disable_all()),
///         ),
///     )]),
/// );
/// let language: Language<Latexlike> = Language::new(
///     LatexlikeDriver::new(Recovery::Strict),
///     ParsingState::lang_initial_with_packages([package]).expect("seed state"),
/// );
///
/// let result = language.parse(r"\phf a {b}\endphf!").unwrap();
/// let phf = result.tree.root().child(0).unwrap();
/// // The content is parsed: a chars node and a group node.
/// let content = phf.argument_content_nodes(0).unwrap();
/// assert_eq!(content.len(), 2);
/// assert_eq!(content.source_text(), Some("a {b}"));
/// // The argument's group node records the prefix (empty here) and the terminator.
/// let group = phf.argument_nodes(0).unwrap().get(0).unwrap();
/// assert_eq!(group.group_delimiters(), Some(("", r"\endphf")));
/// assert_eq!(result.tree.root().child(1).unwrap().chars(), Some("!"));
///
/// // Raw content: `%`, `\x` and `{` are plain characters, and `"#` is not the
/// // terminator `"##`.
/// let result = language.parse(r###"\raw##"a "# \x{%"##."###).unwrap();
/// let raw = result.tree.root().child(0).unwrap();
/// assert_eq!(
///     raw.argument_content_nodes(0).unwrap().source_text(),
///     Some(r##"a "# \x{%"##),
/// );
/// let group = raw.argument_nodes(0).unwrap().get(0).unwrap();
/// assert_eq!(group.group_delimiters(), Some(("##\"", "\"##")));
/// ```
pub struct TerminatedArgumentParser<L: Lang> {
    /// The class of the staged group node (and of the rule carrying the terminator).
    group_type: L::GroupTypeId,
    /// The character whose run starts the prefix and is repeated in the terminator.
    repeat_char: Option<char>,
    /// The fixed string that follows the run in the prefix.
    opener: String,
    /// The fixed string before the repeated characters in the terminator.
    terminator_head: String,
    /// The fixed string after the repeated characters in the terminator.
    terminator_tail: String,
}

impl<L: LangHasGroups> TerminatedArgumentParser<L> {
    /// Creates a parser for an argument ended by `terminator`, staging a group node of
    /// class `group_type`.
    ///
    /// `terminator` is the terminator's tail: the whole terminator unless a repeat
    /// character ([`with_repeat_char`](TerminatedArgumentParser::with_repeat_char)) or a
    /// terminator head
    /// ([`with_terminator_head`](TerminatedArgumentParser::with_terminator_head)) is
    /// configured as well. It may be empty only when a repeat character is configured.
    pub fn new(group_type: L::GroupTypeId, terminator: impl Into<String>) -> Self {
        TerminatedArgumentParser {
            group_type,
            repeat_char: None,
            opener: String::new(),
            terminator_head: String::new(),
            terminator_tail: terminator.into(),
        }
    }

    /// Sets the repeat character: the prefix starts with a run of copies of it, read as
    /// many as there are, and the terminator repeats the same number of copies between
    /// its head and its tail.
    ///
    /// A run of zero copies is allowed unless the terminator would then be empty (no
    /// head and no tail). In that case at least one copy is required, and an argument
    /// that starts with none is reported as [`ExpectedArgumentOpener`].
    pub fn with_repeat_char(mut self, repeat_char: char) -> Self {
        self.repeat_char = Some(repeat_char);
        self
    }

    /// Sets the opener: a fixed string that must follow the run of repeat characters
    /// (default: empty). It is matched character for character, and a mismatch is
    /// reported as [`ExpectedArgumentOpener`].
    ///
    /// The opener must not begin with the repeat character, since the run would always
    /// read that character first. This is checked when the parser runs.
    pub fn with_opener(mut self, opener: impl Into<String>) -> Self {
        self.opener = opener.into();
        self
    }

    /// Sets the terminator head: a fixed string that comes before the repeated
    /// characters in the terminator (default: empty), such as the `"` of `"##` that
    /// ends a Rust-style raw string.
    pub fn with_terminator_head(mut self, head: impl Into<String>) -> Self {
        self.terminator_head = head.into();
        self
    }
}

impl<L: LangHasGroups> TerminatedArgumentParser<L> {
    /// The configuration problem that makes this parser unusable, if any (the two
    /// checks documented on the type).
    fn configuration_error(&self) -> Option<String> {
        match self.repeat_char {
            None if self.terminator_head.is_empty() && self.terminator_tail.is_empty() => {
                Some(String::from(
                    "TerminatedArgumentParser can never form a terminator: no repeat \
                     character is configured, and the terminator head and tail are \
                     both empty",
                ))
            }
            Some(repeat_char) if self.opener.starts_with(repeat_char) => Some(format!(
                "TerminatedArgumentParser's opener ‘{}’ begins with its repeat \
                 character ‘{}’, which the run of repeat characters always reads first",
                self.opener, repeat_char
            )),
            _ => None,
        }
    }

    /// The delta of the prefix probe: every tokenization feature off — whitespace
    /// included, since a space is never part of the prefix and must not be skipped
    /// ahead of it — the inherited close expectation **cleared** (a `}` must be readable
    /// as a prefix character inside a braces group), and the forbidden-character set
    /// cleared with the other features (the prefix is read raw, and a probe must never
    /// reject a token the enclosing state accepts: the probe protocol leaves diagnosing
    /// a failed token to the enclosing content loop).
    fn prefix_probe_delta() -> ParsingStateDelta<L> {
        // The groups literal spreads from the *disabled* block: a whole-block field
        // literal replaces everything `disable_all()` set up for that block.
        ParsingStateDelta::new().rules(TokenRulesOverrides {
            groups: GroupOverrides {
                expecting_close: Some(None),
                ..GroupOverrides::disable()
            },
            ..TokenRulesOverrides::disable_all()
        })
    }

    /// Reads the prefix from `start`: the greedy run of repeat characters, then the
    /// opener. Returns the length of the run, or `None` when the opener does not
    /// follow — diagnosed, and the reader rewound to `start`.
    ///
    /// No probe state is derived when there is no prefix to read (no repeat
    /// character, an empty opener).
    fn read_prefix(
        &self,
        cx: &mut ParseContext<'_, '_, L>,
        start: &StreamPosition<L>,
    ) -> ConstructParserResult<L, Option<usize>> {
        if self.repeat_char.is_none() && self.opener.is_empty() {
            return Ok(Some(0));
        }
        let probe_state = cx.derive_state(&Self::prefix_probe_delta())?;

        let mut count = 0usize;
        if let Some(repeat_char) = self.repeat_char {
            // A tolerated unreadable token (`None`) ends the run: whatever comes next
            // re-reads it — the opener match below, or the content loop, which applies
            // its own token recovery.
            while let Some(token) = cx.probe_token(&probe_state)? {
                if !matches!(cx.tokens.token_kind(&token), TokenKind::Char(c) if c == repeat_char) {
                    break;
                }
                cx.tokens.move_to(&token, TokenEdge::EndPastPostSpace);
                count += 1;
            }
        }

        for expected in self.opener.chars() {
            match cx.probe_token(&probe_state)? {
                Some(token) if matches!(cx.tokens.token_kind(&token), TokenKind::Char(c) if c == expected) =>
                {
                    cx.tokens.move_to(&token, TokenEdge::EndPastPostSpace);
                }
                other => {
                    let at = match &other {
                        Some(token) => cx.tokens.source_span_of(token),
                        None => cx.here(),
                    };
                    cx.recover(ExpectedArgumentOpener::new(self.opener.as_str()), at)?;
                    cx.tokens.move_to_position(start);
                    return Ok(None);
                }
            }
        }
        Ok(Some(count))
    }

    /// The terminator for a prefix whose run had `count` repeat characters: head, the
    /// run, tail.
    fn terminator(&self, count: usize) -> String {
        let mut terminator = self.terminator_head.clone();
        if let Some(repeat_char) = self.repeat_char {
            terminator.extend(core::iter::repeat_n(repeat_char, count));
        }
        terminator.push_str(&self.terminator_tail);
        terminator
    }

    /// The prefix as written, for a run of `count` repeat characters: the run, then the
    /// opener.
    fn prefix_text(&self, count: usize) -> String {
        let mut prefix = String::new();
        if let Some(repeat_char) = self.repeat_char {
            prefix.extend(core::iter::repeat_n(repeat_char, count));
        }
        prefix.push_str(&self.opener);
        prefix
    }
}

impl<L: LangHasGroups> ArgumentParser<L> for TerminatedArgumentParser<L>
where
    ArgumentExt<L>: Default,
    L::InvocationSyntax: FromInvocation<L>,
{
    fn parse_argument(
        &self,
        cx: &mut ParseContext<'_, '_, L>,
        _spec: &ArgumentSpec<L>,
    ) -> ConstructParserResult<L, Option<ParsedArgumentNodes<L>>> {
        // Spec-author contract, validated where the parser runs
        // ([§dd-dr:panic-policy]: an outer-layer contract violation is an `Err`).
        if let Some(problem) = self.configuration_error() {
            return Err(cx.implementation_error(problem, cx.here()));
        }

        // No noise scan: the prefix — or, with an empty prefix, the content — starts
        // right here (type docs, "Where the argument starts").
        let start = cx.tokens.position_here();
        let Some(count) = self.read_prefix(cx, &start)? else {
            return Ok(None);
        };

        let terminator = self.terminator(count);
        if terminator.is_empty() {
            // Only reachable with a repeat character, an empty head and tail, and a
            // run of none: the run is the whole terminator and at least one copy is
            // required. Rewind first, so the diagnostic points where the run was
            // expected.
            cx.tokens.move_to_position(&start);
            let expected = self.repeat_char.map(String::from).unwrap_or_default();
            cx.recover(ExpectedArgumentOpener::new(expected), cx.here())?;
            return Ok(None);
        }

        let prefix_end = cx.tokens.position_here();
        let prefix_span = cx.source_span_within(&start, &prefix_end)?;
        let prefix_text = self.prefix_text(count);

        // The content state: the argument's own state plus the terminator as the
        // expected group close — nothing else, so raw or parsed reading stays the
        // argument state's business (the `probe_minted_group` delta shape).
        let rule = Arc::new(GroupRule {
            group_type: self.group_type,
            open: prefix_text.clone(),
            close: terminator.clone(),
        });
        let content_state =
            cx.derive_state(&ParsingStateDelta::new().rules(TokenRulesOverrides {
                groups: GroupOverrides {
                    expecting_close: Some(Some(rule)),
                    ..GroupOverrides::default()
                },
                ..TokenRulesOverrides::default()
            }))?;

        // The content run, stopping at (and consuming) the terminator — the
        // `GroupParser` interior shape.
        let stop = StopSpec::at_token(
            TokenStopKind::GroupClose {
                group_type: self.group_type,
                close: &terminator,
            },
            true,
        );
        let (outcome, delta) = cx.parse_nodes(content_state, stop, ChildStateSpec::inherit())?;
        // The content-loop parser comes from the driver's factory (outer-layer code
        // for a custom driver), so its contract is validated, not debug-asserted
        // ([§dd-dr:panic-policy]).
        if delta.is_some() {
            return Err(cx.implementation_error(
                "the driver's content-loop parser returned a pass-through state delta \
                 (a nodes parser has no after-effect to report)",
                cx.here(),
            ));
        }

        let (close_span, end): (Option<SourceSpan<L::SourceOrigin>>, _) = match outcome.stop {
            StopCause::TokenCondition { span, after } => (Some(span), after),
            StopCause::EndOfInput => {
                cx.recover(
                    MissingArgumentTerminator::new(
                        terminator.as_str(),
                        MissingTerminatorFound::EndOfInput,
                    ),
                    prefix_span.clone(),
                )?;
                (None, cx.tokens.position_here())
            }
            // A close that nothing in the content opened: close the argument here and
            // leave the token unconsumed for an enclosing level (or the root) to claim
            // — the `UnclosedGroup` unwinding rule.
            StopCause::UnexpectedGroupClose { span, .. } => {
                cx.recover(
                    MissingArgumentTerminator::new(
                        terminator.as_str(),
                        MissingTerminatorFound::StrayGroupClose,
                    ),
                    span,
                )?;
                (None, cx.tokens.position_here())
            }
            StopCause::NodeCondition => {
                return Err(cx.implementation_error(
                    "the driver's content-loop parser reported a node-condition stop, \
                     but the terminated argument's stop spec sets no node condition",
                    cx.here(),
                ));
            }
        };

        let group_span = cx.source_span_within(&start, &end)?;
        // The recorded delimiters are node data. The terminator is one token, so its
        // span is the reader's answer for it; the prefix spans several tokens, and for
        // a language that does not obey span tiling a span over several tokens only
        // describes where they were read, so its text is recorded instead.
        let open = if L::OBEYS_SPAN_TILING {
            node_text_content(&prefix_span, &group_span)
        } else {
            TextContent::Owned(prefix_text.into())
        };
        let close = close_span
            .map(|span| node_text_content(&span, &group_span))
            .unwrap_or_else(TextContent::empty);
        let data = GroupData {
            group_type: Some(self.group_type),
            open,
            close,
        };
        let child_count = outcome.nodes.len() as u32;
        let group = cx
            .stage_node(
                NodeKind::group(data),
                group_span.clone(),
                Arc::clone(&cx.state),
                outcome.nodes,
            )
            .map_err(|error| cx.staging_error(error, group_span))?;
        Ok(Some(ParsedArgumentNodes::new(
            vec![group],
            ContentNodes::InChildrenOf(group, 0..child_count),
            Default::default(),
        )))
    }

    /// The argument is mandatory: absent is a diagnosed recovery, not a valid match.
    fn can_match_empty(&self) -> bool {
        false
    }
}

// Manual impl: a derive would demand `L: Debug` although only `L::GroupTypeId` is
// stored.
impl<L: Lang> fmt::Debug for TerminatedArgumentParser<L> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TerminatedArgumentParser")
            .field("group_type", &self.group_type)
            .field("repeat_char", &self.repeat_char)
            .field("opener", &self.opener)
            .field("terminator_head", &self.terminator_head)
            .field("terminator_tail", &self.terminator_tail)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::constructs::tests::{relaxed_driver, RelaxedStdLang, RELAXED_MACRO};
    use crate::constructs::ImplementationError;
    use crate::engine::{Language, ParseResult, ScopesCommandResolver, StdParseDriver};
    use crate::error::{ParseError, Recovery};
    use crate::node::{check_tree_invariants, NodeRef};
    use crate::scopes::{Package, ScopeStack};
    use crate::spec::{CallableSpec, StdCallableSpec};
    use crate::state::{ParsingState, StateData};
    use crate::token::{
        CommandRule, CommandRules, CommentRule, CommentRules, ForbiddenCharsRules, GroupRules,
        ParagraphRules, SpecialsRules, TokenRules, WhitespaceRules,
    };
    use alloc::format;
    use alloc::string::ToString;
    use alloc::vec::Vec;

    const GT_BRACE: u32 = 0;
    const GT_TERM: u32 = 1;
    const CT_MACRO: u32 = 10;

    /// A compact test language over the standard reader, obeying span tiling (the
    /// default), whose command tokens resolve through the state's scope stack under
    /// [`CT_MACRO`].
    #[derive(Debug, Clone, Copy)]
    struct TermLang;
    impl Lang for TermLang {
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
        type Driver = StdParseDriver<ScopesCommandResolver<TermLang>>;
        fn make_node_ext(
            _kind: &NodeKind<Self>,
            _span: &SourceSpan<Self::SourceOrigin>,
            _state: &Arc<crate::state::ParsingState<Self>>,
            _children: crate::node::StagedChildren<'_, Self>,
        ) -> Result<(), crate::node::NodeBuildError> {
            Ok(())
        }
    }

    /// Whitespace, paragraphs, a `{`…`}` group rule, `\` commands, `%` comments.
    fn rules<L: Lang<GroupTypeId = u32, Features = crate::state::AllLangFeatures>>() -> TokenRules<L>
    {
        TokenRules {
            whitespace: WhitespaceRules {
                enabled: true,
                chars: " \t\n".into(),
            },
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

    /// The argument spec for `parser`, its content parsed (`raw = false`) or read as
    /// raw text through the spec's `disable_all()` delta (`raw = true`).
    fn term_arg(parser: TerminatedArgumentParser<TermLang>, raw: bool) -> ArgumentSpec<TermLang> {
        let spec = ArgumentSpec::new_unnamed(parser);
        if raw {
            spec.with_state_delta(
                ParsingStateDelta::new().rules(TokenRulesOverrides::disable_all()),
            )
        } else {
            spec
        }
    }

    /// A language whose seed state's package defines each named macro with the given
    /// argument specs.
    fn language(
        macros: Vec<(&str, Vec<ArgumentSpec<TermLang>>)>,
        recovery: Recovery,
    ) -> Language<TermLang> {
        let mut package = Package::new("test-macros");
        for (name, arguments) in macros {
            let spec: Arc<dyn CallableSpec<TermLang>> = Arc::new(StdCallableSpec::new(arguments));
            package.insert(CT_MACRO, name, spec);
        }
        let mut scopes = ScopeStack::new();
        scopes.push(Arc::new(package));
        let seed = Arc::new(ParsingState::new(StateData {
            rules: rules(),
            scopes,
            mode: (),
            ext: (),
        }));
        Language::new(
            StdParseDriver::new(
                recovery,
                ScopesCommandResolver {
                    command_type: CT_MACRO,
                },
            ),
            seed,
        )
    }

    /// Parses `input`, checking the tree invariants of every successful parse.
    fn try_parse(
        language: &Language<TermLang>,
        input: &str,
    ) -> Result<ParseResult<TermLang>, ParseError> {
        let result = language.parse(input)?;
        check_tree_invariants(&result.tree);
        Ok(result)
    }

    fn parse(language: &Language<TermLang>, input: &str) -> ParseResult<TermLang> {
        try_parse(language, input).expect("parse")
    }

    fn root_child(result: &ParseResult<TermLang>, i: usize) -> NodeRef<'_, TermLang> {
        result.tree.root().child(i).expect("root child")
    }

    /// The group node of argument 0 — the only node of its region.
    fn arg_group(node: NodeRef<'_, TermLang>) -> NodeRef<'_, TermLang> {
        let region: Vec<_> = node
            .argument_nodes(0)
            .expect("provided argument")
            .iter()
            .collect();
        assert_eq!(region.len(), 1, "the region is the group node alone");
        region[0]
    }

    /// The content nodes of argument 0.
    fn arg_content(node: NodeRef<'_, TermLang>) -> Vec<NodeRef<'_, TermLang>> {
        node.argument_content_nodes(0)
            .expect("provided argument")
            .iter()
            .collect()
    }

    /// The text of argument 0's content, which must be a single chars node.
    fn arg_text(node: NodeRef<'_, TermLang>) -> &str {
        match arg_content(node).as_slice() {
            [chars] => chars.chars().expect("a chars node"),
            other => panic!("expected one chars content node, found {}", other.len()),
        }
    }

    /// The one diagnostic of a tolerant parse, as condition `T`.
    fn only_condition<T: DiagnosticInfo + Clone>(result: &ParseResult<TermLang>) -> T {
        assert_eq!(result.diagnostics.len(), 1, "{:?}", result.diagnostics);
        let diagnostic = result.diagnostics.iter().next().unwrap();
        assert_eq!(diagnostic.identifier(), T::IDENTIFIER);
        diagnostic
            .data()
            .downcast_ref::<T>()
            .expect("the condition")
            .clone()
    }

    #[test]
    fn condition_identifiers_are_pinned() {
        assert_eq!(
            ExpectedArgumentOpener::IDENTIFIER,
            "core.arguments.expected-opener"
        );
        assert_eq!(
            MissingArgumentTerminator::IDENTIFIER,
            "core.arguments.missing-terminator"
        );
    }

    // --- fixed terminators ---------------------------------------------------------

    #[test]
    fn fixed_terminator_with_parsed_content() {
        // `\endm` is also an argument-less macro, so the occurrence inside the braces
        // — where the group's own close is the expected one — resolves as a call.
        let lang = language(
            vec![
                (
                    "m",
                    vec![term_arg(
                        TerminatedArgumentParser::new(GT_TERM, r"\endm"),
                        false,
                    )],
                ),
                ("endm", vec![]),
            ],
            Recovery::Strict,
        );
        let result = parse(&lang, r"\m hello {a\endm b} world\endm after");
        assert!(result.diagnostics.is_empty());

        let m = root_child(&result, 0);
        assert_eq!(m.name(), Some("m"));
        let group = arg_group(m);
        assert_eq!(group.group_type(), Some(GT_TERM));
        assert_eq!(group.group_delimiters(), Some(("", r"\endm")));
        assert_eq!(group.span().content(), r"hello {a\endm b} world\endm");
        // The group records the argument's state; the content nodes the content
        // state, which expects the terminator.
        assert!(group
            .parsing_state()
            .rules()
            .expecting_group_close()
            .is_none());

        let content = arg_content(m);
        assert_eq!(content.len(), 3);
        assert_eq!(content[0].chars(), Some("hello "));
        assert_eq!(
            content[0]
                .parsing_state()
                .rules()
                .expecting_group_close()
                .map(|r| &*r.close),
            Some(r"\endm")
        );
        let braces = content[1];
        assert_eq!(braces.group_delimiters(), Some(("{", "}")));
        assert_eq!(braces.child(0).unwrap().chars(), Some("a"));
        assert_eq!(braces.child(1).unwrap().name(), Some("endm"));
        assert_eq!(braces.child(2).unwrap().chars(), Some("b"));
        assert_eq!(content[2].chars(), Some(" world"));

        assert_eq!(root_child(&result, 1).chars(), Some(" after"));
    }

    #[test]
    fn whitespace_before_the_terminator_is_content() {
        let lang = language(
            vec![(
                "m",
                vec![term_arg(
                    TerminatedArgumentParser::new(GT_TERM, r"\endm"),
                    false,
                )],
            )],
            Recovery::Strict,
        );
        let result = parse(&lang, "\\m a \t\\endm.");
        assert_eq!(arg_text(root_child(&result, 0)), "a \t");
        assert_eq!(root_child(&result, 1).chars(), Some("."));
    }

    #[test]
    fn fixed_terminator_with_raw_content() {
        let lang = language(
            vec![(
                "m",
                vec![term_arg(
                    TerminatedArgumentParser::new(GT_TERM, r"\endm"),
                    true,
                )],
            )],
            Recovery::Strict,
        );
        let result = parse(&lang, r"\m a{b\x %c\endm d");
        assert!(result.diagnostics.is_empty());

        let m = root_child(&result, 0);
        let group = arg_group(m);
        assert_eq!(group.group_delimiters(), Some(("", r"\endm")));
        assert_eq!(arg_text(m), r"a{b\x %c");
        let chars = arg_content(m)[0];
        let rules = chars.parsing_state().rules();
        assert!(!rules.commands_enabled());
        assert!(!rules.groups_enabled());
        assert!(!rules.comments_enabled());
        assert!(!rules.whitespace_enabled());
        assert_eq!(root_child(&result, 1).chars(), Some(" d"));
    }

    #[test]
    fn the_terminator_wins_over_a_command_spelling() {
        // Commands enabled and `endm` undefined: the terminator is recognized before
        // command scanning, so no unresolvable-command error arises.
        let lang = language(
            vec![(
                "m",
                vec![term_arg(
                    TerminatedArgumentParser::new(GT_TERM, r"\endm"),
                    false,
                )],
            )],
            Recovery::Strict,
        );
        let result = parse(&lang, r"\m a\endm b");
        assert!(result.diagnostics.is_empty());
        assert_eq!(arg_text(root_child(&result, 0)), "a");
        assert_eq!(root_child(&result, 1).chars(), Some(" b"));
    }

    #[test]
    fn the_terminator_matches_as_a_prefix() {
        // Pinned: `\endmore` ends a `\endm` argument, leaving `ore` as what follows.
        let lang = language(
            vec![(
                "m",
                vec![term_arg(
                    TerminatedArgumentParser::new(GT_TERM, r"\endm"),
                    false,
                )],
            )],
            Recovery::Strict,
        );
        let result = parse(&lang, r"\m x\endmore");
        assert!(result.diagnostics.is_empty());
        let m = root_child(&result, 0);
        assert_eq!(arg_group(m).group_delimiters(), Some(("", r"\endm")));
        assert_eq!(arg_text(m), "x");
        assert_eq!(root_child(&result, 1).chars(), Some("ore"));
    }

    #[test]
    fn multi_character_fixed_delimiters() {
        let lang = language(
            vec![(
                "q",
                vec![term_arg(
                    TerminatedArgumentParser::new(GT_TERM, ">>").with_opener("<<"),
                    false,
                )],
            )],
            Recovery::Strict,
        );
        let result = parse(&lang, r"\q<<a>b>>c");
        assert!(result.diagnostics.is_empty());
        let q = root_child(&result, 0);
        assert_eq!(arg_group(q).group_delimiters(), Some(("<<", ">>")));
        assert_eq!(arg_text(q), "a>b");
        assert_eq!(root_child(&result, 1).chars(), Some("c"));
    }

    // --- repeat characters ---------------------------------------------------------

    fn plus_run_language(recovery: Recovery) -> Language<TermLang> {
        language(
            vec![(
                "c",
                vec![term_arg(
                    TerminatedArgumentParser::new(GT_TERM, "").with_repeat_char('+'),
                    true,
                )],
            )],
            recovery,
        )
    }

    #[test]
    fn a_delimiter_run_is_counted_and_repeated() {
        let lang = plus_run_language(Recovery::Strict);

        let result = parse(&lang, r"\c++a+b++c");
        let c = root_child(&result, 0);
        assert_eq!(arg_group(c).group_delimiters(), Some(("++", "++")));
        assert_eq!(arg_text(c), "a+b");
        assert_eq!(root_child(&result, 1).chars(), Some("c"));

        let result = parse(&lang, r"\c+a+");
        let c = root_child(&result, 0);
        assert_eq!(arg_group(c).group_delimiters(), Some(("+", "+")));
        assert_eq!(arg_text(c), "a");
        assert!(result.tree.root().child(1).is_none());

        // Nothing is skipped: the spaces after the prefix and before the terminator
        // are content.
        let result = parse(&lang, r"\c+ a +b");
        let c = root_child(&result, 0);
        assert_eq!(arg_text(c), " a ");
        assert_eq!(root_child(&result, 1).chars(), Some("b"));
    }

    #[test]
    fn a_run_that_is_the_whole_terminator_needs_one_copy() {
        let err = try_parse(&plus_run_language(Recovery::Strict), r"\c a").unwrap_err();
        assert!(
            err.to_string()
                .contains("expected ‘+’ to open the argument"),
            "{err}"
        );

        let result = parse(&plus_run_language(Recovery::Tolerant), r"\c a");
        let condition: ExpectedArgumentOpener = only_condition(&result);
        assert_eq!(condition.expected, "+");
        let c = root_child(&result, 0);
        assert!(!c.arguments().unwrap().get(0).unwrap().is_provided());
        // Nothing consumed: what follows the trigger (whose own token took the space
        // after the command name) is parsed again as sibling content.
        assert_eq!(c.span().content(), r"\c ");
        assert_eq!(root_child(&result, 1).chars(), Some("a"));
    }

    #[test]
    fn a_run_plus_a_tail_reads_a_code_span() {
        let lang = language(
            vec![(
                "c",
                vec![term_arg(
                    TerminatedArgumentParser::new(GT_TERM, "`").with_repeat_char('`'),
                    true,
                )],
            )],
            Recovery::Strict,
        );
        let result = parse(&lang, r"\c``a`b```");
        let c = root_child(&result, 0);
        assert_eq!(arg_group(c).group_delimiters(), Some(("``", "```")));
        assert_eq!(arg_text(c), "a`b");

        // No run at all (the space after the command name belongs to the command):
        // the tail alone ends the argument.
        let result = parse(&lang, r"\c x `y");
        let c = root_child(&result, 0);
        assert_eq!(arg_group(c).group_delimiters(), Some(("", "`")));
        assert_eq!(arg_text(c), "x ");
        assert_eq!(root_child(&result, 1).chars(), Some("y"));

        // A longer closing run ends the argument at its first characters (the
        // prefix-match rule; no CommonMark exact-run rule).
        let result = parse(&lang, r"\c`a```b");
        let c = root_child(&result, 0);
        assert_eq!(arg_group(c).group_delimiters(), Some(("`", "``")));
        assert_eq!(arg_text(c), "a");
        assert_eq!(root_child(&result, 1).chars(), Some("`b"));
    }

    fn rust_raw_language(recovery: Recovery) -> Language<TermLang> {
        language(
            vec![(
                "r",
                vec![term_arg(
                    TerminatedArgumentParser::new(GT_TERM, "")
                        .with_repeat_char('#')
                        .with_opener("\"")
                        .with_terminator_head("\""),
                    true,
                )],
            )],
            recovery,
        )
    }

    #[test]
    fn a_run_with_an_opener_and_a_head_reads_a_rust_raw_string() {
        let lang = rust_raw_language(Recovery::Strict);

        let result = parse(&lang, r###"\r##"x"#y"##z"###);
        let r = root_child(&result, 0);
        assert_eq!(arg_group(r).group_delimiters(), Some(("##\"", "\"##")));
        assert_eq!(arg_text(r), "x\"#y");
        assert_eq!(root_child(&result, 1).chars(), Some("z"));

        // A run of none: the head alone is the terminator.
        let result = parse(&lang, r#"\r"a"b"#);
        let r = root_child(&result, 0);
        assert_eq!(arg_group(r).group_delimiters(), Some(("\"", "\"")));
        assert_eq!(arg_text(r), "a");
        assert_eq!(root_child(&result, 1).chars(), Some("b"));
    }

    #[test]
    fn a_missing_opener_reports_the_argument_absent() {
        let err = try_parse(&rust_raw_language(Recovery::Strict), r"\r##x").unwrap_err();
        assert!(
            err.to_string()
                .contains("expected ‘\"’ to open the argument"),
            "{err}"
        );

        let result = parse(&rust_raw_language(Recovery::Tolerant), r"\r##x");
        let condition: ExpectedArgumentOpener = only_condition(&result);
        assert_eq!(condition.expected, "\"");
        let diagnostic = result.diagnostics.iter().next().unwrap();
        // Anchored at the character that is not the opener.
        assert_eq!(diagnostic.span().range(), 4..5);
        let r = root_child(&result, 0);
        assert!(!r.arguments().unwrap().get(0).unwrap().is_provided());
        // The run read while looking for the opener is parsed again as content.
        assert_eq!(root_child(&result, 1).chars(), Some("##x"));

        // The input ends where the opener was expected.
        let result = parse(&rust_raw_language(Recovery::Tolerant), r"\r#");
        let _: ExpectedArgumentOpener = only_condition(&result);
        assert_eq!(root_child(&result, 1).chars(), Some("#"));
    }

    #[test]
    fn a_head_a_run_and_a_tail_read_a_lua_long_string() {
        let lang = language(
            vec![(
                "l",
                vec![term_arg(
                    TerminatedArgumentParser::new(GT_TERM, "]")
                        .with_repeat_char('=')
                        .with_opener("[")
                        .with_terminator_head("]"),
                    true,
                )],
            )],
            Recovery::Strict,
        );
        let result = parse(&lang, r"\l==[a]=]b]==]c");
        let l = root_child(&result, 0);
        assert_eq!(arg_group(l).group_delimiters(), Some(("==[", "]==]")));
        assert_eq!(arg_text(l), "a]=]b");
        assert_eq!(root_child(&result, 1).chars(), Some("c"));
    }

    #[test]
    fn the_prefix_is_read_inside_an_enclosing_group() {
        // The prefix probe clears the inherited close expectation, so `}` can be the
        // repeat character inside a braces group; the enclosing group then finds its
        // own close after the argument.
        let lang = language(
            vec![(
                "c",
                vec![term_arg(
                    TerminatedArgumentParser::new(GT_TERM, "").with_repeat_char('}'),
                    true,
                )],
            )],
            Recovery::Strict,
        );
        let result = parse(&lang, r"{\c}a}}x");
        assert!(result.diagnostics.is_empty());
        let braces = root_child(&result, 0);
        assert_eq!(braces.group_delimiters(), Some(("{", "}")));
        let c = braces.child(0).unwrap();
        assert_eq!(arg_group(c).group_delimiters(), Some(("}", "}")));
        assert_eq!(arg_text(c), "a");
        assert_eq!(root_child(&result, 1).chars(), Some("x"));
    }

    // --- parsed content: groups and nesting -----------------------------------------

    #[test]
    fn a_nested_group_hides_the_terminator() {
        let lang = language(
            vec![(
                "m",
                vec![term_arg(
                    TerminatedArgumentParser::new(GT_TERM, ";;"),
                    false,
                )],
            )],
            Recovery::Strict,
        );
        let result = parse(&lang, r"\m a{b;;c};;d");
        assert!(result.diagnostics.is_empty());
        let m = root_child(&result, 0);
        let content = arg_content(m);
        assert_eq!(content.len(), 2);
        assert_eq!(content[0].chars(), Some("a"));
        assert_eq!(content[1].group_delimiters(), Some(("{", "}")));
        assert_eq!(content[1].child(0).unwrap().chars(), Some("b;;c"));
        assert_eq!(arg_group(m).group_delimiters(), Some(("", ";;")));
        assert_eq!(root_child(&result, 1).chars(), Some("d"));
    }

    #[test]
    fn terminated_arguments_nest() {
        let lang = language(
            vec![(
                "m",
                vec![term_arg(
                    TerminatedArgumentParser::new(GT_TERM, r"\endm"),
                    false,
                )],
            )],
            Recovery::Strict,
        );
        let result = parse(&lang, r"\m a \m b\endm c\endm d");
        assert!(result.diagnostics.is_empty());
        let outer = root_child(&result, 0);
        let content = arg_content(outer);
        assert_eq!(content.len(), 3);
        assert_eq!(content[0].chars(), Some("a "));
        assert_eq!(content[1].name(), Some("m"));
        assert_eq!(arg_text(content[1]), "b");
        assert_eq!(content[2].chars(), Some(" c"));
        assert_eq!(root_child(&result, 1).chars(), Some(" d"));
    }

    // --- recovery --------------------------------------------------------------------

    #[test]
    fn an_unterminated_argument_keeps_its_content() {
        let lang = |recovery| {
            language(
                vec![(
                    "m",
                    vec![term_arg(
                        TerminatedArgumentParser::new(GT_TERM, r"\endm"),
                        false,
                    )],
                )],
                recovery,
            )
        };
        let err = try_parse(&lang(Recovery::Strict), r"\m abc").unwrap_err();
        assert!(
            err.to_string()
                .contains(r"missing terminator ‘\endm’ of the argument before end of input"),
            "{err}"
        );

        let result = parse(&lang(Recovery::Tolerant), r"\m abc");
        let condition: MissingArgumentTerminator = only_condition(&result);
        assert_eq!(condition.terminator, r"\endm");
        assert_eq!(condition.found, MissingTerminatorFound::EndOfInput);
        let m = root_child(&result, 0);
        assert_eq!(arg_group(m).group_delimiters(), Some(("", "")));
        assert_eq!(arg_text(m), "abc");
    }

    #[test]
    fn a_stray_close_ends_the_argument_without_being_consumed() {
        let lang = language(
            vec![(
                "m",
                vec![term_arg(
                    TerminatedArgumentParser::new(GT_TERM, r"\endm"),
                    false,
                )],
            )],
            Recovery::Tolerant,
        );
        let result = parse(&lang, r"\m a}b");
        let diagnostics: Vec<_> = result.diagnostics.iter().collect();
        assert_eq!(diagnostics.len(), 2, "{:?}", result.diagnostics);
        let condition = diagnostics[0]
            .data()
            .downcast_ref::<MissingArgumentTerminator>()
            .expect("the argument's condition first");
        assert_eq!(condition.found, MissingTerminatorFound::StrayGroupClose);
        assert_eq!(diagnostics[0].span().range(), 4..5);
        // The root claims the stray close and diagnoses it separately.
        assert_eq!(
            diagnostics[1].identifier(),
            super::super::StrayGroupClose::IDENTIFIER
        );

        let m = root_child(&result, 0);
        assert_eq!(arg_group(m).group_delimiters(), Some(("", "")));
        assert_eq!(arg_text(m), "a");
        let rest: String = (1..result.tree.root().children().len())
            .map(|i| root_child(&result, i).chars().expect("chars"))
            .collect();
        assert_eq!(rest, "}b");
    }

    #[test]
    fn misconfigurations_are_implementation_errors_under_any_recovery() {
        for parser in [
            TerminatedArgumentParser::new(GT_TERM, ""),
            TerminatedArgumentParser::new(GT_TERM, "x")
                .with_repeat_char('#')
                .with_opener("#a"),
        ] {
            let lang = language(
                vec![("m", vec![term_arg(parser, false)])],
                Recovery::Tolerant,
            );
            let err = try_parse(&lang, r"\m abc").unwrap_err();
            assert_eq!(err.identifier(), ImplementationError::IDENTIFIER, "{err}");
            assert!(
                err.to_string().contains("TerminatedArgumentParser"),
                "{err}"
            );
        }
    }

    #[test]
    fn the_argument_is_mandatory_and_debug_names_its_fields() {
        let parser: TerminatedArgumentParser<TermLang> =
            TerminatedArgumentParser::new(GT_TERM, "]")
                .with_repeat_char('=')
                .with_opener("[")
                .with_terminator_head("]");
        assert!(!ArgumentParser::can_match_empty(&parser));
        let debug = format!("{parser:?}");
        for expected in [
            "TerminatedArgumentParser",
            "group_type: 1",
            "repeat_char: Some('=')",
            "opener: \"[\"",
            "terminator_head: \"]\"",
            "terminator_tail: \"]\"",
        ] {
            assert!(debug.contains(expected), "{debug}");
        }
    }

    // --- a language that does not obey span tiling -----------------------------------

    #[test]
    fn the_prefix_is_recorded_as_text_where_the_language_does_not_obey_span_tiling() {
        let spec: Arc<dyn CallableSpec<RelaxedStdLang>> =
            Arc::new(StdCallableSpec::new([ArgumentSpec::new_unnamed(
                TerminatedArgumentParser::new(GT_TERM, "")
                    .with_repeat_char('#')
                    .with_opener("\"")
                    .with_terminator_head("\""),
            )
            .with_state_delta(
                ParsingStateDelta::new().rules(TokenRulesOverrides::disable_all()),
            )]));
        let mut package = Package::new("test-macros");
        package.insert(RELAXED_MACRO, "r", spec);
        let mut scopes = ScopeStack::new();
        scopes.push(Arc::new(package));
        let seed = Arc::new(ParsingState::new(StateData {
            rules: rules::<RelaxedStdLang>(),
            scopes,
            mode: (),
            ext: (),
        }));
        let language = Language::new(relaxed_driver(Recovery::Strict), seed);
        let result = language
            .parse(r###"\r##"a b"##."###)
            .expect("the parse runs");
        crate::node::validate_tree(&result.tree).expect("the all-trees law holds");

        let r = result.tree.root().child(0).expect("the callable");
        let region: Vec<_> = r.argument_nodes(0).expect("provided").iter().collect();
        let group = region[0];
        let data = group.group().expect("a group node");
        assert!(
            matches!(data.open, TextContent::Owned(_)),
            "{:?}",
            data.open
        );
        assert_eq!(group.group_delimiters(), Some(("##\"", "\"##")));
        let content: Vec<_> = r
            .argument_content_nodes(0)
            .expect("provided")
            .iter()
            .collect();
        assert_eq!(content.len(), 1);
        assert_eq!(content[0].chars(), Some("a b"));
    }
}
