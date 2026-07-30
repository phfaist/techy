//! Task 3 — multi-source inclusion and provenance.
//!
//! techy is no_std and does no I/O; `\input`-like lookup goes through a
//! `SourceResolver`. The latexlike preset ships no `\input` construct that triggers
//! resolution during a parse, so the embedder drives inclusion itself:
//! register `\input` as an ordinary one-argument macro, find its invocations in the
//! parsed tree, and use `Language::resolve_source` + `Language::parse_source` to pull
//! in and parse the referenced content. Provenance (`SourceProvenance::Resolved`) is
//! stamped by the library per include site; we then walk a node's provenance chain and
//! render an "included from X which was included from Y" trail.

use std::sync::Arc;

use techy::engine::{Language, ParseResult};
use techy::error::{format_position, Recovery};
use techy::latexlike::{argument_specs, CallableType, Latexlike, LatexlikeDriver, MacroSpec};
use techy::node::extract;
use techy::scopes::Package;
use techy::source::{MapResolver, Source, SourceProvenance, SourceSpan};

const MAIN_TEX: &str = "\
Preamble text.
\\input{chapter1.tex}
Postamble text.
";

const CHAPTER1_TEX: &str = "\
Chapter one begins.
\\input{section1.tex}
Chapter one ends.
";

const SECTION1_TEX: &str = "\
Section text with $x+y$ math.
Then \\oops appears here.
";

/// The `\input`-aware language: `\input{...}` parses as a plain macro; inclusion is
/// driven by the embedder below.
fn make_language(recovery: Recovery) -> Language<Latexlike> {
    let mut package = Package::new("include-demo");
    package.insert(
        CallableType::Macro,
        "input",
        Arc::new(MacroSpec::new(argument_specs(["m"]).unwrap())),
    );

    let mut resolver = MapResolver::new();
    resolver.insert("chapter1.tex", CHAPTER1_TEX);
    resolver.insert("section1.tex", SECTION1_TEX);
    // Label each resolved source's origin with its reference string, so positions
    // render as `[chapter1.tex]` etc.
    let resolver = resolver.with_reference_as_origin();

    Language::new(LatexlikeDriver::new(recovery))
        .with_provider(Arc::new(package))
        .unwrap()
        .with_resolver(resolver)
}

/// Recursively expand `\input` references: parse, collect (reference, sub-result)
/// pairs depth-first. This loop is embedder code — the library provides resolution
/// and provenance stamping, not the traversal.
fn expand_includes(
    language: &Language<Latexlike>,
    result: &ParseResult<Latexlike>,
    out: &mut Vec<(String, ParseResult<Latexlike>)>,
) {
    let inputs: Vec<(String, SourceSpan)> = result
        .tree
        .root()
        .descendants()
        .filter(|node| node.macro_name() == Some("input"))
        .map(|node| {
            // content_as_chars returns Cow<'_, str> (zero-copy when span-backed).
            let reference = extract::content_as_chars(node.argument_content_nodes(0).unwrap())
                .unwrap()
                .into_owned();
            (reference, node.span().clone())
        })
        .collect();
    for (reference, triggered_at) in inputs {
        let source = language.resolve_source(&reference, &triggered_at).unwrap();
        let sub = language.parse_source(source).unwrap();
        expand_includes(language, &sub, out);
        out.push((reference, sub));
    }
}

/// Render a source's include trail: "included from X …, which was included from Y …".
/// The provenance chain yields this source's provenance first, then each triggering
/// source's provenance, ending with `Primary`.
fn render_include_trail(source: &Arc<Source>) -> String {
    let mut parts: Vec<String> = Vec::new();
    for provenance in source.provenance_chain() {
        match provenance {
            SourceProvenance::Resolved {
                reference,
                triggered_at,
            } => {
                let connective = if parts.is_empty() {
                    "included as"
                } else {
                    "which was included as"
                };
                parts.push(format!(
                    "{connective} ‘{reference}’ {}",
                    format_position(triggered_at)
                ));
            }
            SourceProvenance::Synthesized {
                description,
                triggered_at,
            } => {
                parts.push(format!(
                    "synthesized ({description}) {}",
                    format_position(triggered_at)
                ));
            }
            SourceProvenance::Primary => parts.push("from the primary document".into()),
            #[allow(unreachable_patterns)]
            _ => {}
        }
    }
    parts.join(",\n  ")
}

fn main() {
    let language = make_language(Recovery::Tolerant);

    // Mint the main source by hand so it carries an origin label too.
    let main_source = Arc::new(
        Source::new(MAIN_TEX).with_origin(Some("main.tex".to_string())),
    );
    let main_result = language.parse_source(Arc::clone(&main_source)).unwrap();

    let mut included = Vec::new();
    expand_includes(&language, &main_result, &mut included);

    println!("=== include expansion (depth-first) ===");
    for (reference, sub) in &included {
        println!(
            "{reference}: {} nodes, {} diagnostics",
            sub.tree.node_count(),
            sub.diagnostics.len()
        );
    }

    // -- Provenance walk from a node of the innermost file ----------------------
    let (section_reference, section_result) = &included[0];
    assert_eq!(section_reference, "section1.tex");

    let math = section_result
        .tree
        .root()
        .descendants()
        .find(|node| node.is_math_group())
        .expect("math group in section1");
    println!("\n=== provenance of node ‘{}’ ===", math.span_content());
    let trail = render_include_trail(math.span().source());
    println!("{trail}");

    // -- The library's own rendering: a diagnostic inside the included file -----
    // `\oops` is unresolvable; in tolerant mode it lands in the diagnostics.
    println!("\n=== library-rendered diagnostic from inside the inclusion ===");
    let rendered = section_result.diagnostics.render_all();
    println!("{rendered}");

    // Strict mode: the abort error also carries and renders the include chain.
    // parse_source takes the Arc<Source> — reuse the same resolved source under a
    // second, strict language (sources and results are language-independent data).
    let strict = make_language(Recovery::Strict);
    let trigger = main_result
        .tree
        .root()
        .descendants()
        .find(|node| node.macro_name() == Some("input"))
        .unwrap()
        .span()
        .clone();
    let chapter_source = strict.resolve_source("chapter1.tex", &trigger).unwrap();
    let chapter_result = strict.parse_source(chapter_source).unwrap();
    let section_trigger = chapter_result
        .tree
        .root()
        .descendants()
        .find(|node| node.macro_name() == Some("input"))
        .unwrap()
        .span()
        .clone();
    let section_source = strict.resolve_source("section1.tex", &section_trigger).unwrap();
    let err = strict.parse_source(section_source).unwrap_err();
    println!("=== strict-mode ParseError::render() ===");
    println!("{}", err.render());

    // -- Assertions ------------------------------------------------------------
    // Two files expanded, depth-first: section1 first, then chapter1.
    assert_eq!(included.len(), 2);
    assert_eq!(included[1].0, "chapter1.tex");

    // The math node's source is the resolved section1 source, whose origin label is
    // the reference (MapResolver::with_reference_as_origin).
    let source = math.span().source();
    assert_eq!(
        source.origin().as_deref(),
        Some("section1.tex"),
        "origin label"
    );

    // The provenance chain walks section1 -> chapter1 -> primary.
    let kinds: Vec<&str> = source
        .provenance_chain()
        .map(|p| match p {
            SourceProvenance::Primary => "primary",
            SourceProvenance::Resolved { .. } => "resolved",
            SourceProvenance::Synthesized { .. } => "synthesized",
            #[allow(unreachable_patterns)]
            _ => "?",
        })
        .collect();
    assert_eq!(kinds, ["resolved", "resolved", "primary"]);

    let references: Vec<&str> = source
        .provenance_chain()
        .filter_map(|p| match p {
            SourceProvenance::Resolved { reference, .. } => Some(reference.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(references, ["section1.tex", "chapter1.tex"]);

    // Our hand-rolled trail names both include sites with line/col + origin.
    assert!(trail.contains("‘section1.tex’"));
    assert!(trail.contains("‘chapter1.tex’"));
    assert!(trail.contains("[main.tex]"));

    // The library's diagnostic rendering includes the include chain on its own.
    assert!(rendered.contains("cannot resolve command"));
    assert!(rendered.contains("section1.tex"));
    assert!(rendered.contains("chapter1.tex"));
    let strict_rendered = err.render();
    assert!(strict_rendered.contains("included"));
    assert!(strict_rendered.contains("chapter1.tex"));

    // A failed resolution is a structured ResolveError, not a panic.
    let missing = language.resolve_source("nope.tex", &trigger).unwrap_err();
    assert_eq!(missing.reference(), "nope.tex");

    println!("task3: all assertions passed");
}
