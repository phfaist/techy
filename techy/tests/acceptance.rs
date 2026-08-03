//! Phase 7.9 acceptance suite: the selected slice of pylatexenc's
//! `test/test_2_latexwalker.py`, ported against the `latexlike` preset **through the
//! public API only** (this is an integration-test crate — anything the suite cannot
//! reach is an API gap by definition).
//!
//! Porting conventions:
//! - Each test names the behavior it pins; a `pylatexenc:` comment records the
//!   original test it descends from.
//! - Spans are asserted exactly (`outline` = `{byte range} {summary}` per node).
//! - The specs the inputs reference are registered test-side ([`support::testdb`]);
//!   the standard spec-database port is a later phase.
//! - Every happy-path input parses under **both** recovery modes via
//!   [`support::parse_ok`], which asserts the strict and tolerant trees are
//!   identical (the strict/tolerant matrix); error inputs get explicit per-mode
//!   tests.
//!
//! Omitted from the port, with reasons:
//! - `test_get_token`, `test_get_token_mathmodes`, `test_get_token_mathmodes_dollardollar`
//!   — token-layer behavior, covered by the `token` module's suites and the preset's
//!   `$a$$b$` boundary tests (`latexlike` 7.5).
//! - `test_get_latex_expression`, `test_get_latex_braced_group` — construct-level
//!   entry points, covered by the `constructs` unit suites (techy parses whole
//!   inputs; positional sub-parses are not part of the public entry).
//! - `test_get_latex_nodes_read_max_nodes`, `test_bug_000`, and the `read_max_nodes`
//!   halves of `test_bug_issueno49` — `read_max_nodes` is a pylatexenc strategy techy
//!   does not have (parses are whole-input; slicing is a view-API concern). The
//!   span-bookkeeping regression behind issue #49 is pinned span-exactly instead.
//! - `test_verbatim_via_specials` — specials-triggered verbatim arguments are
//!   std-database-era composition, not in the plan's 7.9 slice.
//! - `test_parsing_state_changes` — `\newcommand` is deferred (Phase 7 scope cut).

mod support {
    use std::sync::Arc;

    use techy::core::constructs::GroupArgumentParser;
    use techy::core::{Language, ParseResult};
    use techy::error::Recovery;
    use techy::latexlike::{
        argument_specs, base_package, default_token_rules, CallableType, EnvironmentSpec,
        GroupType, Latexlike, LatexlikeDriver, MacroSpec, Mode, VerbatimBehavior,
    };
    use techy::core::node::{check_tree_invariants, NodeRef};
    use techy::core::specs::{FallbackProvider, Package, ScopeOp};
    use techy::core::specs::ArgumentSpec;
    use techy::core::{ParsingStateDelta, TokenRulesOverrides};

    /// Argument specs from per-argument code strings ([`argument_specs`] + unwrap).
    pub fn args(codes: &[&str]) -> Vec<Arc<ArgumentSpec<Latexlike>>> {
        argument_specs(codes).unwrap()
    }

    /// A mandatory `{…}` argument whose interior parses back in text mode — the
    /// per-argument-delta equivalent of pylatexenc's `args_math_mode=[False]` (for
    /// `\text`/`\mbox`): mode back to [`Mode::Text`], the math delimiters restored as
    /// group openers, and `$` un-forbidden (the math plug's interior overrides,
    /// statically undone).
    pub fn text_mode_arg() -> Arc<ArgumentSpec<Latexlike>> {
        Arc::new(
            ArgumentSpec::new(Arc::new(GroupArgumentParser::new(GroupType::Content)))
                .with_state_delta(
                    ParsingStateDelta::new().mode(Mode::Text).rules(TokenRulesOverrides {
                        groups: Some(default_token_rules().groups),
                        forbidden_chars: Some("".into()),
                        ..TokenRulesOverrides::default()
                    }),
                ),
        )
    }

    /// The body delta of math environments (`align`, `cases`): enter
    /// [`Mode::Math`] for the body's extent.
    pub fn math_body_delta() -> ParsingStateDelta<Latexlike> {
        ParsingStateDelta::new().mode(Mode::Math)
    }

    /// The suite's shared test-side spec database: the minimal set the ported tests
    /// reference (pylatexenc's default context supplies these; techy's standard
    /// database port is a later phase).
    pub fn testdb() -> Package<Latexlike> {
        let mut db = Package::new("testdb");

        let macros: &[(&str, &[&str])] = &[
            ("textbf", &["m"]),
            ("emph", &["m"]),
            ("vec", &["m"]),
            ("hat", &["m"]),
            ("label", &["m"]),
            ("eqref", &["m"]),
            ("itshape", &[]),
            ("it", &[]),
            ("quad", &[]),
            ("qquad", &[]),
            ("dagger", &[]),
            ("cite", &["o", "m"]),
            ("documentclass", &["o", "m"]),
            ("usepackage", &["o", "m"]),
            ("item", &["o"]),
        ];
        for (name, codes) in macros {
            db.insert(CallableType::Macro, *name, Arc::new(MacroSpec::new(args(codes))));
        }
        // `\text`/`\mbox`: a mandatory argument parsed back in text mode.
        for name in ["text", "mbox"] {
            db.insert(
                CallableType::Macro,
                name,
                Arc::new(MacroSpec::new(vec![text_mode_arg()])),
            );
        }
        // `\verb`: delimited verbatim, auto-matched delimiter.
        db.insert(CallableType::Macro, "verb", Arc::new(MacroSpec::new(args(&["v"]))));

        let environments: &[(&str, &[&str])] = &[
            ("enumerate", &["o"]),
            ("itemize", &[]),
            ("document", &[]),
            ("tabular", &["m"]),
        ];
        for (name, codes) in environments {
            db.insert(
                CallableType::Environment,
                *name,
                Arc::new(EnvironmentSpec::new(args(codes))),
            );
        }
        for name in ["align", "cases"] {
            db.insert(
                CallableType::Environment,
                name,
                Arc::new(EnvironmentSpec::new(Vec::new()).with_body_delta(math_body_delta())),
            );
        }
        db.insert(
            CallableType::Environment,
            "verbatim",
            Arc::new(EnvironmentSpec::from_behavior(Arc::new(VerbatimBehavior::default()))),
        );
        db.insert(
            CallableType::Environment,
            "lstlisting",
            Arc::new(EnvironmentSpec::from_behavior(Arc::new(VerbatimBehavior::new(args(
                &["o"],
            ))))),
        );
        db
    }

    /// A latexlike `Language` with [`testdb`] pushed, under `recovery`.
    pub fn with_recovery(recovery: Recovery) -> Language<Latexlike> {
        Language::new(LatexlikeDriver::new(recovery))
            .with_provider(Arc::new(testdb()))
            .unwrap()
    }

    /// The strict suite language ([`testdb`] over the seed defaults).
    pub fn strict() -> Language<Latexlike> {
        with_recovery(Recovery::Strict)
    }

    /// The tolerant suite language.
    pub fn tolerant() -> Language<Latexlike> {
        with_recovery(Recovery::Tolerant)
    }

    /// The `test_errors` language: [`testdb`] over `base`, with a bottom-of-stack
    /// [`FallbackProvider`] answering **any** macro name as a zero-argument macro —
    /// pylatexenc parity, where an unknown macro parses as an argument-less macro
    /// node instead of erroring. Built with [`ScopeOp::ReplaceStack`] (outermost
    /// first) so the fallback is searched last and shadows nothing.
    pub fn with_macro_fallback(recovery: Recovery) -> Language<Latexlike> {
        let mut fallback = FallbackProvider::new("anymacro");
        fallback.set(CallableType::Macro, Arc::new(MacroSpec::default()));
        Language::new(LatexlikeDriver::new(recovery))
            .with_seed_delta(ParsingStateDelta::new().scope_op(ScopeOp::ReplaceStack(vec![
                Arc::new(fallback),
                Arc::new(base_package()),
                Arc::new(testdb()),
            ])))
            .unwrap()
    }

    /// `{byte range} {summary}` per node — the suite's exact-shape currency.
    pub fn outline<'t>(
        nodes: impl IntoIterator<Item = NodeRef<'t, Latexlike>>,
    ) -> Vec<String> {
        nodes
            .into_iter()
            .map(|node| format!("{:?} {}", node.span().range(), node.summary()))
            .collect()
    }

    /// The whole tree's outline in document order (strict/tolerant comparison key).
    pub fn fingerprint(result: &ParseResult<Latexlike>) -> Vec<String> {
        outline(result.tree.root().descendants())
    }

    /// Parse a happy-path input in **both** recovery modes: assert tree invariants
    /// and zero diagnostics in each, and that the two trees are identical — then
    /// return the strict result for the caller's shape assertions.
    pub fn parse_ok_in(
        language_for: impl Fn(Recovery) -> Language<Latexlike>,
        input: &str,
    ) -> ParseResult<Latexlike> {
        let result = language_for(Recovery::Strict).parse(input).unwrap();
        check_tree_invariants(&result.tree);
        assert!(
            result.diagnostics.is_empty(),
            "unexpected strict diagnostics: {:?}",
            result.diagnostics
        );
        let tolerant_result = language_for(Recovery::Tolerant).parse(input).unwrap();
        check_tree_invariants(&tolerant_result.tree);
        assert!(
            tolerant_result.diagnostics.is_empty(),
            "unexpected tolerant diagnostics: {:?}",
            tolerant_result.diagnostics
        );
        assert_eq!(
            fingerprint(&result),
            fingerprint(&tolerant_result),
            "strict and tolerant trees diverge on a happy-path input"
        );
        result
    }

    /// [`parse_ok_in`] under the suite's default languages.
    pub fn parse_ok(input: &str) -> ParseResult<Latexlike> {
        parse_ok_in(with_recovery, input)
    }

    /// The joined chars content of argument `i` of `node` (house helper from the
    /// factory suite).
    pub fn argument_chars(node: NodeRef<'_, Latexlike>, i: usize) -> String {
        node.argument_content_nodes(i)
            .expect("provided argument")
            .iter()
            .map(|child| child.chars().unwrap_or("<non-chars>").to_string())
            .collect()
    }
}

// -------------------------------------------------------------------------------------
// Optional arguments
// -------------------------------------------------------------------------------------

mod optional_args {
    use super::support::*;
    use std::sync::Arc;
    use techy::error::Recovery;
    use techy::latexlike::{CallableType, MacroSpec};
    use techy::core::specs::Package;

    // pylatexenc: test_get_latex_maybe_optional_arg (the `\cite[Lemma 3]{Author}`
    // half), node-level.
    #[test]
    fn cite_takes_an_optional_and_a_mandatory_argument() {
        let input = r"Indeed thanks to \cite[Lemma 3]{Author}, we know that...";
        let result = parse_ok(input);
        assert_eq!(
            outline(result.tree.root().children()),
            [
                "0..17 chars(Indeed thanks to )",
                "17..39 Macro(cite)",
                "39..56 chars(, we know that...)",
            ]
        );

        let cite = result.tree.root().child(1).unwrap();
        assert!(cite.arguments().unwrap().get(0).unwrap().is_provided());
        // The optional argument's region is the minted `[…]` group; its content is
        // the group's interior.
        assert_eq!(outline(cite.argument_nodes(0).unwrap()), ["22..31 group(Content [ ])"]);
        assert_eq!(outline(cite.argument_content_nodes(0).unwrap()), ["23..30 chars(Lemma 3)"]);
        assert_eq!(outline(cite.argument_content_nodes(1).unwrap()), ["32..38 chars(Author)"]);
    }

    // pylatexenc: test_get_latex_maybe_optional_arg (the `\textbf` half — no `[`
    // ahead means no optional argument).
    #[test]
    fn absent_optional_argument_is_recorded_absent() {
        let result = parse_ok(r"\cite{Author} x");
        let cite = result.tree.root().child(0).unwrap();
        assert!(!cite.arguments().unwrap().get(0).unwrap().is_provided());
        assert_eq!(argument_chars(cite, 1), "Author");
        // Nothing past the mandatory argument was consumed.
        assert_eq!(
            outline(result.tree.root().children()),
            ["0..13 Macro(cite)", "13..15 chars( x)"]
        );
    }

    // pylatexenc: test_bug_issueno57 — trailing optional arguments absent at end of
    // input must not error (argspec `[{[`, ported as `o m o`).
    #[test]
    fn trailing_optionals_absent_at_end_of_input_are_no_error() {
        let language_for = |recovery: Recovery| {
            let mut package = Package::new("issue57");
            package.insert(
                CallableType::Macro,
                "foo",
                Arc::new(MacroSpec::new(args(&["o", "m", "o"]))),
            );
            with_recovery(recovery).with_provider(Arc::new(package)).unwrap()
        };
        let result = parse_ok_in(language_for, r"\foo{test}");

        let foo = result.tree.root().child(0).unwrap();
        assert_eq!(outline([foo]), ["0..10 Macro(foo)"]);
        let arguments = foo.arguments().unwrap();
        assert_eq!(arguments.len(), 3);
        assert!(!arguments.get(0).unwrap().is_provided());
        assert!(arguments.get(1).unwrap().is_provided());
        assert!(!arguments.get(2).unwrap().is_provided());
        assert_eq!(argument_chars(foo, 1), "test");
    }
}

// -------------------------------------------------------------------------------------
// Environments
// -------------------------------------------------------------------------------------

mod environments {
    use super::support::*;

    // pylatexenc: test_get_latex_environment — the `enumerate` block with an
    // optional argument, `\item` invocations, and an interleaved comment.
    #[test]
    fn enumerate_with_optional_argument_items_and_comment() {
        // offsets:  0        d17    d22 d23   d29          d40                    d63      d71                d89
        let input = "\\begin{enumerate}[(i)]\n\\item Hi there!  % here goes a comment\n \\item[a] Hello!  @@@\n     \\end{enumerate}";
        let result = parse_ok(input);

        let env = result.tree.root().child(0).unwrap();
        assert_eq!(outline([env]), ["0..104 Environment(enumerate)"]);
        assert_eq!(env.environment_name(), Some("enumerate"));

        // The environment's optional argument.
        assert!(env.arguments().unwrap().get(0).unwrap().is_provided());
        assert_eq!(outline(env.argument_content_nodes(0).unwrap()), ["18..21 chars((i))"]);

        // The body, span-exact. The first `\item`'s post-space is the blank after
        // the name (pylatexenc's macro_post_space); the second `\item` is directly
        // followed by its optional argument, so its post-space is empty.
        assert_eq!(
            outline(env.body().unwrap()),
            [
                "22..23 chars(\n)",
                "23..29 Macro(item)",
                "29..40 chars(Hi there!  )",
                "40..63 comment( here goes a comment)",
                "63..71 Macro(item)",
                "71..89 chars( Hello!  @@@\n     )",
            ]
        );

        let first_item = env.body().unwrap().get(1).unwrap();
        assert_eq!(first_item.post_space(), Some(" "));
        assert!(!first_item.arguments().unwrap().get(0).unwrap().is_provided());

        let second_item = env.body().unwrap().get(4).unwrap();
        assert_eq!(second_item.post_space(), Some(""));
        assert_eq!(argument_chars(second_item, 0), "a");

        let comment = env.body().unwrap().get(3).unwrap();
        assert_eq!(comment.comment(), Some(" here goes a comment"));
        assert_eq!(comment.comment_post_space(), Some("\n "));
    }

    // pylatexenc: test_get_latex_environment (the `XYZNFKLD-WRONG` assertRaises
    // half): an unregistered environment name.
    #[test]
    fn unknown_environment_aborts_strict_and_recovers_tolerant() {
        let input = r"\begin{nosuchenv} x \end{nosuchenv}";
        let err = strict().parse(input).unwrap_err();
        assert!(err.to_string().contains("unknown environment"), "{err}");

        let result = tolerant().parse(input).unwrap();
        techy::core::node::check_tree_invariants(&result.tree);
        assert_eq!(result.diagnostics.len(), 1);
        // The body-only fallback still runs the body to its terminator.
        let env = result.tree.root().child(0).unwrap();
        assert_eq!(env.environment_name(), Some("nosuchenv"));
        assert_eq!(outline(env.body().unwrap()), ["17..20 chars( x )"]);
    }
}

// -------------------------------------------------------------------------------------
// General node runs
// -------------------------------------------------------------------------------------

mod general {
    use super::support::*;
    use std::sync::Arc;
    use techy::error::Recovery;
    use techy::latexlike::{CallableType, MacroSpec};
    use techy::core::specs::Package;

    // pylatexenc: test_get_latex_nodes (the `Also: {\itshape some italic text}.`
    // slice — the part its own expected structure covers).
    #[test]
    fn group_and_macro_content_parses_with_exact_spans() {
        let input = "Also: {\\itshape some italic text}.\n";
        let result = parse_ok(input);
        assert_eq!(
            outline(result.tree.root().children()),
            ["0..6 chars(Also: )", "6..33 group(Content { })", "33..35 chars(.\n)"]
        );
        let group = result.tree.root().child(1).unwrap();
        assert_eq!(
            outline(group.children()),
            ["7..16 Macro(itshape)", "16..32 chars(some italic text)"]
        );
        assert_eq!(group.child(0).unwrap().post_space(), Some(" "));
    }

    // pylatexenc: test_get_latex_nodes ("test our own macro lists etc." — `cite`
    // redefined with four mandatory arguments changes how the same line parses;
    // techy-side this also demonstrates innermost-provider shadowing).
    #[test]
    fn shadowing_package_redefines_cite_with_four_mandatory_arguments() {
        let language_for = |recovery: Recovery| {
            let mut package = Package::new("cite4");
            package.insert(
                CallableType::Macro,
                "cite",
                Arc::new(MacroSpec::new(args(&["m", "m", "m", "m"]))),
            );
            with_recovery(recovery).with_provider(Arc::new(package)).unwrap()
        };
        let input = r"Indeed thanks to \cite[Lemma 3]{Author}, we know that...";
        let result = parse_ok_in(language_for, input);

        // `[`, `L`, `e`, `m` are the four single-expression arguments; the rest of
        // the line reflows around them.
        assert_eq!(
            outline(result.tree.root().children()),
            [
                "0..17 chars(Indeed thanks to )",
                "17..26 Macro(cite)",
                "26..31 chars(ma 3])",
                "31..39 group(Content { })",
                "39..56 chars(, we know that...)",
            ]
        );
        let cite = result.tree.root().child(1).unwrap();
        for (i, expected) in ["[", "L", "e", "m"].iter().enumerate() {
            assert_eq!(argument_chars(cite, i), *expected);
        }
    }

    // pylatexenc: test_bug_issueno49 — the span bookkeeping around a
    // whitespace-carrying chars flush between two macros (the read_max_nodes
    // trigger is a pylatexenc strategy techy does not have; the spans are the
    // regression's substance).
    #[test]
    fn spans_stay_exact_across_whitespace_flushes() {
        let result = parse_ok("\\emph{A}\n\\emph{B}");
        assert_eq!(
            outline(result.tree.root().children()),
            ["0..8 Macro(emph)", "8..9 chars(\n)", "9..17 Macro(emph)"]
        );
    }
}

// -------------------------------------------------------------------------------------
// Comments
// -------------------------------------------------------------------------------------

mod comments {
    use super::support::*;

    // pylatexenc: test_get_latex_nodes_comments (first document). techy's default
    // paragraph-break node is a whitespace chars node (pylatexenc-legacy's shape;
    // pylatexenc-modern emits a `\n\n` specials node — see the paragraph_breaks
    // module for that style).
    #[test]
    fn comments_record_content_and_post_space_exactly() {
        let input = "Hello % comment here\n  % more comments \n\nNew paragraph here.\n% comment at end";
        let result = parse_ok(input);
        assert_eq!(
            outline(result.tree.root().children()),
            [
                "0..6 chars(Hello )",
                "6..23 comment( comment here)",
                "23..39 comment( more comments )",
                "39..41 chars(\n\n)",
                "41..61 chars(New paragraph here.\n)",
                "61..77 comment( comment at end)",
            ]
        );

        let root = result.tree.root();
        // Post-space: newline plus following indentation…
        assert_eq!(root.child(1).unwrap().comment_post_space(), Some("\n  "));
        // …but empty when the comment borders a paragraph break or end of input.
        assert_eq!(root.child(2).unwrap().comment_post_space(), Some(""));
        assert_eq!(root.child(5).unwrap().comment_post_space(), Some(""));
    }

    // pylatexenc: test_get_latex_nodes_comments (second document): comment directly
    // after a macro, multi-line comment runs, two paragraph breaks.
    #[test]
    fn comments_interleave_with_macros_and_paragraph_breaks() {
        let input = "Line with % a comment here\n\n% line comment on its own\n% and a second line\n\nAlso a \\itshape% comment after a macro\n% and also a second line\nsome italic text.";
        let result = parse_ok(input);
        assert_eq!(
            outline(result.tree.root().children()),
            [
                "0..10 chars(Line with )",
                "10..26 comment( a comment here)",
                "26..28 chars(\n\n)",
                "28..54 comment( line comment on its own)",
                "54..73 comment( and a second line)",
                "73..75 chars(\n\n)",
                "75..82 chars(Also a )",
                "82..90 Macro(itshape)",
                "90..114 comment( comment after a macro)",
                "114..139 comment( and also a second line)",
                "139..156 chars(some italic text.)",
            ]
        );
        // `\itshape%`: the macro is directly followed by the comment — no
        // post-space on the macro, and the comment's own post-space is its
        // terminating newline.
        let root = result.tree.root();
        assert_eq!(root.child(7).unwrap().post_space(), Some(""));
        assert_eq!(root.child(8).unwrap().comment_post_space(), Some("\n"));
    }
}

// -------------------------------------------------------------------------------------
// Math modes
// -------------------------------------------------------------------------------------

mod math_modes {
    use super::support::*;
    use techy::latexlike::{MathStyle, Mode};

    const MATHMODES_TEXT: &str = "\nHere is an inline expression like $\\vec{x} + \\hat p$ and a display equation $$\n   ax + b = y\n$$ and another, with a subtle inner math mode:\n\\[ cx^2+z=-d\\quad\\text{if $x<0$} \\]\nAnd a final inline math mode \\(\\mbox{Prob}(\\mbox{some event if \\(x>0\\)})=1\\).\n";

    // pylatexenc: test_get_latex_nodes_mathmodes (the `$…$` part).
    #[test]
    fn inline_dollar_math_with_argument_macros() {
        let result = parse_ok(MATHMODES_TEXT);
        let p = MATHMODES_TEXT.find('$').unwrap();

        let math = result.tree.root().child(1).unwrap();
        assert_eq!(outline([math]), [format!("{}..{} group(Math $ $)", p, p + 18)]);
        assert_eq!(math.math_style(), Some(MathStyle::Inline));
        assert_eq!(
            outline(math.children()),
            [
                format!("{}..{} Macro(vec)", p + 1, p + 8),
                format!("{}..{} chars( + )", p + 8, p + 11),
                format!("{}..{} Macro(hat)", p + 11, p + 17),
            ]
        );

        // The interior parses in math mode; the group node itself was parsed in the
        // surrounding text mode.
        assert_eq!(math.parsing_state().mode(), Mode::Text);
        for child in math.children() {
            assert_eq!(child.parsing_state().mode(), Mode::Math);
        }

        let vec = math.child(0).unwrap();
        assert_eq!(argument_chars(vec, 0), "x");
        let hat = math.child(2).unwrap();
        assert_eq!(hat.post_space(), Some(" "));
        assert_eq!(argument_chars(hat, 0), "p");
    }

    // pylatexenc: test_get_latex_nodes_mathmodes (the `$$…$$` part).
    #[test]
    fn display_dollar_math_contains_plain_chars() {
        let result = parse_ok(MATHMODES_TEXT);
        let p = MATHMODES_TEXT.find("$$").unwrap();

        let math = result.tree.root().child(3).unwrap();
        assert_eq!(outline([math]), [format!("{}..{} group(Math $$ $$)", p, p + 19)]);
        assert_eq!(math.math_style(), Some(MathStyle::Display));
        assert_eq!(
            outline(math.children()),
            [format!("{}..{} chars(\n   ax + b = y\n)", p + 2, p + 17)]
        );
    }

    // pylatexenc: test_get_latex_nodes_mathmodes (the `\[…\]` part): `\text{…}`
    // resets to text mode inside display math, and its argument contains a nested
    // inline `$…$` — the in_math_mode assertions ported onto recorded node states.
    #[test]
    fn bracket_display_math_with_text_mode_reset_inside() {
        let result = parse_ok(MATHMODES_TEXT);
        let p = MATHMODES_TEXT.find("\\[").unwrap();

        let math = result.tree.root().child(5).unwrap();
        assert_eq!(outline([math]), [format!("{}..{} group(Math \\[ \\])", p, p + 35)]);
        assert_eq!(math.math_style(), Some(MathStyle::Display));
        assert_eq!(
            outline(math.children()),
            [
                format!("{}..{} chars( cx^2+z=-d)", p + 2, p + 12),
                format!("{}..{} Macro(quad)", p + 12, p + 17),
                format!("{}..{} Macro(text)", p + 17, p + 32),
                format!("{}..{} chars( )", p + 32, p + 33),
            ]
        );
        // `^` is a plain character (sub/superscript specials are std-database
        // material; pylatexenc's default walker leaves them as chars too).
        assert_eq!(math.child(0).unwrap().parsing_state().mode(), Mode::Math);

        // Inside `\text{…}`: text mode again (pylatexenc: ps2.in_math_mode is
        // False)…
        let text = math.child(2).unwrap();
        let text_arg = text.argument_content_nodes(0).unwrap();
        assert_eq!(
            outline(text_arg),
            [
                format!("{}..{} chars(if )", p + 23, p + 26),
                format!("{}..{} group(Math $ $)", p + 26, p + 31),
            ]
        );
        assert_eq!(text_arg.get(0).unwrap().parsing_state().mode(), Mode::Text);

        // …and the nested `$…$` interior is math again (ps3.in_math_mode is True).
        let nested = text_arg.get(1).unwrap();
        assert_eq!(nested.math_style(), Some(MathStyle::Inline));
        let nested_chars = nested.child(0).unwrap();
        assert_eq!(nested_chars.chars(), Some("x<0"));
        assert_eq!(nested_chars.parsing_state().mode(), Mode::Math);
    }

    // pylatexenc: test_get_latex_nodes_mathmodes (the `\(…\)` part): `\mbox`
    // arguments in text mode, with a nested `\(…\)` inline math inside the second.
    #[test]
    fn paren_inline_math_with_nested_paren_math_inside_mbox() {
        let result = parse_ok(MATHMODES_TEXT);
        let p = MATHMODES_TEXT.find("\\(").unwrap();

        let math = result.tree.root().child(7).unwrap();
        assert_eq!(outline([math]), [format!("{}..{} group(Math \\( \\))", p, p + 47)]);
        assert_eq!(math.math_style(), Some(MathStyle::Inline));
        assert_eq!(
            outline(math.children()),
            [
                format!("{}..{} Macro(mbox)", p + 2, p + 13),
                format!("{}..{} chars(()", p + 13, p + 14),
                format!("{}..{} Macro(mbox)", p + 14, p + 42),
                format!("{}..{} chars()=1)", p + 42, p + 45),
            ]
        );

        // First `\mbox{Prob}`: argument interior in text mode.
        let mbox1 = math.child(0).unwrap();
        assert_eq!(argument_chars(mbox1, 0), "Prob");
        let prob = mbox1.argument_content_nodes(0).unwrap().get(0).unwrap();
        assert_eq!(prob.parsing_state().mode(), Mode::Text);

        // Second `\mbox{some event if \(x>0\)}`: text mode, with a nested inline
        // `\(…\)` math group inside.
        let mbox2 = math.child(2).unwrap();
        let mbox2_content = mbox2.argument_content_nodes(0).unwrap();
        assert_eq!(mbox2_content.get(0).unwrap().chars(), Some("some event if "));
        assert_eq!(mbox2_content.get(0).unwrap().parsing_state().mode(), Mode::Text);
        let nested = mbox2_content.get(1).unwrap();
        assert_eq!(nested.math_style(), Some(MathStyle::Inline));
        assert_eq!(nested.group_delimiters(), Some(("\\(", "\\)")));
        assert_eq!(nested.child(0).unwrap().chars(), Some("x>0"));
        assert_eq!(nested.child(0).unwrap().parsing_state().mode(), Mode::Math);
    }

    // pylatexenc: test_get_latex_nodes_mathmodes_dollardollar — plus the tail
    // pylatexenc commented out of its own input (`$$A=B\mbox{$b=a$}$$`), which
    // techy handles: `$` boundaries close before they open, and the `\mbox` reset
    // re-enables `$` inside display math.
    #[test]
    fn consecutive_inline_and_display_math_share_dollar_boundaries() {
        // pos ruler:  0    1        9    10       18   19  21   24     29  30   34  36
        let input = "x$\\dagger$$\\dagger$$$A=B\\mbox{$b=a$}$$";
        let result = parse_ok(input);
        assert_eq!(
            outline(result.tree.root().children()),
            [
                "0..1 chars(x)",
                "1..10 group(Math $ $)",
                "10..19 group(Math $ $)",
                "19..38 group(Math $$ $$)",
            ]
        );

        let first = result.tree.root().child(1).unwrap();
        assert_eq!(outline(first.children()), ["2..9 Macro(dagger)"]);
        assert_eq!(first.math_style(), Some(MathStyle::Inline));
        let second = result.tree.root().child(2).unwrap();
        assert_eq!(outline(second.children()), ["11..18 Macro(dagger)"]);

        let display = result.tree.root().child(3).unwrap();
        assert_eq!(display.math_style(), Some(MathStyle::Display));
        assert_eq!(
            outline(display.children()),
            ["21..24 chars(A=B)", "24..36 Macro(mbox)"]
        );
        let mbox = display.child(1).unwrap();
        let inner = mbox.argument_content_nodes(0).unwrap().get(0).unwrap();
        assert_eq!(outline([inner]), ["30..35 group(Math $ $)"]);
        assert_eq!(inner.child(0).unwrap().chars(), Some("b=a"));
    }
}

// -------------------------------------------------------------------------------------
// Verbatim
// -------------------------------------------------------------------------------------

mod verbatim {
    use super::support::*;

    // pylatexenc: test_verbatim — `\verb+…+` and the `verbatim` environment, modern
    // node shapes (group + chars; the newline right after `\begin{verbatim}` is
    // staged but designated out of the body content — techy's 7.7 gobble rule).
    #[test]
    fn verb_macro_and_verbatim_environment_read_raw_content() {
        let input = "Use the environment \\verb+\\begin{verbatim}...\\end{verbatim}+ to\n\\begin{verbatim}\ntypeset \\verbatim text with \\LaTeX $ escapes \\(like this\\).\n\\end{verbatim}\nThis is it.";
        let result = parse_ok(input);
        assert_eq!(
            outline(result.tree.root().children()),
            [
                "0..20 chars(Use the environment )",
                "20..60 Macro(verb)",
                "60..64 chars( to\n)",
                "64..155 Environment(verbatim)",
                "155..167 chars(\nThis is it.)",
            ]
        );

        // `\verb+…+`: the argument is a Verbatim-class group with the raw content
        // as its single chars child — comment/escape/math characters inert.
        let verb = result.tree.root().child(1).unwrap();
        assert_eq!(outline(verb.argument_nodes(0).unwrap()), ["25..60 group(Verbatim + +)"]);
        assert_eq!(
            outline(verb.argument_content_nodes(0).unwrap()),
            ["26..59 chars(\\begin{verbatim}...\\end{verbatim})"]
        );

        // The environment body: raw chars up to the literal `\end{verbatim}`; the
        // newline gobbled after `\begin{verbatim}` is not part of the content.
        let env = result.tree.root().child(3).unwrap();
        assert_eq!(env.environment_name(), Some("verbatim"));
        assert_eq!(
            outline(env.body().unwrap()),
            ["81..141 chars(typeset \\verbatim text with \\LaTeX $ escapes \\(like this\\).\n)"]
        );
    }

    // pylatexenc: test_lstlisting_handling — a raw body full of tokenization traps
    // (unmatched brace, comment markers, `$`).
    #[test]
    fn lstlisting_reads_a_raw_body() {
        let input = "Use lstlisting environment for code\n\\begin{lstlisting}\nint foo() {\n    \"^\\\\[1-9]+\\n$\" // some special chars\n    // unmatched open brace confuses non-verbatim parsing\n\\end{lstlisting}\nThis is it.";
        let result = parse_ok(input);
        assert_eq!(
            outline(result.tree.root().children()),
            [
                "0..36 chars(Use lstlisting environment for code\n)",
                "36..182 Environment(lstlisting)",
                "182..194 chars(\nThis is it.)",
            ]
        );

        let env = result.tree.root().child(1).unwrap();
        assert!(!env.arguments().unwrap().get(0).unwrap().is_provided());
        assert_eq!(
            outline(env.body().unwrap()),
            ["55..166 chars(int foo() {\n    \"^\\\\[1-9]+\\n$\" // some special chars\n    // unmatched open brace confuses non-verbatim parsing\n)"]
        );
    }

    // pylatexenc: test_lstlisting_withoptarg — the optional argument parses
    // (tokenized) ahead of the raw body.
    #[test]
    fn lstlisting_takes_an_optional_argument_before_the_raw_body() {
        let input = "Use lstlisting environment for code\n\\begin{lstlisting}[language=C]\nint foo() {\n    \"^\\\\[1-9]+\\n$\" // some special chars\n    // unmatched open brace confuses non-verbatim parsing\n\\end{lstlisting}\nThis is it.";
        let result = parse_ok(input);

        let env = result.tree.root().child(1).unwrap();
        assert_eq!(outline([env]), ["36..194 Environment(lstlisting)"]);
        assert!(env.arguments().unwrap().get(0).unwrap().is_provided());
        assert_eq!(outline(env.argument_nodes(0).unwrap()), ["54..66 group(Content [ ])"]);
        assert_eq!(argument_chars(env, 0), "language=C");
        assert_eq!(
            outline(env.body().unwrap()),
            ["67..178 chars(int foo() {\n    \"^\\\\[1-9]+\\n$\" // some special chars\n    // unmatched open brace confuses non-verbatim parsing\n)"]
        );
    }
}

// -------------------------------------------------------------------------------------
// Errors and recovery
// -------------------------------------------------------------------------------------

mod errors {
    use super::support::*;
    use std::sync::Arc;
    use techy::error::Recovery;
    use techy::latexlike::{CallableType, SpecialsSpec};
    use techy::core::node::check_tree_invariants;
    use techy::core::specs::Package;

    /// pylatexenc's `get_test_latex_data_with_possible_inconsistencies()`: a
    /// document whose deliberate inconsistencies are the mismatched
    /// `\begin{abc}…\end{zzzzz}`, a stray `}`, and an unclosed `{`.
    const INCONSISTENT_DOCUMENT: &str = r#"\documentclass[11pt,a4paper]{article}

\usepackage[utf8]{inputenc}
\usepackage{graphicx}
\usepackage{amsmath}

\begin{document}
D sklfmdklaf dafdaf sah. fjdkslanf \textbf{djksa fnjdka} fdsl\`a; \emph{fnkdsl} a. Fioe
fndjksf.  More {\it italic test} text. Inline math $\vec x$ and
\begin{align}
  \label{eq:Alphas}
  \alpha_{\boldk,\uparrow} =
  \begin{cases} c_{\boldk,\uparrow} & \text{for $k>k_F$} \\
    c_{-\boldk,\downarrow}^\dagger & \text{for $k<k_F$} \end{cases}
  \qquad ; \qquad
  \alpha_{\boldk,\downarrow} =
  \begin{cases} c_{\boldk,\downarrow} & \text{for $k>k_F$} \\
    c_{-\boldk,\uparrow}^\dagger & \text{for $k<k_F$} \end{cases}
  \quad .
\end{align}

\begin{itemize}
\item Show that the $\alpha$, $\alpha^\dagger$'s obey fermionic commutation relations.

  Remember that fermionic creation and annihilation operators $\alpha^\dagger$,
  $\alpha$ obey
  \begin{align}
      \{\alpha^\nodagger_{\boldk,s}, \alpha^\nodagger_{\boldk',s'}\}
      = \{\alpha_{\boldk,s}^\dagger, \alpha_{\boldk',s'}^\dagger\} = 0
      \quad;\quad
      \{\alpha^\nodagger_{\boldk,s}, \alpha_{\boldk',s'}^\dagger\}
      = \delta_{\boldk,\boldk'}\delta_{s,s'}\ .
  \end{align}

\item Argue that eq.~\eqref{eq:Alphas} is a unitary transformation of the creation and annihilation
  operators. Such a transformation is also called a \emph{Bogoliubov transformation}.
  What happens if you act with the annihilators $c_{\boldk,s}$ and
  $\alpha_{\boldk,s}$ on the ground state of the gas?
\end{itemize}

\begin{abc}
  This environment is called `abc,' but I'm going to close it with the wrong "end" command.

  This will confuse the parser:
\end{zzzzz}

What would you think about a stray closing brace: } ?

This is a final sentence. { <-- this brace is not closed.

\end{document}
"#;

    // pylatexenc: test_errors — strict raises, tolerant completes.
    #[test]
    fn inconsistent_document_aborts_strict_and_completes_tolerant() {
        assert!(with_macro_fallback(Recovery::Strict).parse(INCONSISTENT_DOCUMENT).is_err());

        let result =
            with_macro_fallback(Recovery::Tolerant).parse(INCONSISTENT_DOCUMENT).unwrap();
        check_tree_invariants(&result.tree);
        assert!(!result.diagnostics.is_empty());
    }

    // The fallback provider resolves the document's unregistered macro names
    // (`\boldk`, `\nodagger`, …) as zero-argument macros — pylatexenc parity, where
    // unknown macro names are not errors.
    #[test]
    fn unknown_macros_resolve_through_the_fallback_provider() {
        let result =
            with_macro_fallback(Recovery::Tolerant).parse(INCONSISTENT_DOCUMENT).unwrap();
        let mut macro_names = result
            .tree
            .root()
            .descendants()
            .filter_map(|node| node.macro_name().map(str::to_string));
        assert!(macro_names.any(|name| name == "boldk"));

        // …while the registered specs keep their argument structures: `\textbf`
        // consumed its brace group as an argument.
        let textbf = result
            .tree
            .root()
            .descendants()
            .find(|node| node.macro_name() == Some("textbf"))
            .unwrap();
        assert_eq!(argument_chars(textbf, 0), "djksa fnjdka");
    }

    // pylatexenc: test_error_dangling_missing_args_0 (`Test \textbf` — the macro's
    // mandatory argument is missing at end of input).
    #[test]
    fn dangling_macro_missing_its_argument() {
        let input = r"Test \textbf";
        let err = strict().parse(input).unwrap_err();
        assert!(err.to_string().contains("missing mandatory argument"), "{err}");

        // pylatexenc left the tolerant shape an open question (its 0b test is
        // commented out); techy's is defined: the macro stages with the argument
        // recorded absent.
        let result = tolerant().parse(input).unwrap();
        check_tree_invariants(&result.tree);
        assert_eq!(result.diagnostics.len(), 1);
        assert_eq!(
            outline(result.tree.root().children()),
            ["0..5 chars(Test )", "5..12 Macro(textbf)"]
        );
        let textbf = result.tree.root().child(1).unwrap();
        assert!(!textbf.arguments().unwrap().get(0).unwrap().is_provided());
    }

    // pylatexenc: test_error_dangling_missing_args_1 (`Test \begin{tabular}` — the
    // environment's mandatory argument and terminator are both missing).
    #[test]
    fn dangling_begin_missing_environment_arguments() {
        let input = r"Test \begin{tabular}";
        assert!(strict().parse(input).is_err());

        let result = tolerant().parse(input).unwrap();
        check_tree_invariants(&result.tree);
        assert!(!result.diagnostics.is_empty());
        let env = result.tree.root().child(1).unwrap();
        assert_eq!(env.environment_name(), Some("tabular"));
        assert!(!env.arguments().unwrap().get(0).unwrap().is_provided());
    }

    // pylatexenc: test_error_dangling_missing_args_2 (`Test _` with `_` registered
    // as a specials taking a mandatory argument).
    #[test]
    fn specials_with_a_missing_mandatory_argument() {
        let language_for = |recovery: Recovery| {
            let mut package = Package::new("underscore");
            package.insert_specials(
                "_",
                CallableType::Specials,
                Arc::new(SpecialsSpec::new(args(&["m"]))),
            );
            with_recovery(recovery).with_provider(Arc::new(package)).unwrap()
        };

        let err = language_for(Recovery::Strict).parse("Test _").unwrap_err();
        assert!(err.to_string().contains("missing mandatory argument"), "{err}");

        let result = language_for(Recovery::Tolerant).parse("Test _").unwrap();
        check_tree_invariants(&result.tree);
        assert_eq!(result.diagnostics.len(), 1);
        assert_eq!(
            outline(result.tree.root().children()),
            ["0..5 chars(Test )", "5..6 Specials(_)"]
        );

        // The same input parses cleanly with a provided argument.
        let result = parse_ok_in(language_for, "Test _{sub}");
        let specials = result.tree.root().child(1).unwrap();
        assert_eq!(argument_chars(specials, 0), "sub");
    }

    // pylatexenc: test_invalid_environment_macros_0 (`\begin` with no name group).
    #[test]
    fn begin_without_a_name_group() {
        let input = r"\begin \item stuff\end{itemize}";
        let err = strict().parse(input).unwrap_err();
        assert!(err.to_string().contains("malformed ‘\\begin’"), "{err}");

        // Tolerant: the `\begin` trigger (post-space included) recovers as chars,
        // the rest parses normally, and the now-orphan `\end{itemize}` recovers as
        // chars over its consumed extent.
        let result = tolerant().parse(input).unwrap();
        check_tree_invariants(&result.tree);
        assert_eq!(result.diagnostics.len(), 2);
        assert_eq!(
            outline(result.tree.root().children()),
            [
                "0..7 chars(\\begin )",
                "7..13 Macro(item)",
                "13..18 chars(stuff)",
                "18..31 chars(\\end{itemize})",
            ]
        );
    }

    // pylatexenc: test_invalid_environment_macros_1 (`\end` with no name group,
    // inside an open environment body).
    #[test]
    fn end_without_a_name_group() {
        let input = r"\begin{itemize} \item stuff\end";
        assert!(strict().parse(input).is_err());

        let result = tolerant().parse(input).unwrap();
        check_tree_invariants(&result.tree);
        assert!(!result.diagnostics.is_empty());
        let env = result.tree.root().child(0).unwrap();
        assert_eq!(env.environment_name(), Some("itemize"));
    }

    // pylatexenc: test_invalid_environment_macros_2 (tolerant mode: bare `\begin`
    // and `\end` recover as chars). pylatexenc merges `stuff\n\end` into one chars
    // node; techy stages the recovered `\end` extent as its own chars node — runs
    // never merge across staged recovery nodes.
    #[test]
    fn bare_begin_and_end_recover_as_chars_in_tolerant_mode() {
        let input = "\\begin\n  \\item stuff\n\\end";
        let result = tolerant().parse(input).unwrap();
        check_tree_invariants(&result.tree);
        assert_eq!(result.diagnostics.len(), 2);
        assert_eq!(
            outline(result.tree.root().children()),
            [
                "0..9 chars(\\begin\n  )",
                "9..15 Macro(item)",
                "15..21 chars(stuff\n)",
                "21..25 chars(\\end)",
            ]
        );
        let item = result.tree.root().child(1).unwrap();
        assert!(!item.arguments().unwrap().get(0).unwrap().is_provided());
    }
}

// -------------------------------------------------------------------------------------
// Paragraph-break styles
// -------------------------------------------------------------------------------------

mod paragraph_breaks {
    use super::support::*;
    use std::sync::Arc;
    use techy::core::Language;
    use techy::error::Recovery;
    use techy::latexlike::{Latexlike, LatexlikeDriver, ParagraphBreakStyle};
    use techy::extract::content_as_chars;

    fn specials_style(recovery: Recovery) -> Language<Latexlike> {
        Language::new(
            LatexlikeDriver::new(recovery)
                .with_paragraph_break_style(ParagraphBreakStyle::Specials),
        )
        .with_provider(Arc::new(testdb()))
        .unwrap()
    }

    // The default style: a paragraph break is a whitespace chars node
    // (pylatexenc-legacy's shape), transparent to char extraction.
    #[test]
    fn paragraph_break_nodes_default_to_chars() {
        let result = parse_ok("one\n\ntwo");
        assert_eq!(
            outline(result.tree.root().children()),
            ["0..3 chars(one)", "3..5 chars(\n\n)", "5..8 chars(two)"]
        );
        assert_eq!(
            content_as_chars(result.tree.root().children()).unwrap(),
            "one\n\ntwo"
        );
    }

    // pylatexenc: test_get_latex_nodes_comments expects
    // `LatexSpecialsNode(specials_chars='\n\n')` — pylatexenc-modern's paragraph
    // shape, opted into via the driver's ParagraphBreakStyle::Specials flag.
    #[test]
    fn specials_style_emits_named_specials_nodes() {
        let result = parse_ok_in(specials_style, "one\n\ntwo");
        assert_eq!(
            outline(result.tree.root().children()),
            ["0..3 chars(one)", "3..5 Specials(\n\n)", "5..8 chars(two)"]
        );
        let break_node = result.tree.root().child(1).unwrap();
        assert_eq!(break_node.specials_name(), Some("\n\n"));

        // The name is the canonical "\n\n" vocabulary key even when the actual run
        // is longer; the span covers the actual run.
        let result = parse_ok_in(specials_style, "one\n \t\ntwo");
        let break_node = result.tree.root().child(1).unwrap();
        assert_eq!(break_node.specials_name(), Some("\n\n"));
        assert_eq!(break_node.span().range(), 3..7);
        assert_eq!(break_node.span().content(), "\n \t\n");
    }

    // The observable consequence beyond node shape: a Specials-formed paragraph
    // break is not text, so char extraction reports it instead of folding it in.
    #[test]
    fn specials_style_paragraph_breaks_are_opaque_to_char_extraction() {
        let result = parse_ok_in(specials_style, "one\n\ntwo");
        assert!(content_as_chars(result.tree.root().children()).is_err());
    }

    // The comments port's first document, under the specials style: the two
    // paragraph breaks become named specials nodes, everything else is unchanged.
    #[test]
    fn comments_document_with_specials_paragraph_breaks() {
        let input = "Hello % comment here\n  % more comments \n\nNew paragraph here.\n% comment at end";
        let result = parse_ok_in(specials_style, input);
        assert_eq!(
            outline(result.tree.root().children()),
            [
                "0..6 chars(Hello )",
                "6..23 comment( comment here)",
                "23..39 comment( more comments )",
                "39..41 Specials(\n\n)",
                "41..61 chars(New paragraph here.\n)",
                "61..77 comment( comment at end)",
            ]
        );
    }
}
