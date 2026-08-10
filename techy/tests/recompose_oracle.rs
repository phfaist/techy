//! The **reemit oracle** (recompose acceptance suite): parse an input with the
//! latexlike preset and assert that the preset source recomposer
//! ([`source_recomposer`](techy::latexlike::source_recomposer)) reproduces the
//! input **byte-for-byte from recorded payload alone** — the in-crate proof
//! that the parse records everything its recomposition needs (payload
//! completeness with no span crutch: the recomposer never resolves span
//! content; on these trees every emitted byte comes from node payload).
//!
//! Three matrices, all through the **public API only** (the acceptance-suite
//! principle — anything the oracle cannot reach is an API gap):
//!
//! - **strict**: well-formed inputs across the representative constructs
//!   (macros with post-space, arguments incl. optional/absent/single-token,
//!   environments incl. recorded spacing pathologies, verbatim, specials,
//!   paragraph breaks in both driver styles, groups, math, comments);
//! - **tolerant**: recovered shapes reemit exactly what was recorded — for
//!   every recovery that keeps the consumed bytes in the tree this is again
//!   `reemit == input`; the one recorded-less-than-consumed recovery (the
//!   malformed environment terminator, whose `\end` is consumed alone and
//!   recorded nowhere — the S5 flag) is pinned by its own test below;
//! - **multi-source**: `\input` trees (the S6 surface) — the root reemits the
//!   includer's bytes (the `Attached` slot is skipped by the default `Concat`
//!   scope), and the attached body alone reemits the included source's bytes.

use std::sync::Arc;

use techy::core::node::NodeRef;
use techy::core::{
    CommandOverrides, CommandRule, Language, ParsingState, ParsingStateDelta,
    StdDescentGuardInit, TokenRulesOverrides,
};
use techy::error::Recovery;
use techy::latexlike::minidefs::minilatex_package;
use techy::latexlike::{
    argument_specs, input_macro_spec, source_recomposer, BodyMarker, CallableType,
    EnvironmentSpec, Latexlike, LatexlikeDriver, MacroSpec, ParagraphBreakStyle,
    SourceRecomposeError, SourceRecomposer, VerbatimBehavior,
};
use techy::recompose::{recompose, Recompose, RecomposeContext, RecomposeError, Recomposer};
use techy::source::MapResolver;

use techy::core::specs::Package;

/// The oracle's test vocabulary: `\emph{m}`, `\frac{m}{m}`, `\o{[}{{}`,
/// `\st{s}{m}`, `\verb{v}`, environments `itemize`/`fig([)`/`A`/`B` +
/// `verbatim`.
fn testdb() -> Package<Latexlike> {
    let mut package = Package::new("oracle");
    package.insert(
        CallableType::Macro,
        "emph",
        MacroSpec::new(argument_specs(&["m"]).unwrap()),
    );
    package.insert(
        CallableType::Macro,
        "frac",
        MacroSpec::new(argument_specs(&["m", "m"]).unwrap()),
    );
    package.insert(
        CallableType::Macro,
        "o",
        MacroSpec::new(argument_specs(&["[", "{"]).unwrap()),
    );
    package.insert(
        CallableType::Macro,
        "st",
        MacroSpec::new(argument_specs(&["s", "m"]).unwrap()),
    );
    package.insert(
        CallableType::Macro,
        "verb",
        MacroSpec::new(argument_specs(&["v"]).unwrap()),
    );
    package.insert(CallableType::Environment, "itemize", EnvironmentSpec::new(vec![]));
    package.insert(
        CallableType::Environment,
        "fig",
        EnvironmentSpec::new(argument_specs(&["["]).unwrap()),
    );
    package.insert(CallableType::Environment, "A", EnvironmentSpec::new(vec![]));
    package.insert(CallableType::Environment, "B", EnvironmentSpec::new(vec![]));
    package.insert(
        CallableType::Environment,
        "verbatim",
        EnvironmentSpec::from_behavior(Arc::new(VerbatimBehavior::default())),
    );
    package
}

fn language(recovery: Recovery) -> Language<Latexlike> {
    // minilatex supplies the typography specials rows (`~`, the ligatures) — the
    // seed no longer ships them; testdb stays innermost so its `itemize`/`emph`
    // definitions win.
    Language::new(
        LatexlikeDriver::new(recovery),
        ParsingState::lang_initial_with_packages([minilatex_package(), testdb()]),
    )
}

/// Reemit `input` through the source recomposer after a parse under
/// `recovery` (`expect_diagnostics` gates the diagnostics assertion).
fn reemit(recovery: Recovery, expect_diagnostics: bool, input: &str) -> String {
    let result = language(recovery).parse(input).expect("oracle inputs parse");
    if expect_diagnostics {
        assert!(
            !result.diagnostics.is_empty(),
            "expected a diagnosed recovery for {input:?}"
        );
    } else {
        assert!(
            result.diagnostics.is_empty(),
            "unexpected diagnostics for {input:?}: {:?}",
            result.diagnostics
        );
    }
    recompose(&result.tree, (), &mut source_recomposer()).unwrap()
}

/// The strict matrix's per-input assertion: a clean parse reemits its input
/// byte-for-byte.
fn assert_reemit(input: &str) {
    assert_eq!(reemit(Recovery::Strict, false, input), input, "strict reemit of {input:?}");
}

/// The tolerant matrix's per-input assertion: a diagnosed recovery still
/// reemits its input byte-for-byte (every consumed byte was recorded).
fn assert_reemit_tolerant(input: &str) {
    assert_eq!(
        reemit(Recovery::Tolerant, true, input),
        input,
        "tolerant reemit of {input:?}"
    );
}

// --- the strict matrix ------------------------------------------------------------------

#[test]
fn strict_macros_with_and_without_post_space() {
    assert_reemit("\\emph  x");
    assert_reemit("\\emph{x}");
    assert_reemit("pre \\emph x post");
    assert_reemit("\\emph {spaced}");
}

#[test]
fn strict_a_second_escape_character_reemits_as_written() {
    // A language with an additional `@` command rule: the payload records
    // whichever escape fired, and reemission reproduces it.
    let seed = ParsingState::<Latexlike>::lang_initial_with_packages([testdb()]);
    let mut commands = seed.rules().commands.rules.clone();
    commands.push(Arc::new(CommandRule {
        escape_char: '@',
        name_chars: "abcdefghijklmnopqrstuvwxyz".into(),
    }));
    let seed = seed
        .derived(&ParsingStateDelta::new().rules(TokenRulesOverrides {
            commands: CommandOverrides { rules: Some(commands), ..CommandOverrides::default() },
            ..TokenRulesOverrides::default()
        }))
        .unwrap();
    let language = Language::new(LatexlikeDriver::new(Recovery::Strict), seed);
    let input = "@emph x and \\emph y";
    let result = language.parse(input).unwrap();
    assert!(result.diagnostics.is_empty());
    let out = recompose(&result.tree, (), &mut source_recomposer()).unwrap();
    assert_eq!(out, input);
}

#[test]
fn strict_argument_shapes() {
    assert_reemit("\\frac 1 2");
    assert_reemit("\\frac{a}{b}");
    assert_reemit("\\o[x]{y}");
    assert_reemit("\\o{y} after"); // absent optional
    assert_reemit("\\o [x] {y}"); // inter-argument noise
    assert_reemit("\\st*{x}"); // provided star marker
    assert_reemit("\\st y"); // absent star
}

#[test]
fn strict_environments() {
    assert_reemit("\\begin{itemize}x\\end{itemize}");
    assert_reemit("\\begin {itemize}x y\\end {itemize}"); // recorded spacing
    assert_reemit("\\begin{itemize}a\\begin{itemize}b\\end{itemize}c\\end{itemize}");
    assert_reemit("\\begin{fig}[t]body\\end{fig}"); // declared environment argument
    assert_reemit("\\begin{fig}body\\end{fig}"); // …absent
}

#[test]
fn strict_verbatim() {
    assert_reemit("\\begin{verbatim}\na % b {\n\\end{verbatim}");
    assert_reemit("\\verb|a b|");
    assert_reemit("\\verb+{x}+ y");
}

#[test]
fn strict_specials() {
    // minilatex's typography specials: ligatures and the tie (the oracle language
    // loads minilatex — the seed alone would parse these as plain chars, which
    // would reemit trivially and prove nothing about specials recomposition).
    assert_reemit("a---b ~ c");
    assert_reemit("``quoted'' -- dashed");
}

#[test]
fn strict_paragraph_breaks_in_both_styles() {
    // Chars style (the default): the break is a whitespace chars node.
    assert_reemit("a\n\nb");
    assert_reemit("a\n \t\n  b");

    // Specials style: a callable named by the actual whitespace run.
    let language = Language::new(
        LatexlikeDriver::new(Recovery::Strict)
            .with_paragraph_break_style(ParagraphBreakStyle::Specials),
        ParsingState::lang_initial_with_packages([testdb()]),
    );
    for input in ["a\n\nb", "a\n \t\n  b", "one\n\ntwo\n\n\nthree"] {
        let result = language.parse(input).unwrap();
        assert!(result.diagnostics.is_empty());
        let out = recompose(&result.tree, (), &mut source_recomposer()).unwrap();
        assert_eq!(out, input, "specials-style reemit of {input:?}");
    }
}

#[test]
fn strict_groups_and_math() {
    assert_reemit("{nested {groups} here}");
    assert_reemit("inline $x+y$ math");
    assert_reemit("$$display$$ and \\(i\\) and \\[d\\]");
    assert_reemit("$m$ {g {h}} \\[x^2\\]");
}

#[test]
fn strict_comments() {
    assert_reemit("a% note\n  b"); // newline + indentation post-space
    assert_reemit("a% end"); // end-of-input comment, no post-space
    assert_reemit("% leading\nrest");
}

#[test]
fn strict_kitchen_sink() {
    assert_reemit(concat!(
        "Intro with \\emph  {styled} text and \\frac 1 2 math $a+b$.\n",
        "% a comment\n",
        "\\begin {itemize}\n",
        "item one ~ two --- three\n",
        "\\o[opt]{arg}\n",
        "\\begin{verbatim}\nraw % {\n\\end{verbatim}\n",
        "\\end {itemize}\n",
        "\n",
        "New paragraph \\[d^2\\] and \\verb|v {|.",
    ));
}

// --- the tolerant matrix ----------------------------------------------------------------

#[test]
fn tolerant_unterminated_environment_reemits_the_recorded_shape() {
    // The end side is empty; write_end emits nothing — and the input had no
    // terminator, so reemit == input.
    assert_reemit_tolerant("\\begin{itemize}x");
}

#[test]
fn tolerant_terminator_mismatch_reemits_every_byte() {
    // The inner B closes without consuming `\end{A}`; the outer A consumes
    // its own terminator — every byte is in the tree.
    assert_reemit_tolerant("\\begin{A}x\\begin{B}y\\end{A}");
}

#[test]
fn tolerant_unclosed_group_reemits_with_the_empty_recorded_close() {
    assert_reemit_tolerant("{x");
    assert_reemit_tolerant("a{b{c}");
}

#[test]
fn tolerant_stray_close_reemits_the_recovered_chars() {
    assert_reemit_tolerant("x}y");
}

#[test]
fn tolerant_orphan_end_preserves_content() {
    assert_reemit_tolerant("a\\end{itemize}b");
}

#[test]
fn tolerant_forbidden_char_in_math_reemits() {
    // Inside display math the removed `$` delimiter is a forbidden character:
    // diagnosed, consumed as chars — the byte stays.
    assert_reemit_tolerant("\\[a $ b\\]");
}

#[test]
fn tolerant_unknown_macro_reemits_the_recovered_spelling() {
    assert_reemit_tolerant("a\\nope b");
}

/// The **malformed-terminator pin** (the S5 flag, D-plan-10): `\end` not
/// followed by its rigid name group is consumed **alone** (with its
/// syntactic post-space), diagnosed, and recorded nowhere — neither as
/// payload facts (the end side stays empty) nor as a node. Payload-only
/// reemission therefore elides exactly the consumed `\end` spelling; the
/// case is excluded from the equality matrix above and pinned here.
#[test]
fn tolerant_malformed_terminator_elides_the_consumed_end_spelling() {
    let input = "\\begin{A}x\\end y";
    // The consumed-alone spelling is the command plus its post-space:
    let expected = "\\begin{A}xy";
    assert_eq!(reemit(Recovery::Tolerant, true, input), expected);
}

// --- the multi-source matrix (rides the S6 `\input` surface) ----------------------------

fn input_language(entries: &[(&str, &str)]) -> Language<Latexlike> {
    let mut package = testdb();
    package.insert(
        CallableType::Macro,
        "input",
        input_macro_spec(false, BodyMarker::not_body()),
    );
    let mut resolver = MapResolver::new();
    for (reference, content) in entries {
        resolver.insert(*reference, *content);
    }
    Language::new(
        LatexlikeDriver::new(Recovery::Strict)
            .with_source_resolver(resolver.with_reference_as_origin()),
        ParsingState::lang_initial_with_packages([package]),
    )
    // Explicit guard: nested inclusions stack enough construct levels to trip
    // the unconfigured default's half-budget warning in debug builds.
    .with_descent_guard_init(StdDescentGuardInit::depth_limit(64))
}

#[test]
fn multi_source_root_reemits_the_includer() {
    // The attached content is skipped by the default scope: the `\input`
    // invocation text IS its recomposition.
    let language = input_language(&[("sub.tex", "SUB {and} more")]);
    let input = "a\\input{sub.tex} b";
    let result = language.parse(input).unwrap();
    assert!(result.diagnostics.is_empty());
    let out = recompose(&result.tree, (), &mut source_recomposer()).unwrap();
    assert_eq!(out, input);
}

/// A recomposer that grabs the attached body's reemission of every `\input`
/// node it passes (via the slot op, **re-entering itself** — the reentrant
/// self-passing shape, so nested inclusions are grabbed too) while delegating
/// the whole fold to the source recomposer.
struct GrabAttached {
    grabbed: Vec<String>,
    inner: SourceRecomposer<Latexlike>,
}

/// The reentrant error pattern (the transform suite's `OpError` shape): one
/// boxed self-referential conversion makes the context ops `?`-propagatable.
#[derive(Debug)]
#[allow(dead_code)] // the payloads exist for the unwrap() failure rendering
enum GrabError {
    Op(Box<RecomposeError<GrabError>>),
    Source(SourceRecomposeError),
}

impl From<RecomposeError<GrabError>> for GrabError {
    fn from(error: RecomposeError<GrabError>) -> GrabError {
        GrabError::Op(Box::new(error))
    }
}

impl<A> Recomposer<Latexlike, A> for GrabAttached {
    type State = ();
    type Piece = String;
    type Error = GrabError;

    fn recompose_node(
        &mut self,
        node: NodeRef<'_, Latexlike, A>,
        state: &(),
        cx: &mut RecomposeContext<'_, Latexlike, A>,
    ) -> Result<Recompose<String, ()>, GrabError> {
        if node.name() == Some("input") {
            let piece = cx.recompose_slot_content_named(node, "attached", state, self)?;
            self.grabbed.push(piece);
        }
        self.inner.recompose_node(node, state, cx).map_err(GrabError::Source)
    }
}

#[test]
fn multi_source_attached_bodies_reemit_their_own_sources() {
    // Nested inclusion: sub.tex includes subsub.tex. Each attached body
    // reemits its own source's bytes (per-source payload completeness).
    let language = input_language(&[
        ("sub.tex", "SUB \\input{subsub.tex} TAIL"),
        ("subsub.tex", "DEEP {d}"),
    ]);
    let input = "a\\input{sub.tex}b";
    let result = language.parse(input).unwrap();
    assert!(result.diagnostics.is_empty(), "unexpected: {:?}", result.diagnostics);

    let mut grabber = GrabAttached { grabbed: Vec::new(), inner: source_recomposer() };
    let out = recompose(&result.tree, (), &mut grabber).unwrap();
    // The root still reemits the includer.
    assert_eq!(out, input);
    // The outer grab's own sub-fold reaches the nested \input first
    // (reentrant self-passing), so the innermost body lands first; each
    // attached body reemits its own source exactly.
    assert_eq!(
        grabber.grabbed,
        ["DEEP {d}".to_string(), "SUB \\input{subsub.tex} TAIL".to_string()]
    );
}

#[test]
fn multi_source_empty_included_file() {
    let language = input_language(&[("empty.tex", "")]);
    let input = "a\\input{empty.tex}b";
    let result = language.parse(input).unwrap();
    assert!(result.diagnostics.is_empty());

    let mut grabber = GrabAttached { grabbed: Vec::new(), inner: source_recomposer() };
    let out = recompose(&result.tree, (), &mut grabber).unwrap();
    assert_eq!(out, input);
    assert_eq!(grabber.grabbed, [String::new()]);
}
