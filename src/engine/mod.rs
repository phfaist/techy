//! Engine orchestration: [`ParserSession`], the root object of a parse (Phase 6).
//!
//! A session bundles everything one parse accumulates — the staging
//! [`NodeTreeBuilder`], the [`Diagnostics`] sink, and the [`Recovery`] policy — and
//! [`finish`](ParserSession::finish) freezes it into a [`ParseResult`]. Sessions are
//! transient: one parse each, no reuse.
//!
//! The `Language<L>` runtime bundle (long-lived defaults + libraries, with a `parse()`
//! convenience entry point) is **deferred** past Phase 6 (DESIGN_RATIONALE.md §3.6):
//! Phase 6 drives sessions directly, and convenience code is not written before its
//! convenience is demonstrable. Consequently `ParseResult` carries no `'env` lifetime
//! and no `Language` reference.

use core::fmt;

use crate::error::{
    Diagnostic, Diagnostics, ParseError, ParseErrorKind, Recovery,
};
use crate::node::{BuildId, NodeTree, NodeTreeBuilder};
use crate::source::SourceSpan;
use crate::state::Lang;

use alloc::string::ToString;

/// The root object of one parse: node building, diagnostics, and the recovery policy.
///
/// Fields are public: construct parsers reach the builder and diagnostics through
/// [`ParseContext::session`](crate::constructs::ParseContext) — the session *is* the
/// shared mutable surface of a parse (trees stay immutable; this is the mutation
/// boundary, consumed by [`finish`](ParserSession::finish)).
pub struct ParserSession<L: Lang> {
    /// The staging node builder.
    pub builder: NodeTreeBuilder<L>,
    /// The diagnostics accumulated so far.
    pub diagnostics: Diagnostics<L::SourceOrigin>,
    /// The tolerant-parsing policy in force.
    pub recovery: Recovery,
}

impl<L: Lang> ParserSession<L> {
    /// A fresh session under the given recovery policy.
    pub fn new(recovery: Recovery) -> ParserSession<L> {
        ParserSession {
            builder: NodeTreeBuilder::new(),
            diagnostics: Diagnostics::new(),
            recovery,
        }
    }

    /// The detection-site recovery helper (DESIGN_RATIONALE.md §3.8, rule 1): call it
    /// where a recoverable condition is detected, then — on `Ok(())` — continue with the
    /// site's local recovery (chars-node fallback, absent argument, skip, …).
    ///
    /// Under [`Recovery::Tolerant`], records `kind` as an error-severity [`Diagnostic`]
    /// at `span` and returns `Ok(())`. Under [`Recovery::Strict`], returns the condition
    /// as a [`ParseError`] to bubble — nobody continues past an `Err`.
    pub fn recover(
        &mut self,
        kind: ParseErrorKind,
        span: SourceSpan<L::SourceOrigin>,
    ) -> Result<(), ParseError<L::SourceOrigin>> {
        match self.recovery {
            Recovery::Tolerant => {
                self.diagnostics.push(Diagnostic::error(kind.to_string(), span));
                Ok(())
            }
            Recovery::Strict => Err(ParseError::new(kind, span)),
        }
    }

    /// Freeze the session: flatten everything reachable from `root` into the final
    /// [`NodeTree`] (resolving staged argument/slot regions) and hand over the
    /// diagnostics — available even for successful tolerant parses.
    pub fn finish(self, root: BuildId) -> ParseResult<L> {
        ParseResult { tree: self.builder.finish(root), diagnostics: self.diagnostics }
    }
}

impl<L: Lang> fmt::Debug for ParserSession<L> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ParserSession")
            .field("builder", &self.builder)
            .field("diagnostics", &self.diagnostics)
            .field("recovery", &self.recovery)
            .finish()
    }
}

/// A finished parse: the frozen tree plus everything reported along the way.
pub struct ParseResult<L: Lang> {
    /// The parsed document.
    pub tree: NodeTree<L>,
    /// The diagnostics recorded during the parse (possibly non-empty even on success —
    /// tolerant parsing).
    pub diagnostics: Diagnostics<L::SourceOrigin>,
}

impl<L: Lang> fmt::Debug for ParseResult<L> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ParseResult")
            .field("tree", &self.tree)
            .field("diagnostics", &self.diagnostics)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::constructs::{ConstructParser, ConstructParserResult, ParseContext};
    use crate::library::LibraryStack;
    use crate::node::NodeKind;
    use crate::source::{Source, Span};
    use crate::state::{
        Lang, ParsingState, ParsingStateDelta, ResolvedCallable, SimpleLang, StateData,
    };
    use crate::token::{Token, TokenKind, TokenListReader, TokenRules, WhitespaceRules};
    use alloc::string::String;
    use alloc::sync::Arc;
    use alloc::vec;
    use alloc::vec::Vec;

    #[derive(Debug, Clone, Copy)]
    struct PlainLang;
    impl SimpleLang for PlainLang {}

    fn min_rules() -> TokenRules<PlainLang> {
        TokenRules {
            whitespace: Some(WhitespaceRules { chars: " \t\n".into() }),
            multi_newline_paragraphs: true,
            groups: Vec::new(),
            commands: Vec::new(),
            comments: Vec::new(),
            forbidden_chars: String::new(),
            expecting_group_close: None,
        }
    }

    fn state() -> Arc<ParsingState<PlainLang>> {
        Arc::new(ParsingState::new(StateData {
            rules: min_rules(),
            libraries: LibraryStack::new(),
            ext: (),
        }))
    }

    fn span(source: &Arc<Source>, range: core::ops::Range<usize>) -> crate::source::SourceSpan {
        crate::source::SourceSpan::new(source, range)
    }

    #[test]
    fn recover_is_tolerant_or_strict() {
        let source: Arc<Source> = Arc::new(Source::new("abc"));

        let mut session: ParserSession<PlainLang> = ParserSession::new(Recovery::Tolerant);
        let kind = ParseErrorKind::Syntax { message: "unresolvable command ‘foo’".into() };
        assert!(session.recover(kind.clone(), span(&source, 0..3)).is_ok());
        assert_eq!(session.diagnostics.len(), 1);
        assert!(session.diagnostics.has_errors());
        assert_eq!(
            session.diagnostics.iter().next().unwrap().message(),
            "unresolvable command ‘foo’"
        );

        let mut session: ParserSession<PlainLang> = ParserSession::new(Recovery::Strict);
        let err = session.recover(kind.clone(), span(&source, 0..3)).unwrap_err();
        assert_eq!(*err.kind(), kind);
        assert_eq!(err.span().start(), 0);
        assert!(session.diagnostics.is_empty()); // strict mode records nothing
        // Display renders the condition's message; render() adds position info.
        assert_eq!(alloc::format!("{}", err), "unresolvable command ‘foo’");
        assert!(err.render().contains("line 1"));
    }

    #[test]
    fn parse_error_is_a_core_error() {
        fn assert_error<E: core::error::Error>() {}
        assert_error::<ParseError>();
    }

    /// A toy tier-2 construct parser: reads one `Char` token via the context, stages a
    /// `Chars` node, returns no delta. Exercises the full 6.1 plumbing —
    /// `ParseContext` over a `TokenListReader`, staging through the session's builder,
    /// `finish` into a `ParseResult`.
    struct OneCharParser {
        source: Arc<Source>,
    }

    impl ConstructParser<PlainLang> for OneCharParser {
        type Output = crate::node::BuildId;

        fn parse(
            &mut self,
            cx: &mut ParseContext<'_, '_, PlainLang>,
        ) -> ConstructParserResult<
            PlainLang,
            (Self::Output, Option<ParsingStateDelta<PlainLang>>),
        > {
            let token = cx.tokens.next(&cx.state).expect("test token stream is error-free");
            let TokenKind::Char(_) = token.kind else { panic!("test feeds a Char token") };
            let id = cx.session.builder.add(
                NodeKind::chars(token.span),
                crate::source::SourceSpan::new(&self.source, token.span.range()),
                cx.state.clone(),
                vec![],
            );
            Ok((id, None))
        }
    }

    #[test]
    fn construct_parser_plumbing_end_to_end() {
        let source: Arc<Source> = Arc::new(Source::new("q"));
        let st = state();
        let tokens: Vec<Token<'static, PlainLang>> =
            vec![Token::new(TokenKind::Char('q'), Span::new(0, 1), Span::empty(0))];
        let mut reader = TokenListReader::new(tokens);
        let mut session = ParserSession::new(Recovery::Tolerant);

        let mut cx = ParseContext {
            tokens: &mut reader,
            state: st.clone(),
            session: &mut session,
        };
        let mut parser = OneCharParser { source: source.clone() };
        let (id, delta) = parser.parse(&mut cx).unwrap();
        assert!(delta.is_none());
        assert_eq!(cx.tokens.pos(), 1);

        let result = session.finish(id);
        assert!(result.diagnostics.is_empty());
        assert_eq!(result.tree.root().chars(), Some("q"));
    }

    #[test]
    fn context_recover_forwards_to_the_session() {
        let source: Arc<Source> = Arc::new(Source::new("x"));
        let st = state();
        let mut reader: TokenListReader<'static, PlainLang> = TokenListReader::new(vec![]);
        let mut session = ParserSession::new(Recovery::Tolerant);
        let mut cx = ParseContext { tokens: &mut reader, state: st, session: &mut session };

        let kind = ParseErrorKind::Syntax { message: "boom".into() };
        assert!(cx.recover(kind, span(&source, 0..1)).is_ok());
        assert_eq!(session.diagnostics.len(), 1);
    }

    // --- the Phase 6 Lang hook defaults ------------------------------------------------

    #[test]
    fn default_resolve_command_resolves_nothing() {
        let st = state();
        let token: Token<'static, PlainLang> = Token::new(
            TokenKind::Command { name: "foo", escape_char: '\\', post_space: Span::empty(4) },
            Span::new(0, 4),
            Span::empty(0),
        );
        let resolved: Option<ResolvedCallable<PlainLang>> =
            PlainLang::resolve_command(&st, &token);
        assert!(resolved.is_none());
    }

    #[test]
    fn default_paragraph_break_node_is_spanned_whitespace_chars() {
        let st = state();
        let token: Token<'static, PlainLang> =
            Token::new(TokenKind::ParagraphBreak, Span::new(3, 5), Span::new(1, 3));
        let kind = PlainLang::make_paragraph_break_node(&st, &token);
        match kind {
            NodeKind::Chars { content, .. } => {
                // Span-backed over the full token span (newlines included), per the
                // whitespace-as-chars invariant (§3.5).
                assert!(!content.is_owned());
                assert_eq!(content.resolve("x  \n\nz"), "\n\n");
            }
            other => panic!("expected a Chars kind, got {:?}", other),
        }
    }
}
