//! Attached-source parsing: the [`ParseContext::parse_attached_source`] door and
//! the [`ParseContext::attach_source_reference`] resolve-diagnose-attach bundle —
//! the engine half of `\input`-style inclusion ([`SourceResolver`] is the lookup
//! half).
//!
//! The door sub-parses an already-minted [`Source`] **into the running session**:
//! included content must parse under the parsing state *at the inclusion point*,
//! which the running session has naturally — and the staged nodes join the same
//! builder, so a multi-source tree is one tree (`BuildId`s are session-global).
//! Slot assembly stays the calling invocation parser's job: the door returns
//! content nodes, and the caller stages them (for `\input`, as an
//! [`Attached`](crate::node::SlotRole::Attached) slot of its callable node).
//!
//! Recursion control is deliberately **not** here: the core never interprets
//! reference strings, and legitimate self-inclusion exists (`.dtx`-style
//! self-documenting files). An embedder that wants a cycle or depth bound
//! enforces it in its resolver —
//! [`check_include_chain`](crate::source::check_include_chain) and
//! [`Source::including_sources`] are the ready-made tools.

use alloc::string::{String, ToString};
use alloc::sync::Arc;
use alloc::vec::Vec;

use crate::engine::{Frame, FrameTitle};
use crate::error::DiagnosticInfo;
use crate::node::{BuildId, NodeKind};
use crate::source::{resolve_source_reference, ResolveError, Source, SourceSpan};
use crate::state::{Lang, ParsingState};
use crate::token::StdTokenReader;

use super::{
    ConstructParser, ConstructParserResult, NodesOutcome, ParseContext, StopCause,
    StrayGroupClose,
};

impl<L: Lang> ParseContext<'_, '_, L> {
    /// Parse `source` as **attached content** of the construct being parsed — the
    /// sub-parse door of `\input`-style inclusion: a fresh inner context over its
    /// own reader (the outer reader stays untouched, pinned to the outer source),
    /// the **same session and builder** (the staged nodes belong to the running
    /// parse; `BuildId`s are session-global), and `state` as the sub-parse's input
    /// state — for inclusion semantics, the state at the inclusion point
    /// (`Arc::clone(&cx.state)`).
    ///
    /// The caller supplies the construct parser driving the sub-parse; for
    /// `\input`-style inclusion that is the root nodes-parse shape,
    /// `&mut *cx.driver.make_nodes_parser(StopSpec::none(),
    /// ChildStateSpec::inherit())`. The parser speaks the nodes-run vocabulary
    /// ([`NodesOutcome`]) so the door can drive it like
    /// [`Language::parse_source`](crate::engine::Language::parse_source) drives the
    /// root loop, and it **must tolerate re-invocation**: after a recovered stop
    /// the door calls `parse` again to resume (the standard
    /// [`NodesParser`](super::NodesParser) does — its working state drains at every
    /// return).
    ///
    /// Returns the staged **content nodes only** — no wrapper `List`, no slot
    /// record: slot assembly stays the invocation parser's job (the one staging
    /// door, [`stage_node`](ParseContext::stage_node), holds). The whole sub-parse
    /// runs under a traceback [`Frame`] (`attached source`, anchored at the
    /// source's [`triggered_at`](crate::source::SourceProvenance::triggered_at)
    /// when it has one), so conditions inside the attached content point back at
    /// the inclusion.
    ///
    /// # Recovery
    ///
    /// A stray group close in the attached source is recovered **locally**: it is
    /// diagnosed as [`StrayGroupClose`] through the recover funnel, consumed, and
    /// staged as a span-backed `Chars` node (the markup-in-chars recovery
    /// artifact — per-source byte accounting stays exact), and the parser is
    /// re-invoked — an included file's stray `}` never unwinds the includer's own
    /// group structure. Under
    /// [`Recovery::Strict`](crate::error::Recovery::Strict) the funnel aborts the
    /// parse as usual (`Err` bubbles). A stop on the parser's **own** stop
    /// conditions ends the sub-parse normally — unlike the root loop, the door
    /// does not know the caller's stop spec, so a token/node-condition stop means
    /// "this run is complete" (content past it stays unparsed, the caller's
    /// choice). Any pass-through state delta of the sub-parse is discarded: the
    /// door returns nodes, and the attached content's internal after-effects do
    /// not continue into the including document.
    pub fn parse_attached_source<P>(
        &mut self,
        source: Arc<Source<L::SourceOrigin>>,
        state: Arc<ParsingState<L>>,
        parser: &mut P,
    ) -> ConstructParserResult<L, Vec<BuildId>>
    where
        P: ConstructParser<L, Output = NodesOutcome<L>> + ?Sized,
    {
        // The traceback frame: anchored at the inclusion site when the source's
        // provenance records one (the rendered report's provenance-chain section
        // names the reference itself, so a static title carries no less
        // information and stays allocation-free to push).
        let frame = Frame {
            title: FrameTitle::Static("attached source"),
            span: source
                .provenance()
                .triggered_at()
                .cloned()
                .unwrap_or_else(|| SourceSpan::new(&source, 0..0)),
        };

        // The fresh inner context: its own reader over the attached content, the
        // same session/builder/driver. The outer reader (and the outer ambient
        // state) are untouched — `self` is shadowed for the sub-parse's extent.
        let mut reader = StdTokenReader::new(source.content());
        let mut inner = ParseContext::new(
            &mut reader,
            Arc::clone(&source),
            state,
            &mut *self.session,
            self.driver,
        );

        inner.with_frame(frame, |cx| {
            let mut nodes: Vec<BuildId> = Vec::new();
            loop {
                let (outcome, _delta) =
                    cx.parse_scoped(Arc::clone(&cx.state), &mut *parser)?;
                nodes.extend(outcome.nodes);
                // Thread the segment's exit state (the root-loop discipline):
                // resume and diagnosis run under the state the run reached.
                cx.state = outcome.state;
                match outcome.stop {
                    StopCause::EndOfInput => break,
                    StopCause::UnexpectedGroupClose { span } => {
                        // Local recovery: diagnose, consume, stage the delimiter
                        // as chars, resume — the includer never sees the close.
                        let delim = span.slice(cx.source.content()).to_string();
                        cx.recover(
                            StrayGroupClose { delim },
                            SourceSpan::new(&cx.source, span),
                        )?;
                        cx.tokens.move_to_pos(span.end());
                        let id = cx
                            .stage_node(
                                NodeKind::chars(span),
                                SourceSpan::new(&cx.source, span),
                                Arc::clone(&cx.state),
                                Vec::new(),
                            )
                            .map_err(|error| cx.implementation_error(error, span))?;
                        nodes.push(id);
                    }
                    // The caller-supplied parser's own stop conditions ended the
                    // run — the door does not second-guess them (see above).
                    StopCause::TokenCondition { .. } | StopCause::NodeCondition => break,
                }
            }
            Ok(nodes)
        })
    }

    /// Resolve `reference` and parse the resolved source as attached content —
    /// the **single resolve-diagnose-attach raising site** of `\input`-style
    /// inclusion, so the two failure conditions carry one wording across every
    /// `\input`-variant spec and framework:
    ///
    /// - no resolver configured on the driver
    ///   ([`ParseDriver::source_resolver`](crate::engine::ParseDriver::source_resolver)
    ///   is `None`) → [`NoSourceResolver`];
    /// - the resolver returned an error → [`UnresolvableSourceReference`]
    ///   (reference + the live [`ResolveError`], cause chain included).
    ///
    /// Both are raised through the recover funnel at `at` — the invocation span
    /// of the triggering construct, which is also what the minted source's
    /// provenance records as its
    /// [`triggered_at`](crate::source::SourceProvenance::triggered_at). Under
    /// [`Recovery::Tolerant`](crate::error::Recovery::Tolerant) the failure is
    /// recorded and `Ok(None)` is returned — nothing was attached, and the
    /// calling spec stages its callable without an attached slot; under
    /// [`Recovery::Strict`](crate::error::Recovery::Strict) the funnel aborts.
    ///
    /// On success, delegates to
    /// [`parse_attached_source`](ParseContext::parse_attached_source) (whose
    /// parser/state/recovery contracts apply) and returns `Ok(Some(nodes))`.
    /// Resolution itself stays outside the door — the composition is accessor →
    /// [`resolve_source_reference`] → door, so a caching framework can substitute
    /// either half by composing the pieces itself.
    pub fn attach_source_reference<P>(
        &mut self,
        reference: &str,
        at: &SourceSpan<L::SourceOrigin>,
        state: Arc<ParsingState<L>>,
        parser: &mut P,
    ) -> ConstructParserResult<L, Option<Vec<BuildId>>>
    where
        P: ConstructParser<L, Output = NodesOutcome<L>> + ?Sized,
    {
        // The driver field is a shared reference with the context's own lifetime:
        // copy it out so the resolver borrow is independent of `&mut self`.
        let driver = self.driver;
        let Some(resolver) = crate::engine::ParseDriver::source_resolver(driver) else {
            self.recover(NoSourceResolver::new(reference), at.clone())?;
            return Ok(None);
        };
        match resolve_source_reference(resolver, reference, at) {
            Ok(source) => self.parse_attached_source(source, state, parser).map(Some),
            Err(error) => {
                self.recover(UnresolvableSourceReference::new(reference, error), at.clone())?;
                Ok(None)
            }
        }
    }
}

/// Condition: an `\input`-style construct referenced an external source, but no
/// [`SourceResolver`](crate::source::SourceResolver) is configured — the driver's
/// [`source_resolver`](crate::engine::ParseDriver::source_resolver) accessor
/// returned `None` ("this language resolves nothing"). Raised by
/// [`ParseContext::attach_source_reference`]; distinct from
/// [`UnresolvableSourceReference`] (a configured resolver that *failed*) — the
/// remedies differ: configure a resolver vs. fix the reference/environment.
#[derive(Debug, Clone, PartialEq, Eq, DiagnosticInfo)]
#[non_exhaustive]
#[diagnostic(
    id = "core.sources.no-resolver",
    message = "cannot resolve source reference ‘{reference}’: no source resolver \
               is configured on this language’s driver"
)]
pub struct NoSourceResolver {
    /// The reference that could not be resolved (e.g. the file name), as written.
    pub reference: String,
}

/// Condition: the configured [`SourceResolver`](crate::source::SourceResolver)
/// failed to resolve an external source reference. Raised by
/// [`ParseContext::attach_source_reference`]; carries the live [`ResolveError`],
/// so embedders can downcast along its
/// [`Error::source`](core::error::Error::source) chain (e.g. to an `io::Error`)
/// — the serialized projection renders the reference, the message, and the cause
/// chain. Distinct from [`NoSourceResolver`] (no resolver configured at all).
#[derive(Debug, Clone, DiagnosticInfo)]
#[non_exhaustive]
#[diagnostic(id = "core.sources.unresolvable-reference", message = "{error}")]
pub struct UnresolvableSourceReference {
    /// The reference that failed to resolve, as written.
    pub reference: String,
    /// The resolver's failure ([`Clone`] — the cause sits behind an `Arc`).
    pub error: ResolveError,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::constructs::{ChildStateSpec, StopSpec};
    use crate::engine::{ParseDriver, ParserSession, StdParseDriver};
    use crate::error::Recovery;
    use crate::scopes::ScopeStack;
    use crate::source::MapResolver;
    use crate::state::{ParsingState, StateData, TrivialLang};
    use crate::token::{
        CommandRule, GroupRule, TokenRules, WhitespaceRules,
    };
    use alloc::boxed::Box;
    use alloc::vec;
    use alloc::vec::Vec;

    /// A brace-group test lang over the std driver (commands off, groups on) —
    /// the DocLang shape of the engine tests.
    #[derive(Debug, Clone, Copy)]
    struct DocLang;
    impl Lang for DocLang {
        type GroupTypeId = u32;
        type CallableTypeId = u32;
        type ModeId = ();
        type StateExt = ();
        type Event = ();
        type SessionExt = ();
        type SourceOrigin = Option<String>;
        type NodeExts = ();
        type InvocationSyntax = ();
        type Driver = StdParseDriver;

        fn initial_state_data() -> StateData<Self> {
            StateData {
                rules: TokenRules {
                    enable_whitespace: true,
                    whitespace: WhitespaceRules { chars: " \t\n".into() },
                    enable_multi_newline_paragraphs: true,
                    enable_groups: true,
                    groups: vec![Arc::new(GroupRule {
                        group_type: 0,
                        open: "{".into(),
                        close: "}".into(),
                    })],
                    temporary_groups: Vec::new(),
                    enable_commands: true,
                    commands: vec![Arc::new(CommandRule {
                        escape_char: '\\',
                        name_chars:
                            "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ".into(),
                    })],
                    enable_comments: false,
                    comments: Vec::new(),
                    enable_specials: false,
                    forbidden_chars: "".into(),
                    expecting_group_close: None,
                },
                scopes: ScopeStack::new(),
                mode: (),
                ext: (),
            }
        }
        fn make_node_ext(
            _kind: &NodeKind<Self>,
            _span: &SourceSpan<Self::SourceOrigin>,
            _state: &Arc<ParsingState<Self>>,
            _children: crate::node::StagedChildren<'_, Self>,
        ) {
        }
    }

    fn driver(recovery: Recovery) -> StdParseDriver {
        StdParseDriver::new(recovery, ())
    }

    fn driver_resolving(recovery: Recovery, entries: &[(&str, &str)]) -> StdParseDriver {
        let mut resolver = MapResolver::new();
        for (reference, content) in entries {
            resolver.insert(*reference, *content);
        }
        StdParseDriver::new(recovery, ())
            .with_source_resolver(resolver.with_reference_as_origin())
    }

    /// Run `f` inside a context over `content` (an outer primary source).
    fn with_context<R>(
        driver: &StdParseDriver,
        content: &str,
        f: impl FnOnce(&mut ParseContext<'_, '_, DocLang>) -> R,
    ) -> (R, ParserSession<DocLang>) {
        let source: Arc<Source> = Arc::new(Source::new(content));
        let mut reader = StdTokenReader::new(source.content());
        let mut session: ParserSession<DocLang> = ParserSession::new();
        let state = Arc::new(ParsingState::<DocLang>::lang_initial());
        let result = {
            let mut cx =
                ParseContext::new(&mut reader, Arc::clone(&source), state, &mut session, driver);
            f(&mut cx)
        };
        (result, session)
    }

    /// The nodes-run parser the door is driven with in `\input` position.
    fn nodes_parser<'p>(
        driver: &'p StdParseDriver,
    ) -> Box<dyn ConstructParser<DocLang, Output = NodesOutcome<DocLang>> + 'p> {
        ParseDriver::<DocLang>::make_nodes_parser(
            driver,
            StopSpec::none(),
            ChildStateSpec::inherit(),
        )
    }

    #[test]
    fn identifiers_are_the_ruled_slate_strings() {
        assert_eq!(NoSourceResolver::IDENTIFIER, "core.sources.no-resolver");
        assert_eq!(
            UnresolvableSourceReference::IDENTIFIER,
            "core.sources.unresolvable-reference"
        );
    }

    #[test]
    fn the_door_parses_attached_content_into_the_same_session() {
        let driver = driver(Recovery::Strict);
        let (nodes, session) = with_context(&driver, r"\input{chapter.tex}", |cx| {
            let trigger = SourceSpan::entire(&cx.source);
            let attached: Arc<Source> =
                Arc::new(Source::resolved("hello {world}", "chapter.tex", trigger));
            let mut parser = nodes_parser(cx.driver);
            let outer_pos_before = cx.tokens.pos();
            let nodes = cx
                .parse_attached_source(
                    Arc::clone(&attached),
                    Arc::clone(&cx.state),
                    &mut *parser,
                )
                .unwrap();
            // The outer reader did not move: the sub-parse ran a fresh inner one.
            assert_eq!(cx.tokens.pos(), outer_pos_before);
            // Content nodes only, staged into the *same* builder, with spans in
            // the attached source.
            let staged = cx.staged_nodes();
            for id in &nodes {
                let view = staged.get(*id).unwrap();
                assert!(Arc::ptr_eq(view.span().source(), &attached));
            }
            assert_eq!(staged.get(nodes[0]).unwrap().span().range(), 0..6);
            nodes
        });
        assert_eq!(nodes.len(), 2); // "hello " chars + the group
        assert!(session.diagnostics.is_empty());
    }

    #[test]
    fn a_stray_close_in_the_attached_source_recovers_locally() {
        // Tolerant: the included file's stray `}` is diagnosed, staged as chars,
        // and the run resumes — the includer is never unwound.
        let driver = driver(Recovery::Tolerant);
        let (nodes, session) = with_context(&driver, "outer", |cx| {
            let trigger = SourceSpan::new(&cx.source, 0..5);
            let attached: Arc<Source> =
                Arc::new(Source::resolved("a}b", "frag.tex", trigger));
            let mut parser = nodes_parser(cx.driver);
            let nodes = cx
                .parse_attached_source(attached, Arc::clone(&cx.state), &mut *parser)
                .unwrap();
            let staged = cx.staged_nodes();
            let texts: Vec<String> = nodes
                .iter()
                .map(|id| staged.get(*id).unwrap().span().content().into())
                .collect();
            assert_eq!(texts, ["a", "}", "b"]);
            nodes
        });
        assert_eq!(nodes.len(), 3);
        assert_eq!(session.diagnostics.len(), 1);
        let diagnostic = session.diagnostics.iter().next().unwrap();
        assert_eq!(diagnostic.identifier(), StrayGroupClose::IDENTIFIER);
        // The condition carries the door's traceback frame, anchored at the
        // inclusion site in the *outer* source.
        assert_eq!(diagnostic.frames().last().unwrap().title(), "attached source");
        assert_eq!(diagnostic.frames().last().unwrap().span().range(), 0..5);

        // Strict: the funnel aborts, as everywhere.
        let strict = self::driver(Recovery::Strict);
        let (result, _session) = with_context(&strict, "outer", |cx| {
            let trigger = SourceSpan::new(&cx.source, 0..5);
            let attached: Arc<Source> =
                Arc::new(Source::resolved("a}b", "frag.tex", trigger));
            let mut parser = nodes_parser(cx.driver);
            cx.parse_attached_source(attached, Arc::clone(&cx.state), &mut *parser)
        });
        assert_eq!(result.unwrap_err().identifier(), StrayGroupClose::IDENTIFIER);
    }

    #[test]
    fn core_performs_no_recursion_checking_self_inclusion_is_legal() {
        // A source whose provenance chain already contains its own origin — the
        // `.dtx` self-inclusion shape. The door parses it without complaint:
        // recursion policy belongs to the embedder's resolver
        // ([`check_include_chain`]), never to the core.
        let driver = driver(Recovery::Strict);
        let ((), session) = with_context(&driver, "outer", |cx| {
            let trigger = SourceSpan::new(&cx.source, 0..5);
            let first: Arc<Source> = Arc::new(
                Source::resolved("self", "self.tex", trigger)
                    .with_origin(Some("self.tex".into())),
            );
            let second: Arc<Source> = Arc::new(
                Source::resolved("self again", "self.tex", SourceSpan::entire(&first))
                    .with_origin(Some("self.tex".into())),
            );
            let mut parser = nodes_parser(cx.driver);
            let nodes = cx
                .parse_attached_source(second, Arc::clone(&cx.state), &mut *parser)
                .unwrap();
            assert_eq!(nodes.len(), 1);
        });
        assert!(session.diagnostics.is_empty());
    }

    #[test]
    fn attach_without_a_resolver_diagnoses_no_source_resolver() {
        // Tolerant: recorded at `at`, Ok(None) — nothing attached.
        let tolerant = driver(Recovery::Tolerant);
        let (result, session) = with_context(&tolerant, r"\input{chapter.tex}", |cx| {
            let at = SourceSpan::entire(&cx.source);
            let mut parser = nodes_parser(cx.driver);
            cx.attach_source_reference(
                "chapter.tex",
                &at,
                Arc::clone(&cx.state),
                &mut *parser,
            )
        });
        assert_eq!(result.unwrap(), None);
        assert_eq!(session.diagnostics.len(), 1);
        let diagnostic = session.diagnostics.iter().next().unwrap();
        assert_eq!(diagnostic.identifier(), NoSourceResolver::IDENTIFIER);
        assert_eq!(diagnostic.span().range(), 0..19);
        let condition =
            diagnostic.data().downcast_ref::<NoSourceResolver>().unwrap();
        assert_eq!(condition.reference, "chapter.tex");
        assert_eq!(
            diagnostic.message(),
            "cannot resolve source reference ‘chapter.tex’: no source resolver \
             is configured on this language’s driver"
        );

        // Strict: the funnel aborts with the condition.
        let strict = driver(Recovery::Strict);
        let (result, _session) = with_context(&strict, r"\input{chapter.tex}", |cx| {
            let at = SourceSpan::entire(&cx.source);
            let mut parser = nodes_parser(cx.driver);
            cx.attach_source_reference(
                "chapter.tex",
                &at,
                Arc::clone(&cx.state),
                &mut *parser,
            )
        });
        assert_eq!(result.unwrap_err().identifier(), NoSourceResolver::IDENTIFIER);
    }

    #[test]
    fn a_failing_resolver_diagnoses_unresolvable_reference_with_the_live_error() {
        let tolerant = driver_resolving(Recovery::Tolerant, &[]); // empty map
        let (result, session) = with_context(&tolerant, r"\input{missing.tex}", |cx| {
            let at = SourceSpan::entire(&cx.source);
            let mut parser = nodes_parser(cx.driver);
            cx.attach_source_reference(
                "missing.tex",
                &at,
                Arc::clone(&cx.state),
                &mut *parser,
            )
        });
        assert_eq!(result.unwrap(), None);
        assert_eq!(session.diagnostics.len(), 1);
        let diagnostic = session.diagnostics.iter().next().unwrap();
        assert_eq!(
            diagnostic.identifier(),
            UnresolvableSourceReference::IDENTIFIER
        );
        let condition = diagnostic
            .data()
            .downcast_ref::<UnresolvableSourceReference>()
            .unwrap();
        assert_eq!(condition.reference, "missing.tex");
        assert_eq!(condition.error.reference(), "missing.tex");
        // The message is the ResolveError's own rendering.
        assert_eq!(
            diagnostic.message(),
            "cannot resolve source reference 'missing.tex': no entry for this reference"
        );
    }

    #[test]
    fn attach_resolves_and_parses_with_provenance_stamped_at_the_trigger() {
        let driver =
            driver_resolving(Recovery::Strict, &[("chapter.tex", "chapter {content}")]);
        let ((), session) = with_context(&driver, r"\input{chapter.tex} tail", |cx| {
            let at = SourceSpan::new(&cx.source, 0..19);
            let mut parser = nodes_parser(cx.driver);
            let nodes = cx
                .attach_source_reference(
                    "chapter.tex",
                    &at,
                    Arc::clone(&cx.state),
                    &mut *parser,
                )
                .unwrap()
                .expect("resolved and attached");
            assert_eq!(nodes.len(), 2);
            let staged = cx.staged_nodes();
            let attached_source =
                Arc::clone(staged.get(nodes[0]).unwrap().span().source());
            // The minted source records this trigger and the resolver's origin.
            match attached_source.provenance() {
                crate::source::SourceProvenance::Resolved { reference, triggered_at } => {
                    assert_eq!(reference, "chapter.tex");
                    assert_eq!(triggered_at, &at);
                }
                other => panic!("expected Resolved provenance, got {:?}", other),
            }
            assert_eq!(
                attached_source.origin().as_deref(),
                Some("chapter.tex")
            );
        });
        assert!(session.diagnostics.is_empty());
    }

    #[test]
    fn serializable_data_renders_reference_message_and_cause_chain() {
        use crate::error::{DiagnosticValue, ToDiagnosticValue};

        #[derive(Debug)]
        struct Underlying;
        impl core::fmt::Display for Underlying {
            fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
                f.write_str("file not found (os error 2)")
            }
        }
        impl core::error::Error for Underlying {}

        let condition = UnresolvableSourceReference::new(
            "chapter.tex",
            ResolveError::new("chapter.tex", "cannot read file").with_cause(Underlying),
        );
        let projected = crate::error::DiagnosticInfo::serializable_data(&condition);
        let DiagnosticValue::Map(fields) = projected else {
            panic!("expected a map projection")
        };
        assert_eq!(
            fields[0],
            ("reference".into(), DiagnosticValue::Str("chapter.tex".into()))
        );
        let DiagnosticValue::Map(error_fields) = &fields[1].1 else {
            panic!("expected the error field to project as a map")
        };
        assert_eq!(
            error_fields[1],
            ("message".into(), DiagnosticValue::Str("cannot read file".into()))
        );
        assert_eq!(
            error_fields[2],
            (
                "cause-chain".into(),
                DiagnosticValue::List(vec![DiagnosticValue::Str(
                    "file not found (os error 2)".into()
                )])
            )
        );
        // The stand-alone projection of the error type itself matches.
        assert_eq!(fields[1].1, condition.error.to_diagnostic_value());
    }

    #[test]
    fn trivial_lang_smoke_the_default_driver_resolves_nothing() {
        // The `()`-resolver pairing: no resolver, so the bundle diagnoses — the
        // resolver-choice pairing documented with `TrivialLang`.
        #[derive(Debug, Clone, Copy)]
        struct PlainLang;
        impl TrivialLang for PlainLang {}

        let driver: StdParseDriver = StdParseDriver::new(Recovery::Tolerant, ());
        let source: Arc<Source> = Arc::new(Source::new("x"));
        let mut reader = StdTokenReader::new(source.content());
        let mut session: ParserSession<PlainLang> = ParserSession::new();
        let state = Arc::new(ParsingState::<PlainLang>::lang_initial());
        let mut cx = ParseContext::new(&mut reader, source.clone(), state, &mut session, &driver);
        let at = SourceSpan::entire(&cx.source);
        let mut parser = ParseDriver::<PlainLang>::make_nodes_parser(
            &driver,
            StopSpec::none(),
            ChildStateSpec::inherit(),
        );
        let attached = cx
            .attach_source_reference("a.tex", &at, Arc::clone(&cx.state), &mut *parser)
            .unwrap();
        assert_eq!(attached, None);
        assert_eq!(session.diagnostics.len(), 1);
    }
}
