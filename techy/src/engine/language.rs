//! [`Language<L>`]: the long-lived bundle a parse starts from, and the entry points
//! that run one.
//!
//! A `Language` holds what outlives any one parse — the frozen initial
//! [`ParsingState`] and the [`ParseDriver`](crate::engine::ParseDriver) instance —
//! and nothing else, so one value parses many documents. Everything one parse
//! accumulates belongs to the transient [`ParserSession`], and the finished
//! [`ParseResult`] owns its tree and diagnostics outright: it borrows nothing from
//! the `Language`.

use core::fmt;

use alloc::string::{String, ToString};
use alloc::sync::Arc;

use crate::constructs::{
    ConstructParser, FromInvocation, ImplementationError, ParseContext,
};
use crate::error::{Diagnostics, ParseError};
use crate::node::BuildId;
use super::descent_guard::{DescentGuard, StdDescentGuard, StdDescentGuardInit};
use super::driver::ParseDriver;
use crate::source::{Source, SourceSpan};
use crate::state::{Lang, ParsingState};

use super::{ParseResult, ParserSession};

/// A language ready to parse: the parsing state every parse starts from, and the
/// driver supplying the language's parse-time behavior.
///
/// A `Language` holds nothing belonging to a single parse, so one value defines a
/// language once and parses any number of documents. It is cheap to share (`Send +
/// Sync` whenever its two parts are), and a [`ParseResult`] borrows nothing from it:
/// results outlive the language that produced them.
///
/// # Building one
///
/// [`new`](Language::new) takes the two mandatory inputs: the
/// [`ParseDriver`](crate::engine::ParseDriver) instance — which carries the recovery
/// policy — and the initial [`ParsingState`]. That state normally comes from one of
/// two seed constructors: [`ParsingState::lang_initial`] for the language's own
/// canonical seed, and [`ParsingState::lang_initial_with_packages`] for that seed
/// plus the packages whose definitions the parse should see.
///
/// Anything further — different token rules, a different starting mode — is applied
/// to the seed *before* construction, through the one derivation path:
/// `Language::new(driver, ParsingState::lang_initial()?.derived(&delta)?)`. Deriving
/// is what lets the language check its own invariants
/// ([`Lang::finalize_transition`](crate::state::Lang::finalize_transition)) over
/// every customized seed.
///
/// # Running a parse
///
/// [`parse`](Language::parse) is the everyday call: pass a string, get a
/// [`ParseResult`]. [`parse_setup`](Language::parse_setup) is the configurable form,
/// taking a [`Source`] you built yourself and returning a [`ParseSetup`] on which
/// this one parse's initial state and root parser can be replaced before
/// [`ParseSetup::parse`] runs it.
///
/// [Running the parser](crate::guide::parsing) covers both, along with the
/// strict-versus-tolerant choice and what to do with the diagnostics.
///
/// ```
/// # use techy::core::{Language, ParsingState, StdParseDriver, TrivialLang};
/// # use techy::error::Recovery;
/// # #[derive(Debug, Clone, Copy)]
/// # struct MyLang;
/// # impl TrivialLang for MyLang {}
/// let language: Language<MyLang> = Language::new(
///     StdParseDriver::new(Recovery::Tolerant, ()),
///     ParsingState::lang_initial().expect("seed state"),
/// );
/// let result = language.parse("hello").unwrap();
/// assert_eq!(result.tree.root().chars(), None); // the root is a List
/// ```
///
/// To drive construct parsers yourself rather than run a whole parse, take the two
/// pieces from [`initial_state`](Language::initial_state) and
/// [`driver`](Language::driver) and pair them with a [`ParserSession`] of your own,
/// which is independent of any `Language`:
///
/// ```ignore
/// let mut session = ParserSession::new();
/// let mut reader = language.driver().make_token_reader(&source);
/// let mut cx = ParseContext::new(
///     &mut *reader,
///     Arc::clone(language.initial_state()),
///     &mut session,
///     language.driver(),
/// );
/// ```
pub struct Language<L: Lang> {
    /// The language's parse-behavior instance ([`Lang::Driver`]).
    driver: L::Driver,
    /// The frozen initial state every parse starts from — shared by `Arc` across
    /// parses (states are immutable).
    initial_state: Arc<ParsingState<L>>,
    /// The configuration for the per-parse [`StdDescentGuard`] (the parsing-depth
    /// limiter): defaulted in [`new`](Language::new), set with
    /// [`with_descent_guard_init`](Language::with_descent_guard_init), consumed by
    /// [`ParseSetup::parse`] to create each parse's guard.
    descent_guard_init: StdDescentGuardInit,
}

impl<L: Lang> Language<L> {
    /// Creates a language that parses with `driver`, starting every parse from
    /// `initial_state`.
    ///
    /// Both inputs are mandatory; the type-level documentation shows the canonical
    /// construction, and [`ParsingState::lang_initial`] and
    /// [`ParsingState::lang_initial_with_packages`] keep the everyday spellings
    /// short.
    ///
    /// `initial_state` accepts a state by value or an already-shared
    /// `Arc<ParsingState<L>>`. Passing the shared handle preserves the state's
    /// identity — states are shared by handle, and a data-equal copy is a different
    /// state — so a language can start its parses from exactly the state some
    /// already-parsed node recorded.
    ///
    /// Nesting depth is capped by the default descent-guard configuration; choose
    /// the cap explicitly with
    /// [`with_descent_guard_init`](Language::with_descent_guard_init).
    pub fn new(
        driver: L::Driver,
        initial_state: impl Into<Arc<ParsingState<L>>>,
    ) -> Language<L> {
        Language {
            driver,
            initial_state: initial_state.into(),
            descent_guard_init: Default::default(),
        }
    }

    /// Sets how deeply this language's parses may nest, by configuring the
    /// per-parse [`StdDescentGuard`].
    ///
    /// The guard caps nesting so that pathological input is refused with an ordinary
    /// error instead of crashing the process by exhausting the call stack. See
    /// [`StdDescentGuardInit`] for the choice between a stack budget, a plain depth
    /// limit, and no cap at all.
    ///
    /// Without this call, parses run under a deliberately tight built-in stack
    /// budget that also records a one-time warning diagnostic once half of it is
    /// used. An untuned deep parse therefore fails early, with a message naming this
    /// method, rather than consuming an unknown amount of stack.
    pub fn with_descent_guard_init(mut self, init: StdDescentGuardInit) -> Language<L> {
        self.descent_guard_init = init;
        self
    }

    /// The frozen initial state every parse starts from.
    pub fn initial_state(&self) -> &Arc<ParsingState<L>> {
        &self.initial_state
    }

    /// The language's [`ParseDriver`](crate::engine::ParseDriver) instance —
    /// concretely typed, so preset helper methods (and driver-configured capabilities
    /// like [`source_resolver`](crate::engine::ParseDriver::source_resolver)) are
    /// directly reachable.
    pub fn driver(&self) -> &L::Driver {
        &self.driver
    }

    /// Parses `content` and returns the resulting node tree together with the
    /// diagnostics recorded along the way.
    ///
    /// This is the everyday entry point of the crate. Everything the parse needs is
    /// already on the `Language`: the [`initial_state`](Language::initial_state)
    /// fixes the token rules, the starting mode, and the definitions in scope, and
    /// the [`driver`](Language::driver) supplies the parse-time behavior, including
    /// the [`Recovery`](crate::error::Recovery) policy. So there is nothing to pass
    /// here but the text.
    ///
    /// `content` is wrapped in a fresh anonymous [`Source`], which makes this call
    /// exactly `parse_setup(Source::new(content)).parse()`. Reach for
    /// [`parse_setup`](Language::parse_setup) when the source needs an origin label
    /// such as a file name, or when the same source handle must be shared across
    /// parses.
    ///
    /// # What you get back
    ///
    /// [`ParseResult::tree`] is the parsed document: a
    /// [`NodeTree`](crate::core::node::NodeTree) whose root is the list of top-level
    /// nodes. [`ParseResult::diagnostics`] holds
    /// every problem that was recorded rather than raised — empty for a clean parse,
    /// so check
    /// [`has_errors`](crate::error::Diagnostics::has_errors) before treating a
    /// tolerant result as clean. The result owns its tree and borrows nothing from
    /// the language.
    ///
    /// Each call mints a **fresh source identity** ([`Source::new`]), and
    /// [`SourceSpan`]/[`SourcePos`](crate::source::SourcePos) equality compares the
    /// source by identity as well as by offsets. Spans from two `parse` calls
    /// therefore never compare equal, even over byte-identical text (the comparison
    /// answers `false`; it does not fail). To correlate positions across parses —
    /// re-parsing an edited document, diffing two attempts — hold one `Arc<Source>`
    /// and pass the same handle to [`parse_setup`](Language::parse_setup) each time.
    ///
    /// # Errors
    ///
    /// Returns the [`ParseError`] that ended the parse. Which problems end a parse
    /// is the driver's recovery policy: under
    /// [`Recovery::Strict`](crate::error::Recovery::Strict) the first problem in the
    /// document aborts, while under
    /// [`Recovery::Tolerant`](crate::error::Recovery::Tolerant) document problems are
    /// recorded as diagnostics and parsing continues — see
    /// [strict versus tolerant](crate::guide::parsing#strict-versus-tolerant). A
    /// violation of a library contract by an extension (a hook or a custom parser
    /// breaking its documented obligations) aborts under either policy. An aborted
    /// parse returns no tree and no diagnostics: the error itself describes the
    /// problem and the constructs that were open at the time.
    ///
    /// # Examples
    ///
    /// ```
    /// use techy::core::{Language, ParsingState};
    /// use techy::error::Recovery;
    /// use techy::latexlike::{Latexlike, LatexlikeDriver};
    ///
    /// // Define the language once; parse as many documents as you like with it.
    /// let language: Language<Latexlike> = Language::new(
    ///     LatexlikeDriver::new(Recovery::Tolerant),
    ///     ParsingState::lang_initial().expect("seed state"),
    /// );
    ///
    /// let result = language.parse("Hello {world}!").unwrap();
    /// assert!(result.diagnostics.is_empty());
    ///
    /// // The root lists the top-level content: text, the group, more text.
    /// let root = result.tree.root();
    /// assert_eq!(root.child_count(), 3);
    /// assert_eq!(root.child(1).unwrap().span_content(), "{world}");
    ///
    /// // Tolerant parsing records a problem instead of aborting: nothing defines
    /// // `\nope`, so it is diagnosed and recovered as literal characters.
    /// let recovered = language.parse(r"a \nope b").unwrap();
    /// assert!(recovered.diagnostics.has_errors());
    /// ```
    pub fn parse(
        &self,
        content: impl Into<String>,
    ) -> Result<ParseResult<L>, ParseError<L::SourceOrigin>>
    where
        L::InvocationSyntax: FromInvocation<L>,
    {
        self.parse_setup(Source::new(content)).parse()
    }

    /// Sets up a parse of `source` — the configurable counterpart to
    /// [`parse`](Language::parse).
    ///
    /// `source` is a [`Source`] you built yourself, by value or as an already-shared
    /// `Arc<Source>`: one carrying an origin label (a file name, so diagnostics can
    /// name it), one carrying provenance (the result of
    /// [`resolve_source_reference`](crate::source::resolve_source_reference)), or
    /// simply one handle held across several parses so that their positions compare
    /// equal.
    ///
    /// The returned [`ParseSetup`] starts from this language's defaults — the
    /// [`initial_state`](Language::initial_state) and the driver's root parser
    /// ([`ParseDriver::make_root_parser`](crate::engine::ParseDriver::make_root_parser))
    /// — which its `with_*` methods replace for this one parse.
    /// [`ParseSetup::parse`] then runs it and returns the same
    /// [`ParseResult`]-or-[`ParseError`] answer as [`parse`](Language::parse):
    ///
    /// ```
    /// # use std::sync::Arc;
    /// # use techy::core::{Language, ParsingState, StdParseDriver, TrivialLang};
    /// # use techy::error::Recovery;
    /// # use techy::source::Source;
    /// # #[derive(Debug, Clone, Copy)]
    /// # struct MyLang;
    /// # impl TrivialLang for MyLang {}
    /// # let language: Language<MyLang> = Language::new(
    /// #     StdParseDriver::new(Recovery::Tolerant, ()),
    /// #     ParsingState::lang_initial().expect("seed state"),
    /// # );
    /// let source = Arc::new(Source::new("hello"));
    /// let result = language.parse_setup(Arc::clone(&source)).parse().unwrap();
    /// assert!(Arc::ptr_eq(result.tree.root().span().source(), &source));
    /// ```
    pub fn parse_setup<'p>(
        &self,
        source: impl Into<Arc<Source<L::SourceOrigin>>>,
    ) -> ParseSetup<'_, 'p, L> {
        ParseSetup {
            language: self,
            source: source.into(),
            initial_state: Arc::clone(&self.initial_state),
            root_parser: None,
        }
    }
}

// Manual Debug: a derive would demand `L: Debug` although only associated types
// (already bounded) are stored. There is deliberately no `Default` impl: it would
// reintroduce an implicit seed by the back door, and the driver's recovery policy
// must be an explicit choice.
impl<L: Lang> fmt::Debug for Language<L> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Language")
            .field("driver", &self.driver)
            .field("initial_state", &self.initial_state)
            // The descent-guard configuration carries no `Debug` bound: shown by
            // omission.
            .finish_non_exhaustive()
    }
}

/// One parse, configured but not yet run: its source, plus whatever this parse does
/// differently from its [`Language`]'s defaults.
///
/// Created by [`Language::parse_setup`], adjusted with the `with_*` methods, and run
/// by [`parse`](ParseSetup::parse), which consumes the setup — one setup, one parse.
///
/// Only what varies per parse belongs here. What holds across parses — the driver,
/// the language's initial state, the nesting-depth configuration — is set on the
/// [`Language`].
///
/// ```
/// # use std::sync::Arc;
/// # use techy::core::{Language, ParsingState, StdParseDriver, TrivialLang};
/// # use techy::error::Recovery;
/// # use techy::source::Source;
/// # #[derive(Debug, Clone, Copy)]
/// # struct MyLang;
/// # impl TrivialLang for MyLang {}
/// # let language: Language<MyLang> = Language::new(
/// #     StdParseDriver::new(Recovery::Tolerant, ()),
/// #     ParsingState::lang_initial().expect("seed state"),
/// # );
/// // Re-parse a fragment under exactly the state some parsed node recorded.
/// let first = language.parse("hello").unwrap();
/// let node_state = Arc::clone(first.tree.root().parsing_state());
/// let again = language
///     .parse_setup(Source::new("world"))
///     .with_initial_state(Arc::clone(&node_state))
///     .parse()
///     .unwrap();
/// assert!(Arc::ptr_eq(again.tree.root().parsing_state(), &node_state));
/// ```
///
/// With no `with_*` call at all, `parse_setup(source).parse()` is exactly
/// [`Language::parse`] over a source you built yourself.
#[must_use = "a `ParseSetup` does nothing until `parse()` runs it"]
pub struct ParseSetup<'l, 'p, L: Lang> {
    language: &'l Language<L>,
    source: Arc<Source<L::SourceOrigin>>,
    /// The state the parse starts from — the language's initial state unless
    /// [`with_initial_state`](ParseSetup::with_initial_state) replaced it.
    initial_state: Arc<ParsingState<L>>,
    /// The root parser for this parse; `None` = the driver's
    /// [`make_root_parser`](ParseDriver::make_root_parser), consulted at parse time.
    root_parser: Option<&'p mut dyn ConstructParser<L, Output = BuildId>>,
}

impl<'l, 'p, L: Lang> ParseSetup<'l, 'p, L> {
    /// Starts this parse from `state` instead of the language's
    /// [`initial_state`](Language::initial_state).
    ///
    /// Any state handle serves: the language's seed with a delta applied
    /// (`language.initial_state().derived(&delta)?`, the one derivation path), or
    /// the state some already-parsed node recorded
    /// ([`parsing_state`](crate::core::node::NodeRef::parsing_state)), which parses a
    /// fragment under exactly the conditions that node was parsed under.
    ///
    /// A shared `Arc` is used by identity: the root node records this very state,
    /// and the driver's
    /// [`observe_parse_start`](ParseDriver::observe_parse_start) sees it as the
    /// parse's initial state.
    pub fn with_initial_state(mut self, state: impl Into<Arc<ParsingState<L>>>) -> Self {
        self.initial_state = state.into();
        self
    }

    /// Runs `parser` at the root of this parse, instead of the parser the driver's
    /// [`make_root_parser`](ParseDriver::make_root_parser) supplies.
    ///
    /// This is the one-parse way to a different root shape: wrapping an auxiliary
    /// source's content in scaffolding of your own, or staging a different root
    /// node. A language whose parses always need that overrides the factory instead.
    ///
    /// The parser is borrowed for the duration of the parse and is yours again
    /// afterwards, so a root parser may collect data for you to read back once the
    /// parse returns. Its contract — it runs at the top rather than as a descent,
    /// and its output becomes the tree's root — is documented on
    /// [`RootNodesParser`](crate::constructs::RootNodesParser).
    pub fn with_root_parser<'q>(
        self,
        parser: &'q mut dyn ConstructParser<L, Output = BuildId>,
    ) -> ParseSetup<'l, 'q, L> {
        ParseSetup {
            language: self.language,
            source: self.source,
            initial_state: self.initial_state,
            root_parser: Some(parser),
        }
    }

    /// Runs the parse and returns the resulting tree together with the diagnostics
    /// recorded along the way.
    ///
    /// The run proceeds in four steps:
    ///
    /// 1. The driver builds a token reader over the source
    ///    ([`make_token_reader`](ParseDriver::make_token_reader)).
    /// 2. A [`ParserSession`] is created, with its diagnostics cap taken from
    ///    [`diagnostics_limit`](ParseDriver::diagnostics_limit) and its descent
    ///    guard from the language's nesting-depth configuration.
    /// 3. The driver's once-per-parse
    ///    [`observe_parse_start`](ParseDriver::observe_parse_start) hook is called
    ///    with the state this parse starts from, before any token is read.
    /// 4. The root parser runs over a [`ParseContext`] at that state — directly at
    ///    the top, not as a descent (see
    ///    [`RootNodesParser`](crate::constructs::RootNodesParser)) — and the session
    ///    is frozen around the root node it returns into a [`ParseResult`].
    ///
    /// Under the standard root parser, a group close with no matching open at the
    /// root is diagnosed as
    /// [`StrayGroupClose`](crate::constructs::StrayGroupClose): a tolerant parse
    /// consumes it, stages it as a `Chars` node, and continues, while a strict parse
    /// aborts. [`RootNodesParser`](crate::constructs::RootNodesParser) documents the
    /// rest of that behavior.
    ///
    /// # Errors
    ///
    /// Returns the [`ParseError`] that ended the parse: the first document problem
    /// under [`Recovery::Strict`](crate::error::Recovery::Strict), or, under either
    /// policy, a violation of a library contract by an extension — a root parser
    /// factory that fails to build its parser included. Under
    /// [`Recovery::Tolerant`](crate::error::Recovery::Tolerant) document problems are
    /// recorded as diagnostics instead, and the call succeeds.
    pub fn parse(self) -> Result<ParseResult<L>, ParseError<L::SourceOrigin>>
    where
        L::InvocationSyntax: FromInvocation<L>,
    {
        let ParseSetup { language, source, initial_state, root_parser } = self;
        let driver = &language.driver;
        let mut reader = driver.make_token_reader(&source);
        let mut session = ParserSession::new();
        // Seed the diagnostics sink's retention cap from the driver
        // (`ParseDriver::diagnostics_limit`); `None` keeps the default cap.
        if let Some(limit) = driver.diagnostics_limit() {
            session.diagnostics = Diagnostics::with_limit(limit);
        }
        // The descent guard is created eagerly, here at true parse entry on the
        // parsing thread — a stack-measuring guard anchors its reference
        // measurement before any descent runs.
        session.install_descent_guard(StdDescentGuard::init(&language.descent_guard_init));
        // Parse-initialization observation (registration-sanity diagnostics): once
        // per root parse, before any token is read, over the state this parse
        // actually starts from.
        driver.observe_parse_start(&source, &initial_state, &mut session.diagnostics);
        let mut cx = ParseContext::new(&mut *reader, initial_state, &mut session, driver);
        // The root parser runs at the top — a direct call, not `parse_construct`:
        // no descent-guard level, no frame, no enclosing-state entry cover the root.
        // Its pass-through delta has no target (nothing encloses the root).
        let (root, _delta) = match root_parser {
            Some(parser) => parser.parse(&mut cx)?,
            None => {
                // A factory Err aborts under any policy ("could not build the
                // parser" — the hook fallibility contract).
                let mut parser = driver.make_root_parser()?;
                parser.parse(&mut cx)?
            }
        };
        session.finish(root).map_err(|error| {
            ParseError::new(
                ImplementationError::new(error.to_string()),
                SourceSpan::entire(&source),
            )
        })
    }
}

// Manual Debug: the root parser is a `dyn` borrow without a `Debug` bound — shown
// by presence.
impl<L: Lang> fmt::Debug for ParseSetup<'_, '_, L> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ParseSetup")
            .field("language", &self.language)
            .field("source", &self.source)
            .field("initial_state", &self.initial_state)
            .field("custom_root_parser", &self.root_parser.is_some())
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::constructs::{
        ChildStateSpec, ConstructParser, ConstructParserResult, NodesOutcome, StopCause,
        StopSpec, StrayGroupClose,
    };
    use crate::engine::{ParseDriver, StdParseDriver};
    use crate::error::{DiagnosticInfo, Recovery};
    use crate::node::NodeKind;
    use crate::node::check_tree_invariants;
    use crate::scopes::{Package, ScopeOp, ScopeStack};
    use crate::source::{MapResolver, SourceProvenance};
    use crate::state::{GroupOverrides, ParsingStateDelta, StateData, TokenRulesOverrides};
    use crate::token::{
        CommandRules, CommentRule, CommentRules, ForbiddenCharsRules, GroupRule,
        GroupRules, ParagraphRules, SpecialsRules, TokenRules, WhitespaceRules,
    };
    use alloc::string::String;
    use alloc::vec;
    use alloc::vec::Vec;

    /// The latex-ish seed rules the `DocLang`-family test languages share (groups
    /// `{`/`}` and `%` comments enabled) — generic so a sibling lang differing only
    /// in its driver does not re-derive the block.
    fn doc_state_data<
        L: Lang<
            GroupTypeId = u32,
            ModeId = (),
            StateExt = (),
            Features = crate::state::AllLangFeatures,
        >,
    >() -> StateData<L> {
        StateData {
            rules: TokenRules {
                whitespace: WhitespaceRules { enabled: true, chars: " \t\n".into() },
                paragraphs: ParagraphRules { enabled: true },
                groups: GroupRules {
                    enabled: true,
                    rules: vec![Arc::new(GroupRule {
                        group_type: 0,
                        open: "{".into(),
                        close: "}".into(),
                    })],
                    temporary: Vec::new(),
                    expecting_close: None,
                },
                commands: CommandRules {
                    enabled: false,
                    rules: Vec::new(),
                },
                comments: CommentRules {
                    enabled: true,
                    rules: vec![Arc::new(CommentRule { start: "%".into() })],
                },
                specials: SpecialsRules { enabled: false },
                forbidden_chars: ForbiddenCharsRules { chars: "".into() },
            },
            scopes: ScopeStack::new(),
            mode: (),
            ext: (),
        }
    }

    /// A language whose canonical seed enables the latex-ish syntax the tests use —
    /// exercising the "Language seeds from the Lang hook" path.
    #[derive(Debug, Clone, Copy)]
    struct DocLang;
    impl Lang for DocLang {
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
        type Driver = StdParseDriver;

        fn initial_state_data() -> Result<StateData<Self>, crate::state::FinalizeError> {
            Ok(doc_state_data())
        }
        fn make_node_ext(
            _kind: &crate::node::NodeKind<Self>,
            _span: &crate::source::SourceSpan<Self::SourceOrigin>,
            _state: &alloc::sync::Arc<crate::state::ParsingState<Self>>,
            _children: crate::node::StagedChildren<'_, Self>,
        ) -> Result<(), crate::node::NodeBuildError> {
            Ok(())
        }
    }

    fn strict() -> Language<DocLang> {
        Language::new(
            StdParseDriver::new(Recovery::Strict, ()),
            ParsingState::lang_initial().expect("seed state"),
        )
    }

    fn tolerant() -> Language<DocLang> {
        Language::new(
            StdParseDriver::new(Recovery::Tolerant, ()),
            ParsingState::lang_initial().expect("seed state"),
        )
    }

    /// The staged child shapes of a result's root list, as compact strings.
    fn shapes(result: &ParseResult<DocLang>) -> Vec<String> {
        result
            .tree
            .root()
            .children()
            .iter().map(|child| match child.chars() {
                Some(text) => alloc::format!("chars({})", text),
                None if child.group().is_some() => "group".into(),
                None => "other".into(),
            })
            .collect()
    }

    #[test]
    fn new_accepts_a_shared_state_handle_preserving_identity() {
        // A parsed node's state is a shared handle; seeding a new language from that
        // handle preserves the state's identity (no data-equal copy is minted).
        let result = strict().parse("hello").unwrap();
        let node_state = Arc::clone(result.tree.root().parsing_state());

        let language =
            Language::new(StdParseDriver::new(Recovery::Strict, ()), Arc::clone(&node_state));
        assert!(Arc::ptr_eq(language.initial_state(), &node_state));
    }

    #[test]
    fn parse_drives_reader_to_tree_end_to_end() {
        let result = strict().parse("hello {world}").unwrap();
        check_tree_invariants(&result.tree);
        assert!(result.diagnostics.is_empty());
        assert_eq!(shapes(&result), ["chars(hello )", "group"]);
        // The root List spans the entire source.
        assert_eq!(result.tree.root().span().range(), 0..13);
    }

    #[test]
    fn parse_of_empty_content_is_an_empty_root_list() {
        let result = strict().parse("").unwrap();
        check_tree_invariants(&result.tree);
        assert_eq!(result.tree.root().children().iter().count(), 0);
        assert_eq!(result.tree.root().span().range(), 0..0);
        assert!(result.diagnostics.is_empty());
    }

    #[test]
    fn define_once_parse_many_shares_the_seed_state() {
        let language = strict();
        let first = language.parse("a").unwrap();
        let second = language.parse("b{c}").unwrap();
        // Every parse starts from the same frozen seed Arc — the Language contributes
        // at exactly one moment and accumulates nothing.
        for result in [&first, &second] {
            assert!(Arc::ptr_eq(
                result.tree.root().parsing_state(),
                language.initial_state()
            ));
        }
        assert_eq!(shapes(&first), ["chars(a)"]);
        assert_eq!(shapes(&second), ["chars(b)", "group"]);
    }

    #[test]
    fn stray_close_aborts_strict_and_recovers_as_chars_tolerantly() {
        // Strict: the root drive aborts with the core condition.
        let err = strict().parse("a}b").unwrap_err();
        assert_eq!(err.identifier(), StrayGroupClose::IDENTIFIER);
        assert_eq!(err.span().range(), 1..2);
        assert_eq!(err.to_string(), "unexpected closing ‘}’ — no group is open");

        // Tolerant: diagnose, consume, and stage the delimiter as a chars node (the
        // markup-in-chars recovery artifact — revised in 7.9, superseding 7.4's
        // byte-dropping quirk: the root's span tiling holds across the skip).
        let result = tolerant().parse("a}b").unwrap();
        check_tree_invariants(&result.tree);
        assert_eq!(shapes(&result), ["chars(a)", "chars(})", "chars(b)"]);
        assert_eq!(result.diagnostics.len(), 1);
        let diagnostic = result.diagnostics.iter().next().unwrap();
        assert_eq!(diagnostic.identifier(), StrayGroupClose::IDENTIFIER);
        assert_eq!(diagnostic.message(), "unexpected closing ‘}’ — no group is open");
    }

    #[test]
    fn consecutive_stray_closes_each_report_and_resume() {
        let result = tolerant().parse("}}x").unwrap();
        check_tree_invariants(&result.tree);
        assert_eq!(shapes(&result), ["chars(})", "chars(})", "chars(x)"]);
        assert_eq!(result.diagnostics.len(), 2);
    }

    // --- the driver's diagnostics retention cap -----------------------------------------

    /// `DocLang` under a driver that caps diagnostics retention — the
    /// `ParseDriver::diagnostics_limit` seeding path.
    #[derive(Debug, Clone, Copy)]
    struct CapLang;
    impl Lang for CapLang {
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
        type Driver = CapDriver;

        fn initial_state_data() -> Result<StateData<Self>, crate::state::FinalizeError> {
            Ok(doc_state_data())
        }
        fn make_node_ext(
            _kind: &crate::node::NodeKind<Self>,
            _span: &crate::source::SourceSpan<Self::SourceOrigin>,
            _state: &alloc::sync::Arc<crate::state::ParsingState<Self>>,
            _children: crate::node::StagedChildren<'_, Self>,
        ) -> Result<(), crate::node::NodeBuildError> {
            Ok(())
        }
    }

    /// Tolerant driver retaining at most two diagnostics per parse.
    #[derive(Debug, Clone, Copy)]
    struct CapDriver;

    impl ParseDriver<CapLang> for CapDriver {
        fn recovery(&self) -> Recovery {
            Recovery::Tolerant
        }
        fn diagnostics_limit(&self) -> Option<usize> {
            Some(2)
        }
    }

    // --- the session extension returned on the result ------------------------------------

    /// Transition counts accumulated by `observe_transition` — read back off the
    /// [`ParseResult`].
    #[derive(Debug, Default)]
    struct Observed {
        transitions: usize,
    }

    /// `DocLang` under a driver whose `observe_transition` accumulates into the
    /// session extension — the `ParseResult::session_ext` read-back path.
    #[derive(Debug, Clone, Copy)]
    struct ObserveLang;
    impl Lang for ObserveLang {
        type Features = crate::state::AllLangFeatures;
        type GroupTypeId = u32;
        type CallableTypeId = u32;
        type ModeId = ();
        type StateExt = ();
        type Event = ();
        type SessionExt = Observed;
        type SourceOrigin = Option<String>;
        type Tokenization = crate::token::StdTokenization;
        type NodeExts = ();
        type InvocationSyntax = ();
        type Driver = ObserveDriver;

        fn initial_state_data() -> Result<StateData<Self>, crate::state::FinalizeError> {
            Ok(doc_state_data())
        }
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
    struct ObserveDriver;

    impl ParseDriver<ObserveLang> for ObserveDriver {
        fn observe_transition(
            &self,
            ext: &mut Observed,
            _diagnostics: &mut crate::error::Diagnostics,
            _prev: &ParsingState<ObserveLang>,
            _new: &ParsingState<ObserveLang>,
            _delta: &ParsingStateDelta<ObserveLang>,
        ) -> Result<(), ParseError> {
            ext.transitions += 1;
            Ok(())
        }
    }

    #[test]
    fn the_session_extension_is_returned_on_the_parse_result() {
        // Two group descents = two observed transitions (sibling groups memoize the
        // derivation, but observation fires per transition, memo hits included):
        // `observe_transition` accumulates them into the session extension, and the
        // finished result hands the value out.
        let language: Language<ObserveLang> =
            Language::new(ObserveDriver, ParsingState::lang_initial().expect("seed state"));
        let result = language.parse("{a}{b}").unwrap();
        assert_eq!(result.session_ext.transitions, 2);

        // A language declaring no session extension reads back `()`.
        let result = strict().parse("{a}").unwrap();
        let () = result.session_ext;
    }

    #[test]
    fn a_driver_diagnostics_limit_caps_the_parses_diagnostics() {
        // Four stray closes under the tolerant policy: four pushes, two retained,
        // two counted as suppressed (`Diagnostics::with_limit`'s cap semantics).
        let language: Language<CapLang> =
            Language::new(CapDriver, ParsingState::lang_initial().expect("seed state"));
        let result = language.parse("}}}}").unwrap();
        assert_eq!(result.diagnostics.limit(), 2);
        assert_eq!(result.diagnostics.len(), 2);
        assert_eq!(result.diagnostics.suppressed(), 2);
        assert!(result.diagnostics.has_errors());

        // The `None` default keeps the standard cap.
        let result = tolerant().parse("}}").unwrap();
        assert_eq!(
            result.diagnostics.limit(),
            crate::error::Diagnostics::<Option<String>>::DEFAULT_LIMIT
        );
        assert_eq!(result.diagnostics.suppressed(), 0);
    }

    #[test]
    fn a_derived_seed_customizes_through_the_choke_point() {
        // Disabling comments through the delta idiom — the seed derives *before*
        // construction: `Language::new(driver, lang_initial()?.derived(&delta)?)`.
        let seed = ParsingState::<DocLang>::lang_initial().expect("seed state")
            .derived(&ParsingStateDelta::new().rules(TokenRulesOverrides {
                comments: crate::state::CommentOverrides::disable(),
                ..TokenRulesOverrides::default()
            }))
            .unwrap();
        let language = Language::new(StdParseDriver::new(Recovery::Strict, ()), seed);
        assert!(!language.initial_state().rules().comments_enabled());
        let result = language.parse("a%b").unwrap();
        check_tree_invariants(&result.tree);
        assert_eq!(shapes(&result), ["chars(a%b)"]);
    }

    #[test]
    fn the_delta_idiom_surfaces_scope_op_failures_before_construction() {
        // A failing scope op aborts seed derivation — no `Language` is ever built.
        let error = ParsingState::<DocLang>::lang_initial().expect("seed state")
            .derived(
                &ParsingStateDelta::new().scope_op(ScopeOp::Unload { name: "absent".into() }),
            )
            .unwrap_err();
        assert_eq!(error.failures.len(), 1);
    }

    #[test]
    fn resolver_round_trip_parses_a_resolved_source() {
        // The source resolver lives on the driver ([§dd-dr:input-wiring]): configure
        // it there, reach it through the `ParseDriver::source_resolver` accessor, and
        // compose with the free `resolve_source_reference`.
        use crate::source::resolve_source_reference;

        let mut resolver = MapResolver::new();
        resolver.insert("chapter.tex", "chapter {content}");
        let language: Language<DocLang> = Language::new(
            StdParseDriver::new(Recovery::Strict, ()).with_source_resolver(resolver),
            ParsingState::lang_initial().expect("seed state"),
        );

        let main = language.parse(r"\input{chapter.tex}").unwrap();
        let trigger = main.tree.root().span().clone();
        let driver_resolver =
            ParseDriver::<DocLang>::source_resolver(language.driver()).unwrap();
        let resolved =
            resolve_source_reference(driver_resolver, "chapter.tex", &trigger).unwrap();
        match resolved.provenance() {
            SourceProvenance::Resolved { reference, triggered_at } => {
                assert_eq!(reference, "chapter.tex");
                assert_eq!(triggered_at, &trigger);
            }
            other => panic!("expected Resolved provenance, got {:?}", other),
        }

        let result = language.parse_setup(Arc::clone(&resolved)).parse().unwrap();
        check_tree_invariants(&result.tree);
        assert_eq!(shapes(&result), ["chars(chapter )", "group"]);
        // The tree's spans reference the resolved source (provenance intact).
        assert!(Arc::ptr_eq(result.tree.root().span().source(), &resolved));
    }

    #[test]
    fn an_unconfigured_driver_resolves_no_sources() {
        let language = strict();
        assert!(ParseDriver::<DocLang>::source_resolver(language.driver()).is_none());
    }

    /// A driver whose nodes-parser factory violates the output contract by stopping on
    /// a token condition none was set — the root drive aborts with an implementation
    /// error under *any* recovery policy.
    #[test]
    fn a_contract_violating_root_stop_is_an_implementation_error() {
        #[derive(Debug, Clone, Copy)]
        struct BogusLang;
        impl Lang for BogusLang {
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
            type Driver = BogusDriver;
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
        struct BogusDriver;

        struct BogusParser;
        impl ConstructParser<BogusLang> for BogusParser {
            type Output = NodesOutcome<BogusLang>;
            fn parse(
                &mut self,
                cx: &mut ParseContext<'_, '_, BogusLang>,
            ) -> ConstructParserResult<
                BogusLang,
                (Self::Output, Option<alloc::boxed::Box<ParsingStateDelta<BogusLang>>>),
            > {
                Ok((
                    NodesOutcome {
                        nodes: Vec::new(),
                        stop: StopCause::TokenCondition {
                            span: cx.here(),
                            after: cx.tokens.position_here(),
                        },
                        state: Arc::clone(&cx.state),
                        after_effects: None,
                    },
                    None,
                ))
            }
        }

        impl ParseDriver<BogusLang> for BogusDriver {
            fn recovery(&self) -> Recovery {
                Recovery::Tolerant
            }
            fn make_nodes_parser<'p>(
                &'p self,
                _stop: StopSpec<'p, BogusLang>,
                _child_states: ChildStateSpec<'p, BogusLang>,
            ) -> Result<
                alloc::boxed::Box<
                    dyn ConstructParser<BogusLang, Output = NodesOutcome<BogusLang>> + 'p,
                >,
                ParseError,
            > {
                Ok(alloc::boxed::Box::new(BogusParser))
            }
        }

        let language: Language<BogusLang> =
            Language::new(BogusDriver, ParsingState::lang_initial().expect("seed state"));
        let err = language.parse("x").unwrap_err();
        assert_eq!(err.identifier(), ImplementationError::IDENTIFIER);
    }

    // --- the descent guard end to end (Part 2) ------------------------------------------

    use crate::constructs::DescentLimitExceeded;
    use crate::engine::{StdDescentGuard, StdDescentGuardInit};

    /// Balanced `{…{…}…}` nesting, `levels` deep, with one char inside.
    fn nested_braces(levels: usize) -> String {
        let mut input = String::new();
        for _ in 0..levels {
            input.push('{');
        }
        input.push('a');
        for _ in 0..levels {
            input.push('}');
        }
        input
    }

    #[test]
    fn deep_nesting_under_the_unconfigured_default_is_an_error_not_a_crash() {
        // 50 000 nesting levels would exhaust the call stack without the guard; the
        // built-in default refuses long before that, under either recovery policy
        // (a refusal aborts under any policy), and the process survives. The
        // refusal is self-describing: it names the built-in default and the
        // configuration entry point.
        let deep = nested_braces(50_000);
        for language in [strict(), tolerant()] {
            let err = language.parse(deep.clone()).unwrap_err();
            assert_eq!(err.identifier(), DescentLimitExceeded::IDENTIFIER);
            let message = err.to_string();
            assert!(message.contains("StdDescentGuard::DEFAULT_STACK_BUDGET"), "{message}");
            assert!(message.contains("with_descent_guard_init"), "{message}");
        }
    }

    #[test]
    fn a_tiny_fixed_budget_refuses_deep_nesting_in_any_build_profile() {
        // 200 levels always outrun a 4 KiB consumption cap, optimized or not — the
        // deterministic refusal contrast for the `Off` test below. The configured
        // refusal names the configured cap, not the built-in default.
        let language = strict()
            .with_descent_guard_init(StdDescentGuardInit::fixed_stack_budget(4 * 1024));
        let err = language.parse(nested_braces(200)).unwrap_err();
        assert_eq!(err.identifier(), DescentLimitExceeded::IDENTIFIER);
        let message = err.to_string();
        assert!(message.contains("configured budget"), "{message}");
        assert!(!message.contains("with_descent_guard_init"), "{message}");
    }

    #[test]
    fn off_disables_the_limit() {
        // The same 30-level input that a 4 KiB cap refuses parses cleanly under
        // `Off` (30 levels stay comfortably within the test thread's real stack).
        let input = nested_braces(30);
        let capped = strict()
            .with_descent_guard_init(StdDescentGuardInit::fixed_stack_budget(4 * 1024));
        assert!(capped.parse(input.clone()).is_err());
        let off = strict().with_descent_guard_init(StdDescentGuardInit::off());
        let result = off.parse(input).unwrap();
        assert!(result.diagnostics.is_empty());
        check_tree_invariants(&result.tree);
    }

    #[test]
    fn depth_limit_counts_nesting_not_siblings() {
        // Depth mode is deterministic across build profiles: nesting past the
        // limit is refused; sibling constructs at the same depth re-use the level
        // (each descent's exit rebalances the count).
        let language =
            strict().with_descent_guard_init(StdDescentGuardInit::depth_limit(10));
        assert!(language.parse(nested_braces(3)).is_ok());
        let err = language.parse(nested_braces(20)).unwrap_err();
        assert_eq!(err.identifier(), DescentLimitExceeded::IDENTIFIER);
        assert!(err.to_string().contains("depth limit"), "{}", err);
        // Twelve sibling groups — more constructs than the limit, none deeper
        // than the limit — parse cleanly.
        let siblings = "{a}".repeat(12);
        assert!(language.parse(siblings).is_ok());
    }

    #[test]
    fn a_depth_limit_refusal_carries_the_live_traceback() {
        // The refusal error snapshots the live frame stack, including the frame
        // of the construct whose descent was refused (pushed before the guard is
        // asked). With a limit of 4 the refused descent is a group's interior
        // content run: that group's own frame is already on the stack.
        let language =
            strict().with_descent_guard_init(StdDescentGuardInit::depth_limit(4));
        let err = language.parse(nested_braces(20)).unwrap_err();
        assert_eq!(err.identifier(), DescentLimitExceeded::IDENTIFIER);
        let titles: Vec<&str> = err.frames().iter().map(|f| f.title()).collect();
        assert!(!titles.is_empty(), "the refusal carries the live traceback");
        assert!(
            titles.contains(&"group ‘{’"),
            "the traceback includes the refused construct's group frame: {titles:?}"
        );
    }

    #[test]
    fn a_computed_budget_resolves_probe_minus_headroom_at_parse_entry() {
        // The probe answers HEADROOM + 100 bytes: the resolved budget is 100
        // bytes — which the very first descent's own frames already outrun — and
        // the refusal names the headroom-subtracted number, observable end to end.
        fn probe() -> Option<usize> {
            Some(StdDescentGuard::HEADROOM + 100)
        }
        let language = strict()
            .with_descent_guard_init(StdDescentGuardInit::computed_stack_budget(probe));
        let err = language.parse("x").unwrap_err();
        assert_eq!(err.identifier(), DescentLimitExceeded::IDENTIFIER);
        assert!(err.to_string().contains("configured budget of 100 bytes"), "{}", err);
    }

    #[test]
    fn an_empty_driver_impl_is_complete() {
        // Every `ParseDriver` item is defaulted — tokenization included, since
        // `make_token_reader` falls back to the language's `Lang::Tokenization` (the
        // guard is engine-owned, not a driver choice either): `impl ParseDriver<L> for
        // D {}` is a whole driver, and the language parses.
        #[derive(Debug, Clone, Copy)]
        struct OneLineLang;
        impl Lang for OneLineLang {
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
            type Driver = OneLineDriver;
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
        struct OneLineDriver;
        impl ParseDriver<OneLineLang> for OneLineDriver {}
        let language: Language<OneLineLang> =
            Language::new(OneLineDriver, ParsingState::lang_initial().expect("seed state"));
        assert!(language.parse("hello").is_ok());
    }

    // --- state threading across tolerant stray-close skips (findings #1–#3) -----------------
    //
    // A language whose top-level commands carry a `\newcommand`-style after-effect delta:
    // processing one evolves the loop's live state, so it diverges from the frozen seed
    // *before* a later stray close. The after-effect travels the public path
    // (`latexlike::MacroSpec::with_after_effect`); the findings themselves are root-loop
    // regressions, independent of the language flavor driving them.

    use crate::latexlike::{
        default_token_rules, CallableType, GroupType, Latexlike, LatexlikeDriver, MacroSpec,
    };

    fn content_group(open: &str, close: &str) -> Arc<crate::token::GroupRule<Latexlike>> {
        Arc::new(crate::token::GroupRule {
            group_type: GroupType::Content,
            open: open.into(),
            close: close.into(),
        })
    }

    /// A groups override replacing the rule list with the latexlike defaults plus
    /// `extra` (override lists replace wholesale, so an addition restates the
    /// defaults).
    fn add_groups(
        extra: &[Arc<crate::token::GroupRule<Latexlike>>],
    ) -> ParsingStateDelta<Latexlike> {
        let mut rules = default_token_rules::<Latexlike>().groups.rules;
        rules.extend(extra.iter().cloned());
        ParsingStateDelta::new().rules(TokenRulesOverrides {
            groups: GroupOverrides { rules: Some(rules), ..GroupOverrides::default() },
            ..TokenRulesOverrides::default()
        })
    }

    /// A tolerant latexlike language whose seed defines `\{name}` as a zero-argument
    /// macro with after-effect `delta`.
    fn latexlike_defining(
        name: &str,
        delta: ParsingStateDelta<Latexlike>,
    ) -> Language<Latexlike> {
        let mut lib: Package<Latexlike> = Package::new("test");
        lib.insert(CallableType::Macro, name, MacroSpec::new(vec![]).with_after_effect(delta));
        Language::new(
            LatexlikeDriver::new(Recovery::Tolerant),
            ParsingState::lang_initial_with_packages([lib]).expect("seed state"),
        )
    }

    /// A zero-arg macro package defining `name` — a `\def`-style after-effect payload.
    fn zero_arg_macro(name: &str) -> Arc<Package<Latexlike>> {
        let mut lib: Package<Latexlike> = Package::new("defined");
        lib.insert(CallableType::Macro, name, MacroSpec::default());
        Arc::new(lib)
    }

    /// The callable names among a result's root children, in order (chars/other skipped).
    fn callable_names(result: &ParseResult<Latexlike>) -> Vec<String> {
        result
            .tree
            .root()
            .children()
            .iter().filter_map(|child| child.name().map(String::from))
            .collect()
    }

    #[test]
    fn a_stray_close_of_a_delimiter_a_sibling_delta_added_recovers_tolerantly() {
        // Finding #1. `\addangle`'s after-effect adds `<`/`>` as a group pair the seed
        // state lacks, so the `>` that follows is a stray close *only under the loop's
        // evolved state*. Tolerant parsing must diagnose and skip it. The bug: the old
        // root loop re-tokenized the close under the frozen `seed`, where `>` is an
        // ordinary character (not a `GroupClose`), so the recovery misfired into a
        // spurious `ImplementationError` that aborted the parse even under tolerant
        // recovery. The delimiter the loop actually saw must drive the diagnosis, never a
        // re-read under a different state.
        let language = latexlike_defining("addangle", add_groups(&[content_group("<", ">")]));
        let result = language.parse("\\addangle >x").unwrap();
        // Recovery continued past the stray close: `\addangle` staged and `x` reached.
        assert_eq!(callable_names(&result), ["addangle"]);
        assert!(
            result.tree.root().children().iter().any(|c| c.chars() == Some("x")),
            "content after the stray close should be parsed"
        );
        assert_eq!(result.diagnostics.len(), 1);
        let diagnostic = result.diagnostics.iter().next().unwrap();
        assert_eq!(diagnostic.identifier(), StrayGroupClose::IDENTIFIER);
        assert!(diagnostic.message().contains('>'));
    }

    #[test]
    fn definitions_before_a_tolerant_stray_close_stay_in_scope_after_it() {
        // Finding #2. `\def`'s after-effect defines `\late` for subsequent siblings; a
        // stray `}` sits between the definition and its use. Tolerant parsing must skip the
        // `}` and *continue with the definition in scope*, so `\late` resolves. The bug:
        // the old root loop resumed every descent from the frozen `seed`, so the `\def`
        // definition was silently dropped across the skip and `\late` came out unresolvable
        // (a second, spurious diagnostic plus a chars fallback).
        let language = latexlike_defining(
            "def",
            ParsingStateDelta::new().push_provider(zero_arg_macro("late")),
        );
        let result = language.parse("\\def } \\late").unwrap();
        // `\late` resolved against the definition established before the stray close…
        assert_eq!(callable_names(&result), ["def", "late"]);
        // …so the only diagnostic is the stray close itself — no unresolvable-command.
        assert_eq!(result.diagnostics.len(), 1);
        assert_eq!(
            result.diagnostics.iter().next().unwrap().identifier(),
            StrayGroupClose::IDENTIFIER
        );
    }

    #[test]
    fn a_stray_close_reports_the_delimiter_the_loop_saw_not_a_reparse() {
        // Finding #3. `\widen`'s after-effect adds `[[`/`]]`; the seed already closes on a
        // single `]`. A stray `]]` is one 2-char close under the loop's evolved state, and
        // the diagnosis must report *that* delimiter. The bug: the old root loop
        // re-tokenized the close under the frozen `seed`, which only knows the 1-char `]`,
        // so it reported the wrong (shorter) delimiter and consumed a single byte — leaving
        // the second `]` to surface as a *second* stray close. Carrying the delimiter on
        // the stop cause makes the loop report `]]` once and skip both bytes.
        // The seed must already close on a single `]` (the finding's premise), so the
        // language seeds from a state derived with the `[`/`]` pair added; `\widen`'s
        // after-effect keeps that pair and adds `[[`/`]]`.
        let bracket = content_group("[", "]");
        let mut lib: Package<Latexlike> = Package::new("test");
        lib.insert(
            CallableType::Macro,
            "widen",
            MacroSpec::new(vec![])
                .with_after_effect(add_groups(&[bracket.clone(), content_group("[[", "]]")])),
        );
        let seed = ParsingState::lang_initial_with_packages([lib]).expect("seed state")
            .derived(&add_groups(&[bracket]))
            .unwrap();
        let language = Language::new(LatexlikeDriver::new(Recovery::Tolerant), seed);
        let result = language.parse("\\widen ]]x").unwrap();
        assert_eq!(callable_names(&result), ["widen"]);
        assert_eq!(result.diagnostics.len(), 1);
        let diagnostic = result.diagnostics.iter().next().unwrap();
        assert_eq!(diagnostic.identifier(), StrayGroupClose::IDENTIFIER);
        assert!(
            diagnostic.message().contains("]]"),
            "diagnostic should name the 2-char delimiter, got: {}",
            diagnostic.message()
        );
    }

    // --- the parse setup: per-parse initial state and root parser ---------------------------

    #[test]
    fn parse_is_the_shorthand_of_parse_setup_over_a_fresh_source() {
        let language = tolerant();
        let short = language.parse("a}b {c}").unwrap();
        let long = language.parse_setup(Source::new("a}b {c}")).parse().unwrap();
        assert_eq!(shapes(&short), shapes(&long));
        assert_eq!(short.diagnostics.len(), long.diagnostics.len());
        // Each spelling minted its own source: positions do not correlate.
        assert_ne!(short.tree.root().span(), long.tree.root().span());
    }

    #[test]
    fn with_initial_state_starts_this_parse_from_the_given_state() {
        // Comments disabled for one parse only, from a state derived off the
        // language's seed: the root records that very handle, and the language's
        // own initial state is untouched.
        let language = strict();
        let no_comments = Arc::new(
            language
                .initial_state()
                .derived(&ParsingStateDelta::new().rules(TokenRulesOverrides {
                    comments: crate::state::CommentOverrides::disable(),
                    ..TokenRulesOverrides::default()
                }))
                .unwrap(),
        );
        let result = language
            .parse_setup(Source::new("a%b"))
            .with_initial_state(Arc::clone(&no_comments))
            .parse()
            .unwrap();
        check_tree_invariants(&result.tree);
        assert_eq!(shapes(&result), ["chars(a%b)"]);
        assert!(Arc::ptr_eq(result.tree.root().parsing_state(), &no_comments));
        assert!(language.initial_state().rules().comments_enabled());
        // The everyday path still parses under the language's seed (a comment node).
        assert_eq!(shapes(&language.parse("a%b").unwrap()), ["chars(a)", "other"]);
    }

    #[test]
    fn with_initial_state_accepts_a_parsed_nodes_state_by_identity() {
        let language = strict();
        let first = language.parse("x").unwrap();
        let node_state = Arc::clone(first.tree.root().parsing_state());
        let again = language
            .parse_setup(Source::new("y"))
            .with_initial_state(Arc::clone(&node_state))
            .parse()
            .unwrap();
        assert!(Arc::ptr_eq(again.tree.root().parsing_state(), &node_state));
    }

    /// A root parser staging the whole source as one `Chars` root — no content loop
    /// at all — and counting how often it ran, to be read back after the parse.
    struct CharsRoot {
        runs: usize,
    }

    impl<L: Lang> ConstructParser<L> for CharsRoot {
        type Output = crate::node::BuildId;
        fn parse(
            &mut self,
            cx: &mut ParseContext<'_, '_, L>,
        ) -> ConstructParserResult<
            L,
            (Self::Output, Option<alloc::boxed::Box<ParsingStateDelta<L>>>),
        > {
            self.runs += 1;
            let span = SourceSpan::entire(cx.here().source());
            let id = cx
                .stage_node(NodeKind::chars(span.span()), span.clone(), Arc::clone(&cx.state), Vec::new())
                .map_err(|error| cx.staging_error(error, span))?;
            Ok((id, None))
        }
    }

    #[test]
    fn with_root_parser_runs_the_given_parser_at_the_root() {
        // The borrowed parser replaces the driver's root parser for this parse, and
        // is available again afterwards: its count reads back.
        let language = strict();
        let mut root = CharsRoot { runs: 0 };
        let result = language
            .parse_setup(Source::new("a {b}"))
            .with_root_parser(&mut root)
            .parse()
            .unwrap();
        assert_eq!(root.runs, 1);
        assert_eq!(result.tree.root().chars(), Some("a {b}"));
        assert_eq!(result.tree.root().span().range(), 0..5);
        assert!(result.diagnostics.is_empty());
        // The language's own parses are unaffected: the standard root `List`.
        assert_eq!(shapes(&language.parse("a {b}").unwrap()), ["chars(a )", "group"]);
    }

    // --- the driver's root-parser factory ------------------------------------------------

    /// `DocLang` under a driver whose root parser wraps the content in its own
    /// scaffolding: the standard root run's nodes become the children of a `List`
    /// nested in the root `List` (a two-level root).
    #[derive(Debug, Clone, Copy)]
    struct RootLang;
    impl Lang for RootLang {
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
        type Driver = RootDriver;

        fn initial_state_data() -> Result<StateData<Self>, crate::state::FinalizeError> {
            Ok(doc_state_data())
        }
        fn make_node_ext(
            _kind: &crate::node::NodeKind<Self>,
            _span: &crate::source::SourceSpan<Self::SourceOrigin>,
            _state: &alloc::sync::Arc<crate::state::ParsingState<Self>>,
            _children: crate::node::StagedChildren<'_, Self>,
        ) -> Result<(), crate::node::NodeBuildError> {
            Ok(())
        }
    }

    /// Tolerant, so that a factory failure's abort is visibly policy-independent;
    /// `fail` makes the factory refuse to build its parser.
    #[derive(Debug, Clone, Copy)]
    struct RootDriver {
        fail: bool,
    }

    /// The scaffolding root parser: one content run, wrapped twice.
    struct WrappedRoot;

    impl ConstructParser<RootLang> for WrappedRoot {
        type Output = crate::node::BuildId;
        fn parse(
            &mut self,
            cx: &mut ParseContext<'_, '_, RootLang>,
        ) -> ConstructParserResult<
            RootLang,
            (Self::Output, Option<alloc::boxed::Box<ParsingStateDelta<RootLang>>>),
        > {
            let state = Arc::clone(&cx.state);
            let span = SourceSpan::entire(cx.here().source());
            let (outcome, _) = cx.parse_nodes(
                Arc::clone(&state),
                StopSpec::none(),
                ChildStateSpec::inherit(),
            )?;
            assert_eq!(outcome.stop, StopCause::EndOfInput);
            let inner = cx
                .stage_node(NodeKind::list(), span.clone(), Arc::clone(&state), outcome.nodes)
                .map_err(|error| cx.staging_error(error, span.clone()))?;
            let outer = cx
                .stage_node(NodeKind::list(), span.clone(), state, vec![inner])
                .map_err(|error| cx.staging_error(error, span))?;
            Ok((outer, None))
        }
    }

    impl ParseDriver<RootLang> for RootDriver {
        fn recovery(&self) -> Recovery {
            Recovery::Tolerant
        }
        fn make_root_parser<'p>(
            &'p self,
        ) -> Result<
            alloc::boxed::Box<dyn ConstructParser<RootLang, Output = crate::node::BuildId> + 'p>,
            ParseError,
        > {
            if self.fail {
                return Err(ParseError::new(
                    ImplementationError::new("no root parser available"),
                    SourceSpan::new(&Arc::new(Source::new("")), 0..0),
                ));
            }
            Ok(alloc::boxed::Box::new(WrappedRoot))
        }
    }

    #[test]
    fn the_drivers_root_parser_factory_shapes_every_parse_of_the_language() {
        let language: Language<RootLang> = Language::new(
            RootDriver { fail: false },
            ParsingState::lang_initial().expect("seed state"),
        );
        let result = language.parse("a {b}").unwrap();
        check_tree_invariants(&result.tree);
        // Root `List` → inner `List` → the content.
        let root = result.tree.root();
        assert_eq!(root.child_count(), 1);
        let inner = root.child(0).unwrap();
        assert_eq!(inner.child_count(), 2);
        assert_eq!(inner.child(0).unwrap().chars(), Some("a "));

        // A per-parse root parser takes precedence over the factory.
        let mut chars_root = CharsRoot { runs: 0 };
        let result = language
            .parse_setup(Source::new("a {b}"))
            .with_root_parser(&mut chars_root)
            .parse()
            .unwrap();
        assert_eq!(chars_root.runs, 1);
        assert_eq!(result.tree.root().chars(), Some("a {b}"));
    }

    #[test]
    fn a_root_parser_factory_failure_aborts_under_any_policy() {
        // The driver is tolerant, yet "could not build the parser" is an abort (the
        // hook fallibility contract) — and a per-parse root parser bypasses the
        // failing factory altogether.
        let language: Language<RootLang> = Language::new(
            RootDriver { fail: true },
            ParsingState::lang_initial().expect("seed state"),
        );
        let err = language.parse("a").unwrap_err();
        assert_eq!(err.identifier(), ImplementationError::IDENTIFIER);
        let mut chars_root = CharsRoot { runs: 0 };
        let result = language
            .parse_setup(Source::new("a"))
            .with_root_parser(&mut chars_root)
            .parse()
            .unwrap();
        assert_eq!(result.tree.root().chars(), Some("a"));
    }

    // --- observe_parse_start sees the parse's initial state ----------------------------

    /// `DocLang` under a driver whose `observe_parse_start` records, as a note, whether
    /// the parse's initial state has comments enabled.
    #[derive(Debug, Clone, Copy)]
    struct StartLang;
    impl Lang for StartLang {
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
        type Driver = StartDriver;

        fn initial_state_data() -> Result<StateData<Self>, crate::state::FinalizeError> {
            Ok(doc_state_data())
        }
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
    struct StartDriver;

    impl ParseDriver<StartLang> for StartDriver {
        fn observe_parse_start(
            &self,
            source: &Arc<Source>,
            initial_state: &Arc<ParsingState<StartLang>>,
            diagnostics: &mut crate::error::Diagnostics,
        ) {
            diagnostics.push(crate::error::Diagnostic::note(
                ImplementationError::new(alloc::format!(
                    "comments_enabled={}",
                    initial_state.rules().comments_enabled()
                )),
                SourceSpan::entire(source),
            ));
        }
    }

    #[test]
    fn observe_parse_start_receives_the_setups_initial_state() {
        let language: Language<StartLang> =
            Language::new(StartDriver, ParsingState::lang_initial().expect("seed state"));
        let no_comments = language
            .initial_state()
            .derived(&ParsingStateDelta::new().rules(TokenRulesOverrides {
                comments: crate::state::CommentOverrides::disable(),
                ..TokenRulesOverrides::default()
            }))
            .unwrap();
        // The note's message ends with the recorded flag (the condition type's
        // rendering prefixes it).
        let note = |result: &ParseResult<StartLang>| -> String {
            result.diagnostics.iter().next().unwrap().message()
        };
        assert!(note(&language.parse("a").unwrap()).ends_with("comments_enabled=true"));
        let result = language
            .parse_setup(Source::new("a"))
            .with_initial_state(no_comments)
            .parse()
            .unwrap();
        assert!(note(&result).ends_with("comments_enabled=false"));
    }
}
