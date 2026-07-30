//! Extender walkthrough: I know LaTeX, I want to teach the parser MY macros and
//! environments, without learning parser internals.
//!
//! Written against the docs only: README.md quick start + docs/learn-by-example.md
//! (the `techy::guide` chapters). Every task parses real input and asserts on the
//! result.

use std::sync::Arc;

use techy::engine::Language;
use techy::error::Recovery;
use techy::latexlike::{
    argument_specs, CallableType, EnvironmentSpec, Latexlike, LatexlikeDriver, MacroSpec,
};
use techy::scopes::Package;
use techy::state::ParsingStateDelta;

fn main() {
    task1_custom_macro();
    task2_custom_environment();
    task3_reusable_package();
    task4_wrong_usage();
    task5_scoped_definition();
    println!("\nAll extender tasks passed.");
}

// ---------------------------------------------------------------------------
// Task 1: a custom macro \greet[greeting]{name} — one optional, one mandatory.
// ---------------------------------------------------------------------------
fn task1_custom_macro() {
    println!("== Task 1: custom macro \\greet[greeting]{{name}} ==");

    let mut package = Package::new("mymacros");
    package.insert(
        CallableType::Macro,
        "greet",
        // xparse-style argument codes: "o" = optional [..], "m" = mandatory {..}
        Arc::new(MacroSpec::new(argument_specs(["o", "m"]).unwrap())),
    );
    let language = Language::<Latexlike>::default()
        .with_provider(Arc::new(package))
        .unwrap();

    // --- optional argument present ---
    let result = language.parse(r"\greet[Hello]{World}, nice day").unwrap();
    let greet = result.tree.root().child(0).unwrap();
    assert_eq!(greet.macro_name(), Some("greet"));
    assert_eq!(greet.span_content(), r"\greet[Hello]{World}");

    let arguments = greet.arguments().unwrap();
    assert!(arguments.get(0).unwrap().is_provided());
    assert!(arguments.get(1).unwrap().is_provided());
    let greeting = greet.argument_content_nodes(0).unwrap();
    assert_eq!(greeting.source_text(), Some("Hello"));
    let name = greet.argument_content_nodes(1).unwrap();
    assert_eq!(name.source_text(), Some("World"));
    println!(
        "   \\greet[Hello]{{World}}: greeting = {:?}, name = {:?}",
        greeting.source_text(),
        name.source_text()
    );

    // --- optional argument absent ---
    let result = language.parse(r"\greet{World}").unwrap();
    let greet = result.tree.root().child(0).unwrap();
    let arguments = greet.arguments().unwrap();
    assert!(!arguments.get(0).unwrap().is_provided());
    assert!(arguments.get(1).unwrap().is_provided());
    println!(
        "   \\greet{{World}}: optional provided = {}, argument_content_nodes(0) = {:?}",
        arguments.get(0).unwrap().is_provided(),
        greet.argument_content_nodes(0).map(|nodes| nodes.source_text().map(str::to_owned)),
    );
    assert_eq!(
        greet.argument_content_nodes(1).unwrap().source_text(),
        Some("World")
    );
}

// ---------------------------------------------------------------------------
// Task 2: a custom environment \begin{theorem}[title]...\end{theorem}.
// ---------------------------------------------------------------------------
fn task2_custom_environment() {
    println!("== Task 2: custom environment {{theorem}} with optional title ==");

    let mut package = Package::new("mymacros");
    package.insert(
        CallableType::Environment,
        "theorem",
        Arc::new(EnvironmentSpec::new(argument_specs(["o"]).unwrap())),
    );
    let language = Language::<Latexlike>::default()
        .with_provider(Arc::new(package))
        .unwrap();

    let result = language
        .parse(r"\begin{theorem}[Main result] Every {x} is fine. \end{theorem}")
        .unwrap();
    let theorem = result.tree.root().child(0).unwrap();
    assert_eq!(theorem.environment_name(), Some("theorem"));

    // The [title] argument:
    let title = theorem.argument_content_nodes(0).unwrap();
    assert_eq!(title.source_text(), Some("Main result"));

    // The body nodes:
    let body = theorem.body().unwrap();
    let shapes: Vec<String> = body.iter().map(|node| node.summary()).collect();
    println!("   body shapes: {shapes:?}");
    assert_eq!(body.len(), 3);
    assert_eq!(body.get(0).unwrap().chars(), Some(" Every "));
    assert!(body.get(1).unwrap().is_group());
    assert_eq!(body.get(2).unwrap().chars(), Some(" is fine. "));
    assert_eq!(
        body.source_text(),
        Some(" Every {x} is fine. ")
    );
}

// ---------------------------------------------------------------------------
// Task 3: bundle definitions as a reusable package and activate it per parse.
// ---------------------------------------------------------------------------

/// My whole notation, built once, shared by every Language that wants it.
fn my_notation_package() -> Arc<Package<Latexlike>> {
    let mut package = Package::new("mynotation");
    package.insert(
        CallableType::Macro,
        "greet",
        Arc::new(MacroSpec::new(argument_specs(["o", "m"]).unwrap())),
    );
    package.insert(
        CallableType::Macro,
        "emph",
        Arc::new(MacroSpec::new(argument_specs(["m"]).unwrap())),
    );
    package.insert(
        CallableType::Macro,
        "cite",
        Arc::new(MacroSpec::new(argument_specs(["o", "m"]).unwrap())),
    );
    package.insert(
        CallableType::Environment,
        "theorem",
        Arc::new(EnvironmentSpec::new(argument_specs(["o"]).unwrap())),
    );
    Arc::new(package)
}

fn task3_reusable_package() {
    println!("== Task 3: reusable package, activated per language ==");

    // Activation is one call; the Arc makes the package shareable across languages.
    let notation = my_notation_package();
    let language = Language::<Latexlike>::default()
        .with_provider(notation.clone())
        .unwrap();

    let result = language
        .parse(r"\begin{theorem}[T1] \emph{all} of \cite{knuth} \end{theorem}")
        .unwrap();
    let theorem = result.tree.root().child(0).unwrap();
    assert_eq!(theorem.environment_name(), Some("theorem"));
    let body = theorem.body().unwrap();
    let names: Vec<Option<&str>> = body.iter().map(|node| node.macro_name()).collect();
    println!("   body macro names: {names:?}");
    assert!(names.contains(&Some("emph")));
    assert!(names.contains(&Some("cite")));

    // A second, independent language reuses the same Arc'd package.
    let another = Language::<Latexlike>::new(LatexlikeDriver::new(Recovery::Tolerant))
        .with_provider(notation)
        .unwrap();
    let result = another.parse(r"\greet{again}").unwrap();
    assert_eq!(result.tree.root().child(0).unwrap().macro_name(), Some("greet"));
}

// ---------------------------------------------------------------------------
// Task 4: what does MY user see when they use \greet wrongly?
// ---------------------------------------------------------------------------
fn task4_wrong_usage() {
    println!("== Task 4: wrong usage of \\greet ==");

    let make_language = |recovery| {
        Language::<Latexlike>::new(LatexlikeDriver::new(recovery))
            .with_provider(my_notation_package())
            .unwrap()
    };

    // (a) Missing mandatory argument at end of input, strict mode: parse aborts.
    let strict = make_language(Recovery::Strict);
    let err = strict.parse(r"\greet[Hi]").unwrap_err();
    println!("   strict Display: {err}");
    println!("   strict identifier: {}", err.identifier());
    println!("   strict render:\n{}", indent(&err.render(), 6));

    // (b) Same input, tolerant mode: parse completes, diagnostic is on the result.
    let tolerant = make_language(Recovery::Tolerant);
    let result = tolerant.parse(r"\greet[Hi]").unwrap();
    assert!(result.diagnostics.has_errors());
    for diagnostic in result.diagnostics.iter() {
        println!(
            "   tolerant diagnostic: severity={:?} identifier={} span={:?}",
            diagnostic.severity(),
            diagnostic.identifier(),
            diagnostic.span().range(),
        );
        println!("{}", indent(&diagnostic.render(), 6));
    }
    // What does the recovered tree look like?
    let shapes: Vec<String> =
        result.tree.root().children().iter().map(|node| node.summary()).collect();
    println!("   tolerant recovered tree: {shapes:?}");

    // (c) The LaTeX-faithful surprise: `\greet word` is NOT an error — the mandatory
    // argument falls back to the single expression `w` (like `\frac12` in TeX).
    let result = strict.parse(r"\greet word").unwrap();
    let greet = result.tree.root().child(0).unwrap();
    assert_eq!(
        greet.argument_content_nodes(1).unwrap().source_text(),
        Some("w")
    );
    println!("   \\greet word: mandatory argument = {:?} (single-expression fallback)",
        greet.argument_content_nodes(1).unwrap().source_text());

    // (d) An unknown macro (my user typo'd \gret): strict aborts with a
    // resolution error that names the command.
    let err = strict.parse(r"\gret{World}").unwrap_err();
    println!("   typo'd macro, strict: {err}");
}

// ---------------------------------------------------------------------------
// Task 5 (stretch): a definition only active inside a certain environment.
// ---------------------------------------------------------------------------
fn task5_scoped_definition() {
    println!("== Task 5: \\qed defined only inside {{proof}} ==");

    // The inner package holds the definitions local to the environment body.
    let mut inner = Package::new("proof-locals");
    inner.insert(
        CallableType::Macro,
        "qed",
        Arc::new(MacroSpec::new(Vec::new())),
    );

    // The environment's body delta pushes that package for the body's extent only.
    let mut package = Package::new("mymacros");
    package.insert(
        CallableType::Environment,
        "proof",
        Arc::new(
            EnvironmentSpec::new(Vec::new())
                .with_body_delta(ParsingStateDelta::new().push_provider(Arc::new(inner))),
        ),
    );
    let language = Language::<Latexlike>::default()
        .with_provider(Arc::new(package))
        .unwrap();

    // Inside the environment: \qed resolves.
    let result = language.parse(r"\begin{proof}done \qed\end{proof}").unwrap();
    let proof = result.tree.root().child(0).unwrap();
    let body = proof.body().unwrap();
    let qed = body.get(1).unwrap();
    assert_eq!(qed.macro_name(), Some("qed"));
    println!("   inside {{proof}}: \\qed resolved to macro {:?}", qed.macro_name());

    // Outside: \qed is unresolvable.
    let err = language.parse(r"done \qed").unwrap_err();
    println!("   outside {{proof}}: {err}");
}

// Small display helper for multi-line renders.
fn indent(text: &str, by: usize) -> String {
    let pad = " ".repeat(by);
    text.lines().map(|line| format!("{pad}{line}\n")).collect()
}
