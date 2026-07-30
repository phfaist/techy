//! Phase 1 "document consumer" walkthrough of the techy public API.
//!
//! Role-play: a first-time user who wants to parse LaTeX documents and inspect the
//! result. One binary, sections per task. Everything here uses only the public API,
//! learned from README.md and the docs/ guide pages.

use std::sync::Arc;

use techy::engine::Language;
use techy::error::{format_position, Recovery};
use techy::latexlike::{
    argument_specs, CallableType, EnvironmentSpec, Latexlike, LatexlikeDriver, MacroSpec,
};
use techy::node::extract;
use techy::node::{NodeKind, NodeRef};
use techy::scopes::Package;

// ---------------------------------------------------------------------------------------
// Shared setup: a language with a handful of everyday LaTeX definitions.
//
// (Friction note: there is no standard macro database yet — `\emph`, `\cite`, `\item`,
// `itemize` all have to be registered by hand before anything realistic parses.)
// ---------------------------------------------------------------------------------------

fn build_language(recovery: Recovery) -> Language<Latexlike> {
    let mut package = Package::new("walkthrough");
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
        CallableType::Macro,
        "item",
        Arc::new(MacroSpec::new(argument_specs(["o"]).unwrap())),
    );
    package.insert(
        CallableType::Environment,
        "itemize",
        Arc::new(EnvironmentSpec::new(Vec::new())),
    );
    Language::new(LatexlikeDriver::new(recovery))
        .with_provider(Arc::new(package))
        .unwrap()
}

/// The realistic snippet for tasks 1-5: macros with mandatory+optional arguments, an
/// itemize environment, inline math, a `%` comment, and a `{...}` group.
const SRC: &str = "\
Intro \\emph{word} and \\cite[p.~7]{knuth84}.
% a comment
{grouped text} and $x + y$ math.
\\begin{itemize}
\\item First one
\\item Second \\emph{nested}
\\end{itemize}";

/// The malformed snippet for task 6: unknown command, unclosed group, missing `\end`,
/// unterminated math.
const BAD: &str = "\
Good text \\unknowncmd more
{unclosed group
\\begin{itemize} item body
$ math never closed";

fn main() {
    let language = build_language(Recovery::Strict);

    // -----------------------------------------------------------------------------------
    // Task 1: parse a realistic snippet with the latexlike preset.
    // -----------------------------------------------------------------------------------
    println!("== Task 1: parse ==");
    let result = language.parse(SRC).expect("well-formed snippet should parse");
    let root = result.tree.root();
    println!(
        "parsed {} top-level nodes, {} total nodes, {} diagnostics",
        root.child_count(),
        result.tree.node_count(),
        result.diagnostics.len()
    );
    assert!(result.diagnostics.is_empty());

    // Sanity: each requested construct is present.
    let all: Vec<NodeRef<'_, Latexlike>> = root.descendants().collect();
    assert!(all.iter().any(|n| n.macro_name() == Some("emph")));
    assert!(all.iter().any(|n| n.macro_name() == Some("cite")));
    assert!(all.iter().any(|n| n.environment_name() == Some("itemize")));
    assert!(all.iter().any(|n| n.is_math_group()));
    assert!(all.iter().any(|n| n.is_comment()));
    assert!(all
        .iter()
        .any(|n| n.is_group() && !n.is_math_group() && n.group_delimiters() == Some(("{", "}"))));

    // -----------------------------------------------------------------------------------
    // Task 2: walk the AST and print a tree dump (kind + source text per node).
    // -----------------------------------------------------------------------------------
    println!("\n== Task 2: tree dump ==");
    dump(root, 0);

    // -----------------------------------------------------------------------------------
    // Task 3: extract the plain text content.
    // -----------------------------------------------------------------------------------
    println!("\n== Task 3: plain text ==");
    // `extract::content_as_chars` fails honestly on callables, so it cannot be used on a
    // whole document containing macros. The document-level answer is the descendant walk
    // over chars nodes. (Note what this drops: comment text -- fine -- but also the
    // `~` specials and any ligature specials, which have no chars content.)
    let plain: String = root.descendants().filter_map(|n| n.chars()).collect();
    println!("{plain:?}");
    assert!(plain.contains("Intro "));
    assert!(plain.contains("word")); // argument content is included
    assert!(plain.contains("x + y")); // math body included
    assert!(!plain.contains("a comment")); // comments excluded
    assert!(!plain.contains("itemize")); // markup excluded

    // -----------------------------------------------------------------------------------
    // Task 4: find all `\emph` invocations, extract each first mandatory argument.
    // -----------------------------------------------------------------------------------
    println!("\n== Task 4: \\emph arguments ==");
    let emphs: Vec<NodeRef<'_, Latexlike>> = root
        .descendants()
        .filter(|n| n.macro_name() == Some("emph"))
        .collect();
    assert_eq!(emphs.len(), 2);
    let mut emph_texts = Vec::new();
    for node in &emphs {
        // `\emph` has one argument spec (index 0, the mandatory one). The *content*
        // view sees through the braces.
        let arg = node.argument_content_nodes(0).expect("emph has argument 0");
        let text = extract::content_as_chars(arg).expect("emph argument is plain text");
        println!("  {} -> {:?}", node.span_content(), text);
        emph_texts.push(text.into_owned());
    }
    assert_eq!(emph_texts, ["word", "nested"]);

    // -----------------------------------------------------------------------------------
    // Task 5: source span -> line/column for one node.
    // -----------------------------------------------------------------------------------
    println!("\n== Task 5: line/column ==");
    let nested_emph = emphs[1];
    let span = nested_emph.span();
    let source = span.source();
    let mut line_index = source.line_index();
    let (line, col) = line_index
        .line_col(span.start())
        .expect("offset within scan limit");
    println!(
        "node `{}` at bytes {:?} = line {}, col {}",
        nested_emph.span_content(),
        span.range(),
        line,
        col
    );
    // `\item Second ` is 13 chars into line 6 -> 1-based column 14.
    assert_eq!((line, col), (6, 14));
    // The one-shot convenience from the error module formats the same position:
    println!("format_position: {}", format_position(span));

    // -----------------------------------------------------------------------------------
    // Task 6: malformed input -- strict vs tolerant, and displaying diagnostics.
    // -----------------------------------------------------------------------------------
    println!("\n== Task 6: malformed input ==");

    // Strict (the default policy): the parse aborts on the first problem; `parse`
    // returns Err. That is how a user detects *failure*.
    let strict = build_language(Recovery::Strict);
    let err = strict.parse(BAD).expect_err("strict parse must abort");
    println!("--- strict: Err(ParseError) ---");
    println!("display: {err}");
    println!("render:\n{}", err.render());

    // Tolerant: the parse completes with a best-effort tree; problems are recorded as
    // diagnostics on the Ok result. "Succeeded with warnings" vs "recovered errors" vs
    // "clean" is read off the Diagnostics collection.
    let tolerant = build_language(Recovery::Tolerant);
    let result = tolerant.parse(BAD).expect("tolerant parse completes");
    println!(
        "--- tolerant: Ok with {} diagnostics ---",
        result.diagnostics.len()
    );
    assert!(!result.diagnostics.is_empty());
    for diagnostic in result.diagnostics.iter() {
        println!(
            "  [{}] {} {}",
            diagnostic.severity(),
            diagnostic.message(),
            format_position(diagnostic.span()),
        );
    }
    // The collection-level pretty report (shared line index, provenance chains):
    println!("render_all:\n{}", result.diagnostics.render_all());

    // How a user distinguishes the outcomes:
    let verdict = match tolerant.parse(BAD) {
        Err(_) => "hard failure (no tree)",
        Ok(r) if r.diagnostics.has_errors() => "parsed with recovered errors",
        Ok(r) if !r.diagnostics.is_empty() => "parsed with warnings/notes",
        Ok(_) => "clean parse",
    };
    println!("verdict: {verdict}");
    assert_eq!(verdict, "parsed with recovered errors");

    // The recovered tree is still inspectable:
    let shapes: Vec<String> = tolerant
        .parse(BAD)
        .unwrap()
        .tree
        .root()
        .children()
        .iter()
        .map(|n| n.summary())
        .collect();
    println!("recovered top-level shapes: {shapes:?}");

    println!("\nall tasks passed");
}

// ---------------------------------------------------------------------------------------
// Task 2 helper: recursive dump using only public accessors.
// ---------------------------------------------------------------------------------------

/// A short structural label for a node. (Friction note: there is no public
/// `NodeKind::name()`-style accessor; a consumer writes this match themselves or leans
/// on `summary()`, whose exact format is a debugging aid.)
fn kind_label(node: NodeRef<'_, Latexlike>) -> &'static str {
    match node.kind() {
        NodeKind::Chars { .. } => "Chars",
        NodeKind::Group(_) => "Group",
        NodeKind::Callable(_) => "Callable",
        NodeKind::Comment { .. } => "Comment",
        NodeKind::List { .. } => "List",
    }
}

fn dump(node: NodeRef<'_, Latexlike>, depth: usize) {
    let mut text: String = node.span_content().replace('\n', "\\n");
    if text.chars().count() > 46 {
        text = format!("{}…", text.chars().take(45).collect::<String>());
    }
    let name = node.name().map(|n| format!(" `{n}`")).unwrap_or_default();
    println!(
        "{:indent$}{}{} [{}..{}] {:?}",
        "",
        kind_label(node),
        name,
        node.span().start(),
        node.span().end(),
        text,
        indent = depth * 2
    );
    for child in node.children().iter() {
        dump(child, depth + 1);
    }
}
