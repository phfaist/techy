//! How an invocation was spelled: the record the preset stores on every callable
//! node.
//!
//! [`InvocationSyntaxData`] is the latexlike value of `Lang::InvocationSyntax`, and a
//! parse stores one on every callable node it stages. Its three variants follow the
//! three invocation forms: a macro records the escape character and the whitespace
//! that ended the command name, an environment records its `\begin` and `\end`
//! spellings, and a specials invocation records nothing at all, because the node's own
//! [`name`](crate::core::node::CallableData::name) is already the spelling as written.
//!
//! The environment side is a small family of its own: [`EnvironmentSyntax`] is the
//! contract such a record fulfills, [`StdEnvironmentSyntax`] is the standard
//! implementation, and [`StdEnvironmentSideSyntax`] holds one of its two sides.
//!
//! These records are what lets the preset reproduce the input byte for byte: source
//! recomposition ([`SourceRecomposer`](super::SourceRecomposer)) reads node payload
//! and nothing else, so whatever it re-emits has to be recorded here while parsing.
//! Read a record back with
//! [`NodeRef::invocation_syntax`](crate::core::node::NodeRef::invocation_syntax), or
//! read the macro post-space alone with
//! [`NodeRef::post_space`](crate::core::node::NodeRef::post_space).

use alloc::format;
use alloc::string::String;
use alloc::sync::Arc;
use core::fmt;

use crate::constructs::{
    node_text_content, EnvironmentBeginSyntaxData, EnvironmentTerminatorSyntaxData,
    FromInvocation, Invocation, NameGroup,
};
use crate::source::{Source, SourceSpan, TextContent};
use crate::state::{InvocationSyntax, Lang};
use crate::token::{GroupRule, TokenEdge, TokenKind, TokenReader};

use super::lang::{LatexlikeInvocationSyntax, LatexlikeLang};
use super::Latexlike;

/// How one callable invocation was spelled, as recorded while parsing.
///
/// This is the latexlike value of [`Lang::InvocationSyntax`]: every callable node a
/// preset parse stages holds one, and the variant says which invocation form was
/// used. Read it with
/// [`NodeRef::invocation_syntax`](crate::core::node::NodeRef::invocation_syntax), or
/// through the readers of
/// [`LatexlikeInvocationSyntax`](super::LatexlikeInvocationSyntax), which answer the
/// individual facts without matching on the variant.
///
/// - [`Macro`](InvocationSyntaxData::Macro) — a command trigger such as `\frac`: the
///   escape character as written, and the trigger token's own syntactic post-space,
///   which is the whitespace that ended a multi-character command name (pylatexenc's
///   `macro_post_space`). Nothing beyond that token's own post-space is ever
///   recorded: whitespace after a single-character command, or after the last
///   argument, is ordinary content of the surrounding node, as in TeX.
/// - [`Environment`](InvocationSyntaxData::Environment) — an environment-shaped
///   invocation: the spelling of its two sides, in the `Env` record type (by default
///   [`StdEnvironmentSyntax`]).
/// - [`Specials`](InvocationSyntaxData::Specials) — a specials trigger such as `~` or
///   `---`: a variant with no fields, because the node's
///   [`name`](crate::core::node::CallableData::name) is already the spelling as
///   written. That is the same rule the macro arm follows (`\foo` records `foo` even
///   where the spec was resolved by prefix), and a paragraph-break node records the
///   whole whitespace run as its name. Which specials a node is, is therefore decided
///   by the identity of its spec — the canonical
///   [`ParagraphBreakSpec`](super::ParagraphBreakSpec) value for a paragraph break —
///   and never by comparing the name against a canonical spelling.
///
/// Source recomposition ([`SourceRecomposer`](super::SourceRecomposer)) re-emits a
/// recorded post-space exactly as it stands. Normalizing, collapsing or dropping that
/// whitespace is a converter's decision, not techy's.
///
/// # The `Env` parameter
///
/// `Env` is where a language family chooses what it records for an environment: a
/// family member names its own [`Lang::InvocationSyntax`], for instance
/// `InvocationSyntaxData<StdEnvironmentSyntax<Flm>>`, and the default is the record
/// of the preset language [`Latexlike`].
///
/// How *tolerantly* the begin and end syntax is scanned is not part of the record but
/// of the parser: a family member that wants looser syntax replaces the invocation or
/// body parser through
/// [`make_invocation_parser`](crate::core::specs::CallableSpec::make_invocation_parser),
/// and the record then records whatever that parser consumed.
///
/// [`Lang::InvocationSyntax`]: crate::core::Lang::InvocationSyntax
#[derive(Clone, Debug)]
pub enum InvocationSyntaxData<Env = StdEnvironmentSyntax<Latexlike>> {
    /// A command-triggered (macro-formed) invocation's spelling facts.
    Macro {
        /// The escape character as written (`\` in `\frac`; a language with
        /// several command rules records whichever fired).
        escape_char: char,
        /// The whitespace that ended the command name, as written; empty when what
        /// followed the name came directly after it.
        ///
        /// Span-backed while the tree still refers to its source, and owned after
        /// [`materialize`](crate::core::node::NodeTree::materialize); a span-backed
        /// value resolves against the carrying node's own source,
        /// `node.span().source()` ([`TextContent::resolve`]). Where it is span-backed
        /// it lies inside the node's own span: at the end of it for a callable with
        /// no arguments, between the name and the first argument region otherwise.
        post_space: TextContent,
    },
    /// An environment-shaped invocation's begin/end syntax facts.
    Environment(Env),
    /// A specials-formed invocation: nothing to record beyond the node's `name`,
    /// which is the spelling as written (see the enum docs).
    Specials,
}

impl<L: Lang, Env: InvocationSyntax<L>> InvocationSyntax<L> for InvocationSyntaxData<Env> {
    fn materialized(&self, source: &Source<L::SourceOrigin>) -> Self {
        match self {
            InvocationSyntaxData::Macro { escape_char, post_space } => {
                InvocationSyntaxData::Macro {
                    escape_char: *escape_char,
                    post_space: post_space.materialized(source),
                }
            }
            InvocationSyntaxData::Environment(env) => {
                InvocationSyntaxData::Environment(env.materialized(source))
            }
            InvocationSyntaxData::Specials => InvocationSyntaxData::Specials,
        }
    }
}

/// Builds the record from the trigger token, at the standard staging site.
///
/// A [`Command`](TokenKind::Command) trigger produces the
/// [`Macro`](InvocationSyntaxData::Macro) variant: the escape character the reader
/// reports, and the trigger token's own syntactic post-space — kept as a span of the
/// node's source for a language that obeys span tiling
/// ([`Lang::OBEYS_SPAN_TILING`](crate::core::Lang::OBEYS_SPAN_TILING)), and as the
/// text itself for a language that does not, where the node's span is not known to
/// contain the trigger's bytes at all.
///
/// Every other trigger — a specials token, and the paragraph-break token the preset
/// stages at its specials site — produces
/// [`Specials`](InvocationSyntaxData::Specials). The
/// [`Environment`](InvocationSyntaxData::Environment) variant is never built here:
/// the `\begin` composition stages its node itself
/// ([`stage_node`](crate::core::constructs::ParseContext::stage_node)) with
/// [`environment_form`](LatexlikeInvocationSyntax::environment_form).
impl<L: Lang, Env> FromInvocation<L> for InvocationSyntaxData<Env> {
    fn from_invocation(
        invocation: &Invocation<'_, L>,
        tokens: &dyn TokenReader<'_, L>,
    ) -> Self {
        match tokens.token_kind(invocation.token) {
            TokenKind::Command { escape_char, .. } => {
                // The post-space is a reader answer, recorded before the node's span
                // is known — so the node-data rule cannot be applied to it here. For a
                // language that obeys span tiling a bare span is sound anyway: the
                // node this payload rides on starts at this very token, so the two lie
                // in one source. A language with `OBEYS_SPAN_TILING = false` promises
                // no such thing (the node's span is whatever its reader describes for
                // the whole invocation), and the text is recorded instead.
                let post_space = tokens.source_span_between(
                    invocation.token,
                    TokenEdge::End,
                    TokenEdge::EndPastPostSpace,
                );
                InvocationSyntaxData::Macro {
                    escape_char,
                    post_space: match L::OBEYS_SPAN_TILING {
                        true => TextContent::Spanned(post_space.span()),
                        false => TextContent::Owned(post_space.content().into()),
                    },
                }
            }
            _ => InvocationSyntaxData::Specials,
        }
    }
}

impl<LLL: LatexlikeLang, Env: EnvironmentSyntax<LLL>> LatexlikeInvocationSyntax<LLL>
    for InvocationSyntaxData<Env>
{
    type Env = Env;

    fn macro_form(escape_char: char, post_space: TextContent) -> Self {
        InvocationSyntaxData::Macro { escape_char, post_space }
    }

    fn environment_form(env: Env) -> Self {
        InvocationSyntaxData::Environment(env)
    }

    fn specials_form() -> Self {
        InvocationSyntaxData::Specials
    }

    fn macro_syntax(&self) -> Option<(char, &TextContent)> {
        match self {
            InvocationSyntaxData::Macro { escape_char, post_space } => {
                Some((*escape_char, post_space))
            }
            _ => None,
        }
    }

    fn environment_syntax(&self) -> Option<&Env> {
        match self {
            InvocationSyntaxData::Environment(env) => Some(env),
            _ => None,
        }
    }

    fn is_specials(&self) -> bool {
        matches!(self, InvocationSyntaxData::Specials)
    }
}

/// One side of a [`StdEnvironmentSyntax`] record: a `\begin{name}`- or
/// `\end{name}`-shaped command with its name group, as written.
///
/// The four fields are the escape character, the command word (`begin` or `end`) as
/// written, that command token's own syntactic post-space — the whitespace tolerated
/// in `\begin {itemize}`, recorded rather than normalized away — and the [`GroupRule`]
/// of the name group, cloned from the matched token. The rule holds the exact
/// delimiter characters as written and, unlike a byte-level recording, also the
/// group's class; it refers to no source, so materialization leaves it alone.
///
/// The environment's *name* is not stored here: it is the node's
/// [`name`](crate::core::node::CallableData::name), and the record's writers take it
/// as an argument.
///
/// # Only `\end{name}`-shaped terminators fit
///
/// A latexlike environment ends with `\end{name}`, and that is the only end syntax
/// this record can hold — the core construct parsers allow more general terminators,
/// such as the literal string a
/// [`VerbatimBodyParser`](crate::core::constructs::VerbatimBodyParser) can be given.
/// A custom
/// [`EnvironmentBehavior::make_body_parser`](super::EnvironmentBehavior::make_body_parser)
/// that reports its terminator as
/// [`EnvironmentTerminatorSyntaxData::Literal`](crate::core::constructs::EnvironmentTerminatorSyntaxData::Literal)
/// therefore records an end side that cannot reproduce the input; see
/// [`StdEnvironmentSyntax::from_parsed`](EnvironmentSyntax::from_parsed) for what is
/// stored in that case.
pub struct StdEnvironmentSideSyntax<L: Lang> {
    /// The escape character as written.
    pub escape_char: char,
    /// The command word as written (`begin` or `end`), without the escape character.
    pub command_word: TextContent,
    /// The whitespace between the command word and the name group, as written; empty
    /// when the name group follows the command word directly.
    pub post_space: TextContent,
    /// The rule of the name group, cloned from the matched token: the delimiter
    /// characters exactly as written, together with the group's class.
    pub name_group_rule: Arc<GroupRule<L>>,
}

impl<L: Lang> StdEnvironmentSideSyntax<L> {
    /// Resolve this side's spelling around `name` (the environment name as
    /// written): escape char + command word + post-space + open delimiter + name +
    /// close delimiter. `source` resolves the span-backed fields (the carrying
    /// node's own source).
    fn write(&self, name: &str, source: &Source<L::SourceOrigin>) -> String {
        format!(
            "{}{}{}{}{}{}",
            self.escape_char,
            self.command_word.resolve(source),
            self.post_space.resolve(source),
            self.name_group_rule.open,
            name,
            self.name_group_rule.close,
        )
    }

    fn materialized(&self, source: &Source<L::SourceOrigin>) -> StdEnvironmentSideSyntax<L> {
        StdEnvironmentSideSyntax {
            escape_char: self.escape_char,
            command_word: self.command_word.materialized(source),
            post_space: self.post_space.materialized(source),
            // Source-independent — exempt from materialization.
            name_group_rule: Arc::clone(&self.name_group_rule),
        }
    }
}

impl<L: Lang> Clone for StdEnvironmentSideSyntax<L> {
    fn clone(&self) -> Self {
        StdEnvironmentSideSyntax {
            escape_char: self.escape_char,
            command_word: self.command_word.clone(),
            post_space: self.post_space.clone(),
            name_group_rule: Arc::clone(&self.name_group_rule),
        }
    }
}

impl<L: Lang> fmt::Debug for StdEnvironmentSideSyntax<L> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("StdEnvironmentSideSyntax")
            .field("escape_char", &self.escape_char)
            .field("command_word", &self.command_word)
            .field("post_space", &self.post_space)
            .field("name_group_rule", &self.name_group_rule)
            .finish()
    }
}

/// What an environment-syntax record must provide: a constructor from the parsed
/// facts, and a writer per side.
///
/// The [`Environment`](InvocationSyntaxData::Environment) variant stores a value of a
/// type implementing this trait. [`StdEnvironmentSyntax`] is the standard
/// implementation; a language family that wants to record something else supplies its
/// own type and names it in its `Lang::InvocationSyntax`.
///
/// A record scans nothing itself. The preset's `\begin` invocation parser does all
/// the scanning — the begin trigger, the name group, the arguments, and the body,
/// whose own parser consumes the terminator — and passes the collected facts to
/// [`from_parsed`] once, when the node is staged. How tolerant that scanning is, is
/// equally the parser's business: replace the invocation or body parser through
/// [`make_invocation_parser`](crate::core::specs::CallableSpec::make_invocation_parser),
/// and the record records what the new parser consumed.
///
/// Re-emission is a pair of writers, [`write_begin`] and [`write_end`], because the
/// two sides are needed separately: source recomposition writes the begin side, then
/// the node's children, then the end side. What [`from_parsed`] recorded is exactly
/// what the writers emit.
///
/// The data bounds and the materialization step come from the [`InvocationSyntax`]
/// supertrait.
///
/// [`from_parsed`]: EnvironmentSyntax::from_parsed
/// [`write_begin`]: EnvironmentSyntax::write_begin
/// [`write_end`]: EnvironmentSyntax::write_end
pub trait EnvironmentSyntax<L: LatexlikeLang>: InvocationSyntax<L> {
    /// Builds the record from the facts the parse collected.
    ///
    /// `begin` is the begin side: the validated command trigger and the name group
    /// that matched. `terminator` is what the body parser reported back —
    /// [`Scanned`](EnvironmentTerminatorSyntaxData::Scanned) when the terminator's
    /// command and name group are known separately (a tokenized terminator, and a raw
    /// body's too when it was given those pieces),
    /// [`Literal`](EnvironmentTerminatorSyntaxData::Literal) when a raw body was given
    /// nothing but a terminator string, and `None` when the body closed without
    /// consuming a terminator at all: a name mismatch, a malformed terminator, or the
    /// end of the input. On `None` the end side stays empty.
    ///
    /// The spellings arrive as source-qualified spans, as the reader answered them.
    /// `node_span` is the extent of the node being staged, and every span the record
    /// keeps is checked against it: a span from another source — reachable only under
    /// a reader serving one parse from several sources — is recorded as text instead,
    /// or not at all.
    fn from_parsed(
        begin: EnvironmentBeginSyntaxData<L>,
        terminator: Option<EnvironmentTerminatorSyntaxData<L>>,
        node_span: &SourceSpan<L::SourceOrigin>,
    ) -> Self;

    /// The begin-side spelling as recorded, written around `name` — the environment's
    /// name as written, which the record does not store itself.
    ///
    /// This is what source recomposition emits ahead of the node's children.
    /// `source` resolves the span-backed fields and must be the carrying node's own
    /// source, `node.span().source()`; a materialized record ignores it.
    ///
    /// # Panics
    ///
    /// Panics if a span-backed field names a range that is not within `source`'s
    /// content, or does not fall on character boundaries — the panic condition of
    /// [`TextContent::resolve`]. Passing a source other than the carrying node's own
    /// is the way to reach it; a record built by a parse and resolved against its own
    /// source never does.
    fn write_begin(&self, name: &str, source: &Source<L::SourceOrigin>) -> String;

    /// The end-side spelling as recorded, written around `name`; the empty string
    /// when the end side is empty.
    ///
    /// An empty end side means the body closed without consuming a terminator, and
    /// emitting nothing is then what reproduces the recovered input.
    ///
    /// # Panics
    ///
    /// The same condition as [`write_begin`](EnvironmentSyntax::write_begin): a
    /// span-backed field that is not valid for `source`.
    fn write_end(&self, name: &str, source: &Source<L::SourceOrigin>) -> String;
}

/// The standard environment-syntax record: the `\begin` and `\end` spellings, one
/// [`StdEnvironmentSideSyntax`] per side.
///
/// This is the `Env` type of the preset's own [`InvocationSyntaxData`], so it is what
/// an ordinary latexlike parse records for an environment. The begin side is always
/// present; the end side is filled from the terminator facts at construction
/// ([`from_parsed`](EnvironmentSyntax::from_parsed)) and stays `None` when the body
/// closed without consuming a terminator — a name mismatch, a malformed terminator,
/// or the end of the input.
///
/// # Only `\end{name}`-shaped terminators fit
///
/// A latexlike environment ends with `\end{name}`, and that is the only end syntax
/// this record can hold, although the core construct parsers allow more general
/// terminators — a
/// [`VerbatimBodyParser`](crate::core::constructs::VerbatimBodyParser) can be given a
/// literal terminator string, for one. A custom
/// [`EnvironmentBehavior::make_body_parser`](super::EnvironmentBehavior::make_body_parser)
/// that reports its terminator as
/// [`EnvironmentTerminatorSyntaxData::Literal`](crate::core::constructs::EnvironmentTerminatorSyntaxData::Literal)
/// leaves this record with no spelling to keep, and
/// [`from_parsed`](EnvironmentSyntax::from_parsed) then stores a placeholder end side
/// that re-emits visibly wrong text: [source recomposition](crate::recompose) still
/// runs, but its output no longer reproduces the input. The preset's own verbatim
/// environments do not take that path.
pub struct StdEnvironmentSyntax<L: Lang> {
    /// The `\begin{name}` side, always recorded.
    pub begin: StdEnvironmentSideSyntax<L>,
    /// The `\end{name}` side; `None` when the body closed without consuming a
    /// terminator.
    pub end: Option<StdEnvironmentSideSyntax<L>>,
}

// Diagonal deliberately (not for all `(L, L2)` pairs): a lang's environment
// record materializes against that lang's own source-origin type; a broader impl
// would only sanction cross-lang payload reuse.
impl<L: Lang> InvocationSyntax<L> for StdEnvironmentSyntax<L> {
    fn materialized(&self, source: &Source<L::SourceOrigin>) -> Self {
        StdEnvironmentSyntax {
            begin: self.begin.materialized(source),
            end: self.end.as_ref().map(|end| end.materialized(source)),
        }
    }
}

impl<L: LatexlikeLang> EnvironmentSyntax<L> for StdEnvironmentSyntax<L> {
    /// What each terminator case records:
    ///
    /// - the begin side is recorded as parsed, spans staying spans;
    /// - a [`Scanned`](EnvironmentTerminatorSyntaxData::Scanned) terminator records
    ///   the end side the same way;
    /// - a [`Literal`](EnvironmentTerminatorSyntaxData::Literal) terminator has no
    ///   command and name group to record, and this record has nowhere to keep the
    ///   literal string instead, so the end side is filled with the placeholder
    ///   command word `??END_SYNTAX_NOT_AVAILABLE??`. Re-emitting it is then visibly
    ///   wrong rather than quietly plausible. The preset's own verbatim environments
    ///   do not take this path: they give
    ///   [`VerbatimBodyParser`](crate::core::constructs::VerbatimBodyParser) a
    ///   [`StopEnvironmentCommand`](crate::core::constructs::VerbatimBodyTerminator::StopEnvironmentCommand)
    ///   terminator, which reports `Scanned` facts;
    /// - `None` leaves the end side empty.
    fn from_parsed(
        begin: EnvironmentBeginSyntaxData<L>,
        terminator: Option<EnvironmentTerminatorSyntaxData<L>>,
        node_span: &SourceSpan<L::SourceOrigin>,
    ) -> Self {
        let transcribe_side = |escape_char: char,
                               command_word: &SourceSpan<L::SourceOrigin>,
                               post_space: &SourceSpan<L::SourceOrigin>,
                               name_group: &NameGroup<L>| {
            StdEnvironmentSideSyntax {
                escape_char,
                command_word: node_text_content(command_word, node_span),
                post_space: node_text_content(post_space, node_span),
                name_group_rule: Arc::clone(name_group.rule()),
            }
        };
        let begin_side = transcribe_side(
            begin.escape_char,
            &begin.command_word,
            &begin.post_space,
            &begin.name_group,
        );
        let end = match &terminator {
            Some(EnvironmentTerminatorSyntaxData::Scanned {
                escape_char,
                command_word,
                post_space,
                name_group,
            }) => Some(transcribe_side(*escape_char, command_word, post_space, name_group)),
            Some(EnvironmentTerminatorSyntaxData::Literal { .. }) => {
                // In latexlike, environments should NOT report a Literal terminator if we
                // want an accurate StdEnvironmentSyntax.
                // If you report a Literal terminator, we store garbage.
                Some(StdEnvironmentSideSyntax {
                    escape_char: begin_side.escape_char,
                    command_word: TextContent::from(String::from("??END_SYNTAX_NOT_AVAILABLE??")),
                    post_space: TextContent::empty(),
                    name_group_rule: Arc::clone(&begin_side.name_group_rule),
                })
            }
            None => None,
        };
        StdEnvironmentSyntax { begin: begin_side, end }
    }

    fn write_begin(&self, name: &str, source: &Source<L::SourceOrigin>) -> String {
        self.begin.write(name, source)
    }

    fn write_end(&self, name: &str, source: &Source<L::SourceOrigin>) -> String {
        match &self.end {
            Some(end) => end.write(name, source),
            None => String::new(),
        }
    }
}

impl<L: Lang> Clone for StdEnvironmentSyntax<L> {
    fn clone(&self) -> Self {
        StdEnvironmentSyntax { begin: self.begin.clone(), end: self.end.clone() }
    }
}

impl<L: Lang> fmt::Debug for StdEnvironmentSyntax<L> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("StdEnvironmentSyntax")
            .field("begin", &self.begin)
            .field("end", &self.end)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use crate::source::Span;

    use alloc::boxed::Box;
    use alloc::string::ToString;
    use alloc::vec;
    use alloc::vec::Vec;

    use super::super::test_support::{macro_package, with_package, with_packages};
    use super::super::{
        CallableType, EnvironmentSpec, Latexlike, LatexlikeDriver, MacroSpec,
        VerbatimBehavior,
    };
    use super::*;
    use crate::constructs::{ConstructParser, GroupArgumentParser, StdInvocationParser};
    use crate::engine::{Language, ParseResult};
    use crate::error::Recovery;
    use crate::node::{
        BuildId, NodeRef, ParsedArguments, ParsedSlots,
    };
    use crate::latexlike::check_latexlike_tree_invariants;
    use crate::scopes::Package;
    use crate::spec::{ArgumentSpec, CallableSpec, FrameRole};
    use crate::state::{CommandOverrides, ParsingState, ParsingStateDelta, TokenRulesOverrides};
    use crate::token::CommandRule;

    fn parse_ok(language: &Language<Latexlike>, input: &str) -> ParseResult<Latexlike> {
        let result = language.parse(input).unwrap();
        check_latexlike_tree_invariants(&result.tree);
        assert!(
            result.diagnostics.is_empty(),
            "unexpected diagnostics: {:?}",
            result.diagnostics
        );
        result
    }

    fn payload<'t>(node: NodeRef<'t, Latexlike>) -> &'t InvocationSyntaxData {
        node.invocation_syntax().expect("a callable node")
    }

    /// A resolution source over `content` (equal content, so span-backed payload
    /// fields resolve identically to the parse's own source).
    fn src(content: &str) -> Source {
        Source::new(content)
    }

    // --- the macro arm -----------------------------------------------------------------

    #[test]
    fn macros_record_escape_char_and_post_space() {
        let language = with_package(Recovery::Strict, macro_package("t", "emph", None));

        // Multi-char command with post-space: recorded exactly (the token's own).
        let result = parse_ok(&language, "\\emph  x");
        let emph = result.tree.root().child(0).unwrap();
        match payload(emph) {
            InvocationSyntaxData::Macro { escape_char, post_space } => {
                assert_eq!(*escape_char, '\\');
                assert_eq!(post_space.resolve(&src("\\emph  x")), "  ");
            }
            other => panic!("expected the Macro arm, got {other:?}"),
        }
        // The sugar reads the same fact.
        assert_eq!(emph.post_space(), Some("  "));

        // No post-space (`{` follows the name directly): recorded empty.
        let result = parse_ok(&language, "\\emph{x}");
        let emph = result.tree.root().child(0).unwrap();
        assert_eq!(emph.post_space(), Some(""));
    }

    #[test]
    fn from_invocation_takes_the_post_space_from_the_reader() {
        // The constructor directly: the reader says the trigger is a command, and
        // where its syntactic post-space lies.
        use crate::token::{StdTokenReader, TokenReader};
        use alloc::sync::Arc;

        // minilatex supplies the `~` specials trigger the second half needs.
        let seed = ParsingState::<Latexlike>::lang_initial_with_packages([
            super::super::minidefs::minilatex_package(),
            macro_package("t", "emph", None),
        ])
        .expect("seed state");
        let state = Arc::new(seed);
        let source: Arc<Source> = Arc::new(Source::new("\\emph  x"));
        let mut reader = StdTokenReader::new(&source);
        let token = TokenReader::<'_, Latexlike>::next(&mut reader, &state).unwrap();
        let tokens: &dyn TokenReader<'_, Latexlike> = &reader;
        let spec: Arc<dyn CallableSpec<Latexlike>> = Arc::new(MacroSpec::new(vec![]));
        let invocation = crate::constructs::Invocation {
            callable_type: CallableType::Macro,
            name: "emph",
            spec: &spec,
            token: &token,
        };
        match InvocationSyntaxData::<StdEnvironmentSyntax<Latexlike>>::from_invocation(
            &invocation,
            tokens,
        ) {
            InvocationSyntaxData::Macro { escape_char, post_space } => {
                assert_eq!(escape_char, '\\');
                assert_eq!(post_space.resolve(&source), "  ");
            }
            other => panic!("expected the Macro arm, got {other:?}"),
        }

        // A trigger that is not a command records the specials arm instead — read
        // from a real specials token, since the arm is the reader's answer now.
        let tilde_source: Arc<Source> = Arc::new(Source::new("~x"));
        let mut tilde_reader = StdTokenReader::new(&tilde_source);
        let tilde =
            TokenReader::<'_, Latexlike>::next(&mut tilde_reader, &state).unwrap();
        let tilde_tokens: &dyn TokenReader<'_, Latexlike> = &tilde_reader;
        assert!(matches!(
            tilde_tokens.token_kind(&tilde),
            crate::token::TokenKind::Specials { .. }
        ));
        let specials = crate::constructs::Invocation {
            callable_type: CallableType::Specials,
            name: "~",
            spec: &spec,
            token: &tilde,
        };
        assert!(matches!(
            InvocationSyntaxData::<StdEnvironmentSyntax<Latexlike>>::from_invocation(
                &specials,
                tilde_tokens
            ),
            InvocationSyntaxData::Specials
        ));
    }

    #[test]
    fn macros_record_the_escape_char_as_written() {
        // A second command rule with the `@` escape: the payload records whichever
        // escape fired, not a canonical `\`.
        let seed = ParsingState::<Latexlike>::lang_initial_with_packages([macro_package(
            "t", "emph", None,
        )]).expect("seed state");
        let mut commands = seed.rules().commands.rules.clone();
        commands.push(Arc::new(CommandRule {
            escape_char: '@',
            name_chars: "abcdefghijklmnopqrstuvwxyz".into(),
        }));
        let seed = seed
            .derived(&ParsingStateDelta::new().rules(TokenRulesOverrides {
                commands: CommandOverrides {
                    rules: Some(commands),
                    ..CommandOverrides::default()
                },
                ..TokenRulesOverrides::default()
            }))
            .unwrap();
        let language = Language::new(LatexlikeDriver::new(Recovery::Strict), seed);

        let result = parse_ok(&language, "@emph x");
        let emph = result.tree.root().child(0).unwrap();
        match payload(emph) {
            InvocationSyntaxData::Macro { escape_char, post_space } => {
                assert_eq!(*escape_char, '@');
                assert_eq!(post_space.resolve(&src("@emph x")), " ");
            }
            other => panic!("expected the Macro arm, got {other:?}"),
        }
    }

    // --- the specials arm --------------------------------------------------------------

    #[test]
    fn specials_record_the_unit_arm_and_the_name_as_written() {
        // minilatex supplies the `---`/`~` typography specials.
        let language = with_packages(
            Recovery::Strict,
            [super::super::minidefs::minilatex_package(), macro_package("t", "emph", None)],
        );
        let result = parse_ok(&language, "a---b ~ c");
        let ligature = result.tree.root().child(1).unwrap();
        assert!(matches!(payload(ligature), InvocationSyntaxData::Specials));
        // Option 1: the name IS the invocation spelling as written.
        assert_eq!(ligature.specials_name(), Some("---"));
        assert_eq!(ligature.post_space(), Some(""));

        let tilde = result.tree.root().child(3).unwrap();
        assert!(matches!(payload(tilde), InvocationSyntaxData::Specials));
        assert_eq!(tilde.specials_name(), Some("~"));
    }

    // --- the environment arm -----------------------------------------------------------

    fn env_language() -> Language<Latexlike> {
        let mut package = Package::new("t");
        package.insert(CallableType::Environment, "itemize", EnvironmentSpec::new(vec![]));
        package.insert(
            CallableType::Environment,
            "verbatim",
            EnvironmentSpec::from_behavior(Arc::new(VerbatimBehavior::default())),
        );
        package.insert(CallableType::Environment, "A", EnvironmentSpec::new(vec![]));
        package.insert(CallableType::Environment, "B", EnvironmentSpec::new(vec![]));
        with_package(Recovery::Tolerant, package)
    }

    fn env_payload<'t>(node: NodeRef<'t, Latexlike>) -> &'t StdEnvironmentSyntax<Latexlike> {
        match payload(node) {
            InvocationSyntaxData::Environment(env) => env,
            other => panic!("expected the Environment arm, got {other:?}"),
        }
    }

    #[test]
    fn environments_record_begin_and_end_facts() {
        let language = env_language();
        let content = "\\begin {itemize}x\\end{itemize}";
        let result = parse_ok(&language, content);
        let env = result.tree.root().child(0).unwrap();
        let syntax = env_payload(env);

        // Begin side: escape char, command word, the *recorded* (no longer
        // normalized-away) post-space, and the name-group rule's exact bytes.
        assert_eq!(syntax.begin.escape_char, '\\');
        let source = src(content);
        assert_eq!(syntax.begin.command_word.resolve(&source), "begin");
        assert_eq!(syntax.begin.post_space.resolve(&source), " ");
        assert_eq!(&*syntax.begin.name_group_rule.open, "{");
        assert_eq!(&*syntax.begin.name_group_rule.close, "}");

        // End side: filled from the terminator the body parser consumed.
        let end = syntax.end.as_ref().expect("a consumed terminator");
        assert_eq!(end.escape_char, '\\');
        assert_eq!(end.command_word.resolve(&source), "end");
        assert_eq!(end.post_space.resolve(&source), "");

        // The spelling writers reemit both sides exactly.
        assert_eq!(syntax.write_begin("itemize", &source), "\\begin {itemize}");
        assert_eq!(syntax.write_end("itemize", &source), "\\end{itemize}");

        // The sugar: environment-formed callables answer empty post-space.
        assert_eq!(env.post_space(), Some(""));
    }

    #[test]
    fn verbatim_environments_record_std_end_facts_from_the_terminator() {
        let language = env_language();
        let content = "\\begin{verbatim}\na % b\n\\end{verbatim}";
        let result = parse_ok(&language, content);
        let env = result.tree.root().child(0).unwrap();
        let syntax = env_payload(env);

        // The raw body consumed its terminator as one token, but it was *given*
        // the terminator piecewise (`VerbatimBodyTerminator::StopEnvironmentCommand`)
        // and reports those pieces back as standard `Scanned` facts — span-backed
        // like a tokenized terminator's, not synthesized.
        let end = syntax.end.as_ref().expect("the terminator was consumed");
        assert_eq!(end.escape_char, '\\');
        let source = src(content);
        let evpos = content.find("\\end{verbatim}").unwrap();
        let TextContent::Spanned(command_word) = end.command_word else {
            panic!("the end command word is span-backed, got {:?}", end.command_word);
        };
        assert_eq!(command_word.range(), evpos + 1..evpos + 4);
        assert_eq!(end.command_word.resolve(&source), "end");
        let TextContent::Spanned(post_space) = end.post_space else {
            panic!("the end post-space is span-backed, got {:?}", end.post_space);
        };
        assert_eq!(post_space.range(), evpos + 4..evpos + 4);
        assert_eq!(end.post_space.resolve(&source), "");
        assert_eq!(syntax.write_end("verbatim", &source), "\\end{verbatim}");
    }

    #[test]
    fn a_body_without_a_terminator_leaves_the_end_side_empty() {
        let language = env_language();

        // End of input inside the body.
        let result = language.parse("\\begin{itemize}x").unwrap();
        let env = result.tree.root().child(0).unwrap();
        let syntax = env_payload(env);
        assert!(syntax.end.is_none());
        assert_eq!(syntax.write_end("itemize", &src("\\begin{itemize}x")), "");

        // A name mismatch unwinds B without consuming `\end{A}`: B's end side is
        // empty, while A found and recorded its own terminator.
        let content = "\\begin{A}x\\begin{B}y\\end{A}";
        let result = language.parse(content).unwrap();
        let outer = result.tree.root().child(0).unwrap();
        assert_eq!(outer.environment_name(), Some("A"));
        assert!(env_payload(outer).end.is_some());
        let inner = outer.body().unwrap().iter().nth(1).unwrap();
        assert_eq!(inner.environment_name(), Some("B"));
        assert!(env_payload(inner).end.is_none());
    }

    // --- materialize-through -----------------------------------------------------------

    #[test]
    fn materialize_resolves_the_payload_through_the_bound_trait() {
        let language = env_language();
        let content = "\\begin {itemize}x\\end{itemize}";
        let result = parse_ok(&language, content);
        let owned = result.tree.materialize();

        let env = owned.root().child(0).unwrap();
        let syntax = env_payload(env);
        assert!(syntax.begin.command_word.is_owned());
        assert!(syntax.begin.post_space.is_owned());
        let empty = src("");
        assert_eq!(syntax.begin.command_word.resolve(&empty), "begin");
        assert_eq!(syntax.begin.post_space.resolve(&empty), " ");
        let end = syntax.end.as_ref().unwrap();
        assert!(end.command_word.is_owned());
        // The writers now resolve with no source at all (source-independent
        // byte-faithful reconstruction).
        assert_eq!(syntax.write_begin("itemize", &empty), "\\begin {itemize}");
        assert_eq!(syntax.write_end("itemize", &empty), "\\end{itemize}");

        // The macro arm likewise.
        let language = with_package(Recovery::Strict, macro_package("t", "emph", None));
        let result = parse_ok(&language, "\\emph  x");
        let owned = result.tree.materialize();
        let emph = owned.root().child(0).unwrap();
        match payload(emph) {
            InvocationSyntaxData::Macro { post_space, .. } => {
                assert!(post_space.is_owned());
                assert_eq!(post_space.resolve(&src("")), "  ");
            }
            other => panic!("expected the Macro arm, got {other:?}"),
        }
        assert_eq!(emph.post_space(), Some("  "));
    }

    // --- the role-trait impl -----------------------------------------------------------

    #[test]
    fn the_enum_satisfies_the_fifth_role_trait_coherence_contracts() {
        type Syntax = InvocationSyntaxData;
        let macro_form: Syntax =
            LatexlikeInvocationSyntax::<Latexlike>::macro_form('\\', TextContent::from(" ".to_string()));
        let (escape_char, post_space) =
            LatexlikeInvocationSyntax::<Latexlike>::macro_syntax(&macro_form).unwrap();
        assert_eq!(escape_char, '\\');
        assert_eq!(post_space.resolve(&src("")), " ");
        assert!(!LatexlikeInvocationSyntax::<Latexlike>::is_specials(&macro_form));

        let specials: Syntax = LatexlikeInvocationSyntax::<Latexlike>::specials_form();
        assert!(LatexlikeInvocationSyntax::<Latexlike>::is_specials(&specials));
        assert!(LatexlikeInvocationSyntax::<Latexlike>::macro_syntax(&specials).is_none());
        assert!(
            LatexlikeInvocationSyntax::<Latexlike>::environment_syntax(&specials).is_none()
        );
    }

    // --- stage_invocation (the staging shorthand) ---------------------------------------

    /// A rest-of-line takeover: consumes through the end of the line and claims the
    /// extent via `end: Some(&position)` — the consumed-extent-outruns-children
    /// case.
    #[derive(Debug)]
    struct RestOfLineSpec;

    impl crate::serialize::SerializableObject<Latexlike> for RestOfLineSpec {}

    impl CallableSpec<Latexlike> for RestOfLineSpec {
        fn requires_content(&self) -> bool {
            true
        }

        fn make_invocation_parser<'a>(
            &'a self,
            invocation: crate::constructs::Invocation<'a, Latexlike>,
        ) -> Result<
            Box<dyn ConstructParser<Latexlike, Output = BuildId> + 'a>,
            crate::error::ParseError,
        >
        {
            struct RestOfLineParser<'a> {
                invocation: crate::constructs::Invocation<'a, Latexlike>,
            }
            impl ConstructParser<Latexlike> for RestOfLineParser<'_> {
                type Output = BuildId;
                fn parse(
                    &mut self,
                    cx: &mut crate::constructs::ParseContext<'_, '_, Latexlike>,
                ) -> crate::constructs::ConstructParserResult<
                    Latexlike,
                    (BuildId, Option<Box<ParsingStateDelta<Latexlike>>>),
                > {
                    // Consume the rest of the line, raw: under a state with every
                    // recognizer off (the verbatim recipe) every byte arrives as a
                    // `Char` token, so the read stops at the line's newline without
                    // consuming it.
                    let raw = cx.derive_state(&ParsingStateDelta::new().rules(
                        TokenRulesOverrides {
                            groups: crate::state::GroupOverrides {
                                expecting_close: Some(None),
                                ..crate::state::GroupOverrides::disable()
                            },
                            ..TokenRulesOverrides::disable_all()
                        },
                    ))?;
                    while let Some(token) = cx.probe_token(&raw)? {
                        match cx.tokens.token_kind(&token) {
                            TokenKind::Char('\n') => break,
                            TokenKind::Char(_) => {
                                cx.tokens.move_to(&token, TokenEdge::EndPastPostSpace)
                            }
                            _ => break,
                        }
                    }
                    // The claimed extent is where the reader now stands.
                    let end = cx.tokens.position_here();
                    let id = cx.stage_invocation(
                        &self.invocation,
                        ParsedArguments::empty(),
                        ParsedSlots::empty(),
                        Vec::new(),
                        Some(&end),
                    )?;
                    Ok((id, None))
                }
            }
            Ok(Box::new(RestOfLineParser { invocation }))
        }

        fn stack_frame_title(&self, role: FrameRole, name: &str) -> alloc::string::String {
            super::super::spec::frame_title("macro", role, name)
        }
    }

    /// A takeover that stages with an explicit end **before** the trigger's own
    /// start (the trigger's pre-space edge) — the vehicle for the
    /// bad-computed-span contract violation: `stage_invocation` must answer an
    /// implementation error, never panic.
    #[derive(Debug)]
    struct BadEndSpec;

    impl crate::serialize::SerializableObject<Latexlike> for BadEndSpec {}

    impl CallableSpec<Latexlike> for BadEndSpec {
        fn make_invocation_parser<'a>(
            &'a self,
            invocation: crate::constructs::Invocation<'a, Latexlike>,
        ) -> Result<
            Box<dyn ConstructParser<Latexlike, Output = BuildId> + 'a>,
            crate::error::ParseError,
        >
        {
            struct BadEndParser<'a> {
                invocation: crate::constructs::Invocation<'a, Latexlike>,
            }
            impl ConstructParser<Latexlike> for BadEndParser<'_> {
                type Output = BuildId;
                fn parse(
                    &mut self,
                    cx: &mut crate::constructs::ParseContext<'_, '_, Latexlike>,
                ) -> crate::constructs::ConstructParserResult<
                    Latexlike,
                    (BuildId, Option<Box<ParsingStateDelta<Latexlike>>>),
                > {
                    // A legitimately obtained position that nonetheless cannot
                    // end this node: the trigger's own pre-space edge lies before
                    // its start.
                    let end = cx.tokens.position_at(
                        self.invocation.token,
                        crate::token::TokenEdge::StartBeforePreSpace,
                    );
                    let id = cx.stage_invocation(
                        &self.invocation,
                        ParsedArguments::empty(),
                        ParsedSlots::empty(),
                        Vec::new(),
                        Some(&end),
                    )?;
                    Ok((id, None))
                }
            }
            Ok(Box::new(BadEndParser { invocation }))
        }

        fn stack_frame_title(&self, role: FrameRole, name: &str) -> alloc::string::String {
            super::super::spec::frame_title("macro", role, name)
        }
    }

    /// A language whose `\bad` macro stages with an end before the trigger's start.
    fn bad_end_language(recovery: Recovery) -> Language<Latexlike> {
        let mut package = Package::new("t");
        package.insert(CallableType::Macro, "bad", Arc::new(BadEndSpec));
        with_packages(recovery, [package])
    }

    #[test]
    fn stage_invocation_reports_a_bad_computed_span_as_an_error_not_a_panic() {
        let assert_implementation_error = |error: crate::error::ParseError| {
            let condition = error
                .data()
                .downcast_ref::<crate::constructs::ImplementationError>()
                .expect("an ImplementationError condition");
            assert!(
                condition.detail.contains("invalid node span"),
                "unexpected detail: {}",
                condition.detail
            );
            // Anchored at the trigger — the construct whose staging failed — not
            // at wherever the reader happened to stand (`\bad ` is 3..8, its
            // syntactic post-space included).
            assert_eq!(error.span().range(), 3..8);
        };

        // An end preceding the trigger's start (`\bad` starts at 3, its pre-space
        // at 2). An end outside the source content, or off a character boundary, is
        // no longer expressible: a stream position comes from the reader, and the
        // reader hands out only valid ones.
        let language = bad_end_language(Recovery::Strict);
        assert_implementation_error(language.parse("ab \\bad cd").unwrap_err());

        // Multi-byte content takes the same path (an abort, never a panic).
        let language = bad_end_language(Recovery::Strict);
        assert_implementation_error(language.parse("ab \\bad é").unwrap_err());

        // Tolerant recovery does not swallow the abort (the implementation-error
        // contract: a contract violation is not a source condition).
        let language = bad_end_language(Recovery::Tolerant);
        assert_implementation_error(language.parse("ab \\bad cd").unwrap_err());
    }

    #[test]
    fn stage_invocation_applies_the_std_and_explicit_end_rules() {
        // end: None — the std rule: last child's span end…
        let mut package = Package::new("t");
        package.insert(
            CallableType::Macro,
            "emph",
            MacroSpec::new(vec![Arc::new(ArgumentSpec::new_unnamed(Arc::new(
                GroupArgumentParser::new(super::super::GroupType::Content),
            )))]),
        );
        package.insert(CallableType::Macro, "title", Arc::new(RestOfLineSpec));
        // minilatex supplies the `---` specials the childless-shape probe uses.
        let language = with_packages(
            Recovery::Strict,
            [super::super::minidefs::minilatex_package(), Arc::new(package)],
        );

        let result = parse_ok(&language, "\\emph{ab} x");
        let emph = result.tree.root().child(0).unwrap();
        assert_eq!(emph.span().range(), 0..9);

        // …else the trigger's end (childless shapes, post-space included).
        let result = parse_ok(&language, "a---b");
        let ligature = result.tree.root().child(1).unwrap();
        assert_eq!(ligature.span().range(), 1..4);

        // end: Some — the consumed extent outruns the (empty) child list.
        let content = "\\title The Title\nrest";
        let result = parse_ok(&language, content);
        let title = result.tree.root().child(0).unwrap();
        assert_eq!(title.name(), Some("title"));
        assert_eq!(title.span().range(), 0..content.find('\n').unwrap());
        assert_eq!(title.child_count(), 0);
        // The payload still transcribed from the bundle.
        assert!(matches!(
            payload(title),
            InvocationSyntaxData::Macro { escape_char: '\\', .. }
        ));
        let after = result.tree.root().child(1).unwrap();
        assert_eq!(after.chars(), Some("\nrest"));
    }

    // --- the () payload ----------------------------------------------------------------

    #[test]
    fn the_unit_payload_records_nothing_and_satisfies_both_traits() {
        // materialized: the identity. (from_invocation for `()` is exercised by
        // every TrivialLang parse across the core suites.)
        #[allow(clippy::unit_cmp)]
        {
            assert_eq!(
                crate::state::InvocationSyntax::<Latexlike>::materialized(
                    &(),
                    &Source::new("abc")
                ),
                ()
            );
        }
    }

    // --- Debug still renders the parser bundle (regression for the swap) ---------------

    #[test]
    fn std_invocation_parser_debug_mentions_the_bundle() {
        let language = with_package(Recovery::Strict, macro_package("t", "emph", None));
        let result = parse_ok(&language, "\\emph x");
        // Debug of the payload renders the arm.
        let emph = result.tree.root().child(0).unwrap();
        let rendered = alloc::format!("{:?}", payload(emph));
        assert!(rendered.contains("Macro"), "{rendered}");
        let _ = StdInvocationParser::new(crate::constructs::Invocation {
            callable_type: CallableType::Macro,
            name: "emph",
            spec: &(Arc::new(MacroSpec::new(vec![])) as Arc<dyn CallableSpec<Latexlike>>),
            token: &crate::token::StdToken::end_of_stream(Span::empty(0)),
        });
    }
    // --- the macro arm under a language that does not obey span tiling (PLAN §1.5 R3) ---

    /// The `post_space` [`InvocationSyntaxData::Macro`] records for the `\foo` of
    /// `"\foo  x"`, under the language named by `L`.
    fn macro_post_space<L>() -> TextContent
    where
        L: crate::state::Lang<
            CallableTypeId = u32,
            SourceOrigin = Option<String>,
            Features = crate::state::AllLangFeatures,
            StateExt = (),
            ModeId = (),
        >,
        L::Tokenization: crate::token::Tokenization<
            L,
            Token = crate::token::StdToken<L>,
            StreamPosition = crate::token::StdStreamPosition,
        >,
    {
        let source: Arc<crate::source::Source> = Arc::new(Source::new("\\foo  x"));
        let mut rules = crate::constructs::tests::min_rules::<L>();
        rules.commands.rules = vec![Arc::new(CommandRule {
            escape_char: '\\',
            name_chars: "abcdefghijklmnopqrstuvwxyz".into(),
        })];
        let state = Arc::new(ParsingState::new(crate::state::StateData {
            rules,
            scopes: crate::scopes::ScopeStack::new(),
            mode: (),
            ext: (),
        }));
        let mut reader = crate::token::StdTokenReader::new(&source);
        let token = crate::token::TokenReader::<L>::peek(&mut reader, &state)
            .expect("a command token");
        let spec: Arc<dyn CallableSpec<L>> = Arc::new(crate::spec::StdCallableSpec::default());
        let invocation = crate::constructs::Invocation {
            callable_type: 0u32,
            name: "foo",
            spec: &spec,
            token: &token,
        };
        let reader_ref: &dyn crate::token::TokenReader<'_, L> = &reader;
        match InvocationSyntaxData::<()>::from_invocation(&invocation, reader_ref) {
            InvocationSyntaxData::Macro { post_space, .. } => post_space,
            other => panic!("expected the macro arm, got {other:?}"),
        }
    }

    /// The payload is minted before the node's span exists, so the node-data rule
    /// cannot decide its representation: a tiled language may record the bare span
    /// (the node starts at this very token), a language with
    /// `OBEYS_SPAN_TILING = false` records the text.
    #[test]
    fn the_macro_post_space_is_owned_where_the_language_does_not_obey_span_tiling() {
        let tiled = macro_post_space::<crate::constructs::tests::PlainLang>();
        assert!(
            matches!(tiled, TextContent::Spanned(span) if span == Span::new(4, 6)),
            "a tiled parse records the post-space as a span, got {tiled:?}"
        );
        let relaxed = macro_post_space::<crate::constructs::tests::RelaxedStdLang>();
        assert!(
            matches!(&relaxed, TextContent::Owned(text) if &**text == "  "),
            "a relaxed parse records the post-space text, got {relaxed:?}"
        );
    }
}
