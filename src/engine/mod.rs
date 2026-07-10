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
use crate::state::{Lang, ParsingState, ParsingStateDelta, TokenRulesOverrides};
use crate::token::GroupRule;

use alloc::string::ToString;
use alloc::sync::Arc;
use alloc::vec::Vec;

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
    /// The parse-global mutable language extension ([`Lang::SessionExt`],
    /// `Default`-initialized): the preset-owned mutable object of a parse — transition
    /// observation counters ([`Lang::observe_transition`]), parse-global caches.
    pub ext: L::SessionExt,
    /// The group-interior-state memo (DESIGN_RATIONALE.md §3.6): `(base, rule, interior)`
    /// triples matched by `Arc` pointer identity, consulted only by
    /// [`group_interior_state`](ParserSession::group_interior_state) — the one derivation
    /// whose inputs recur with `Arc` identity. Entries hold their key `Arc`s alive, so
    /// pointer keys cannot be reused (no ABA hazard).
    group_interior_memo: GroupInteriorMemo<L>,
}

type GroupInteriorMemo<L> =
    Vec<(Arc<ParsingState<L>>, Arc<GroupRule<L>>, Arc<ParsingState<L>>)>;

impl<L: Lang> ParserSession<L> {
    /// A fresh session under the given recovery policy.
    pub fn new(recovery: Recovery) -> ParserSession<L> {
        ParserSession {
            builder: NodeTreeBuilder::new(),
            diagnostics: Diagnostics::new(),
            recovery,
            ext: Default::default(),
            group_interior_memo: Vec::new(),
        }
    }

    /// Session-mediated state derivation — the in-parse standard (DESIGN_RATIONALE.md
    /// §3.6): within a parse frame, construct parsers derive states through this seam so
    /// every transition event reaches [`Lang::observe_transition`] (with the session's
    /// [`ext`](ParserSession::ext)). **Data-equivalent to
    /// [`ParsingState::derived`]** — the session layer may observe, never alter the
    /// resulting state — and **never memoized**: an arbitrary delta has no identity to
    /// key a memo on (keyed helpers exist where derivation inputs have `Arc` identity —
    /// [`group_interior_state`](ParserSession::group_interior_state)).
    ///
    /// Out-of-parse code (initial states, tests, tree transforms) keeps calling
    /// `derived()` directly.
    pub fn derived_state(
        &mut self,
        base: &Arc<ParsingState<L>>,
        delta: &ParsingStateDelta<L>,
    ) -> Arc<ParsingState<L>> {
        let new = Arc::new(base.derived(delta));
        L::observe_transition(&mut self.ext, base, &new, delta);
        new
    }

    /// The group-interior derivation, deduplicated (DESIGN_RATIONALE.md §3.6): the state
    /// a group's interior is parsed under is always `base` + `expecting_group_close =
    /// rule` — the uniform invariant that guarantees the close delimiter stays
    /// recognizable — and sibling groups under one state repeat the identical
    /// derivation, the dominant state-cloning cost in deep documents. This helper keeps
    /// always-derive *semantics* but consults a `(base, rule)` pointer-identity memo, so
    /// one `StateData` clone (and one [`Lang::finalize_transition`] run) serves every
    /// sibling descent; [`Lang::observe_transition`] still fires on **every** call, memo
    /// hits included.
    pub fn group_interior_state(
        &mut self,
        base: &Arc<ParsingState<L>>,
        rule: &Arc<GroupRule<L>>,
    ) -> Arc<ParsingState<L>> {
        let delta = ParsingStateDelta::new().rules(TokenRulesOverrides {
            expecting_group_close: Some(Some(Arc::clone(rule))),
            ..TokenRulesOverrides::default()
        });
        let memoized = self
            .group_interior_memo
            .iter()
            .find(|(b, r, _)| Arc::ptr_eq(b, base) && Arc::ptr_eq(r, rule))
            .map(|(_, _, interior)| Arc::clone(interior));
        let interior = match memoized {
            Some(interior) => interior,
            None => {
                let interior = Arc::new(base.derived(&delta));
                self.group_interior_memo.push((
                    Arc::clone(base),
                    Arc::clone(rule),
                    Arc::clone(&interior),
                ));
                interior
            }
        };
        L::observe_transition(&mut self.ext, base, &interior, &delta);
        interior
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
            .field("ext", &self.ext)
            .field("group_interior_memo", &self.group_interior_memo.len())
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
        TokenRulesOverrides,
    };
    use crate::token::{
        GroupRule, Token, TokenKind, TokenListReader, TokenRules, WhitespaceRules,
    };
    use alloc::string::String;
    use alloc::sync::Arc;
    use alloc::vec;
    use alloc::vec::Vec;

    #[derive(Debug, Clone, Copy)]
    struct PlainLang;
    impl SimpleLang for PlainLang {}

    fn min_rules<L: Lang<GroupTypeId = u32>>() -> TokenRules<L> {
        TokenRules {
            enable_whitespace: true,
            whitespace: WhitespaceRules { chars: " \t\n".into() },
            enable_multi_newline_paragraphs: true,
            enable_groups: true,
            groups: Vec::new(),
            enable_commands: true,
            commands: Vec::new(),
            enable_comments: true,
            comments: Vec::new(),
            enable_specials: true,
            forbidden_chars: String::new(),
            expecting_group_close: None,
        }
    }

    fn state<L: Lang<GroupTypeId = u32, StateExt = ()>>() -> Arc<ParsingState<L>> {
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
    struct OneCharParser;

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
                crate::source::SourceSpan::new(&cx.source, token.span.range()),
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
            source: source.clone(),
            state: st.clone(),
            session: &mut session,
        };
        let mut parser = OneCharParser;
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
        let mut cx = ParseContext {
            tokens: &mut reader,
            source: source.clone(),
            state: st,
            session: &mut session,
        };

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

    // --- the session-mediated derivation seam (6.3, DESIGN_RATIONALE.md §3.6) ----------

    #[derive(Debug, Default)]
    struct Observed {
        transitions: usize,
    }

    #[derive(Debug, Clone, Copy)]
    struct ObserverLang;
    impl Lang for ObserverLang {
        type GroupTypeId = u32;
        type CallableTypeId = u32;
        type StateExt = ();
        type Event = ();
        type SessionExt = Observed;
        type SourceOrigin = Option<String>;
        type NodeExts = ();

        fn observe_transition(
            ext: &mut Observed,
            _prev: &ParsingState<Self>,
            _new: &ParsingState<Self>,
            _delta: &ParsingStateDelta<Self>,
        ) {
            ext.transitions += 1;
        }
    }

    #[test]
    fn derived_state_is_data_equivalent_and_observed_but_never_memoized() {
        let base: Arc<ParsingState<ObserverLang>> = state();
        let delta = ParsingStateDelta::new().rules(TokenRulesOverrides {
            enable_comments: Some(false),
            ..TokenRulesOverrides::default()
        });
        let mut session: ParserSession<ObserverLang> = ParserSession::new(Recovery::Strict);

        let via_session = session.derived_state(&base, &delta);
        // Data-equivalent to the pure transition…
        assert_eq!(via_session.rules(), base.derived(&delta).rules());
        // …with the transition event observed.
        assert_eq!(session.ext.transitions, 1);

        // Never memoized: an identical call derives a fresh state (an arbitrary delta
        // has no identity to key a memo on).
        let again = session.derived_state(&base, &delta);
        assert!(!Arc::ptr_eq(&via_session, &again));
        assert_eq!(session.ext.transitions, 2);
    }

    #[test]
    fn group_interior_state_memoizes_by_pointer_identity_and_observes_hits() {
        let base: Arc<ParsingState<ObserverLang>> = state();
        let rule: Arc<GroupRule<ObserverLang>> =
            Arc::new(GroupRule { group_type: 0, open: "{".into(), close: "}".into() });
        let mut session: ParserSession<ObserverLang> = ParserSession::new(Recovery::Strict);

        let first = session.group_interior_state(&base, &rule);
        assert!(first
            .rules()
            .expecting_group_close
            .as_ref()
            .is_some_and(|expected| Arc::ptr_eq(expected, &rule)));

        // Memo hit: the same interior Arc, and the transition event still observed.
        let second = session.group_interior_state(&base, &rule);
        assert!(Arc::ptr_eq(&first, &second));
        assert_eq!(session.ext.transitions, 2);

        // Keys are pointer identities: an equal-but-distinct rule Arc is a fresh
        // derivation, and so is a distinct base.
        let equal_rule: Arc<GroupRule<ObserverLang>> =
            Arc::new(GroupRule { group_type: 0, open: "{".into(), close: "}".into() });
        let third = session.group_interior_state(&base, &equal_rule);
        assert!(!Arc::ptr_eq(&first, &third));
        let other_base = Arc::new(base.derived(&ParsingStateDelta::new()));
        let fourth = session.group_interior_state(&other_base, &rule);
        assert!(!Arc::ptr_eq(&first, &fourth));
        assert_eq!(session.ext.transitions, 4);
    }

    #[test]
    fn session_ext_is_default_initialized() {
        let session: ParserSession<ObserverLang> = ParserSession::new(Recovery::Tolerant);
        assert_eq!(session.ext.transitions, 0);
    }
}
