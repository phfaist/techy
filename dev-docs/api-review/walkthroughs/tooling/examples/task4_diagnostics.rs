//! Task 4 — compiler-style diagnostics rendering.
//!
//! Parse malformed input tolerantly, then render every diagnostic the way a compiler
//! would: severity, message, file:line:col, the source line, and a caret/underline
//! marking the span.
//!
//! Division of labor observed:
//!   Library-provided: severity, structured condition payload (downcastable),
//!     wire identifier, message text, exact SourceSpan, traceback frames,
//!     `format_position` ("@ (line 2, col 5) [origin]"), `Diagnostic::render`
//!     (message + position + traceback + include chain), `Diagnostics::render_all`.
//!   Hand-rolled here: the `file:line:col:` compact position format, fetching the
//!     source *line text*, and the caret/underline gutter — LineIndex answers
//!     line/col for an offset but there is no line→text/line-range lookup, so the
//!     line boundaries are re-scanned manually.

use std::sync::Arc;

use techy::engine::Language;
use techy::error::{Diagnostic, Recovery, Severity};
use techy::latexlike::{argument_specs, CallableType, LatexlikeDriver, MacroSpec};
use techy::scopes::Package;
use techy::source::Source;
use techy::UnresolvableCommand; // root re-export of the condition payload type

const DOC: &str = "\
A paragraph that is fine.
Then \\undefinedmacro appears.
A stray } close brace here.
An {unclosed group starts here
and runs to the end of input.
";

/// Compiler-style rendering of one diagnostic: hand-rolled position + source line +
/// underline. `origin_label` stands in for a file name.
fn render_compiler_style(diagnostic: &Diagnostic) -> String {
    let span = diagnostic.span();
    let source = span.source();
    let content = source.content();

    // line/col of the span start (LineIndex is cheap but must be rebuilt here —
    // it borrows the content and Diagnostic doesn't carry one).
    let mut line_index = source.line_index();
    let (line, col) = line_index
        .line_col(span.start())
        .unwrap_or((0, 0));

    let file = source
        .origin()
        .as_deref()
        .unwrap_or("<input>");

    // Hand-rolled: the text of the line containing the span start. LineIndex has no
    // "give me line N's range/text" query, so scan for the line boundaries.
    let line_start = content[..span.start()]
        .rfind('\n')
        .map(|i| i + 1)
        .unwrap_or(0);
    let line_end = content[span.start()..]
        .find('\n')
        .map(|i| span.start() + i)
        .unwrap_or(content.len());
    let line_text = &content[line_start..line_end];

    // Underline from the span start column across the span (clamped to this line).
    let underline_start = span.start() - line_start;
    let underline_len = span.len().clamp(1, line_text.len() - underline_start.min(line_text.len()));
    let gutter = " ".repeat(underline_start);
    let marker = if span.len() <= 1 {
        "^".to_string()
    } else {
        "^".repeat(underline_len)
    };

    format!(
        "{file}:{line}:{col}: {}: {}\n  |\n  | {line_text}\n  | {gutter}{marker}",
        diagnostic.severity(),
        diagnostic.message(),
    )
}

fn main() {
    let mut package = Package::new("diag-demo");
    package.insert(
        CallableType::Macro,
        "cite",
        Arc::new(MacroSpec::new(argument_specs(["o", "m"]).unwrap())),
    );
    let language = Language::new(LatexlikeDriver::new(Recovery::Tolerant))
        .with_provider(Arc::new(package))
        .unwrap();

    let source = Arc::new(Source::new(DOC).with_origin(Some("essay.tex".to_string())));
    let result = language.parse_source(source).unwrap();

    println!(
        "=== {} diagnostics ({} suppressed beyond cap {}) ===\n",
        result.diagnostics.len(),
        result.diagnostics.suppressed(),
        result.diagnostics.limit()
    );

    // -- 1. Our compiler-style rendering ---------------------------------------
    for diagnostic in result.diagnostics.iter() {
        println!("{}\n", render_compiler_style(diagnostic));
    }

    // -- 2. The library's own rendering, for comparison ------------------------
    println!("=== library render_all() ===");
    println!("{}", result.diagnostics.render_all());

    // -- 3. Structured (machine) access ----------------------------------------
    // Wire identifiers, stable across releases:
    println!("\n=== identifiers ===");
    for diagnostic in &result.diagnostics {
        println!("{} @{:?}", diagnostic.identifier(), diagnostic.span().range());
    }

    // Typed access by downcast: every unresolvable command's name.
    let unresolved: Vec<&str> = result
        .diagnostics
        .conditions::<UnresolvableCommand>()
        .map(|condition| condition.name.as_str())
        .collect();
    assert_eq!(unresolved, ["undefinedmacro"]);

    // Severity threshold filtering uses the derived Ord.
    let errors = result
        .diagnostics
        .iter()
        .filter(|d| d.severity() >= Severity::Warning)
        .count();
    assert_eq!(errors, result.diagnostics.len(), "all are Error severity");

    // -- 4. Traceback frames: an error nested in open constructs ---------------
    // The diagnostics above were recorded at the top level, so their `frames()` are
    // empty. An error inside an argument shows the traceback machinery.
    let nested = language.parse(r"see \cite[note]{Ref \oops more}").unwrap();
    let with_frames = nested
        .diagnostics
        .iter()
        .find(|d| !d.frames().is_empty())
        .expect("a diagnostic carrying traceback frames");
    println!("\n=== traceback frames (innermost first) ===");
    for frame in with_frames.frames() {
        println!("  {} -- {}", frame.title(), techy::format_position(frame.span()));
    }
    println!("--- format_traceback ---\n{}", techy::format_traceback(with_frames.frames()));
    let titles: Vec<&str> = with_frames.frames().iter().map(|f| f.title()).collect();
    assert!(
        titles.iter().any(|t| t.contains("cite")),
        "frame titles name the enclosing construct: {titles:?}"
    );

    // -- Assertions ------------------------------------------------------------
    assert_eq!(result.diagnostics.len(), 3, "\\undefinedmacro, stray }}, unclosed {{");
    assert!(!result.diagnostics.is_empty());
    assert!(result.diagnostics.has_errors());

    let identifiers: Vec<&str> = result
        .diagnostics
        .iter()
        .map(|d| d.identifier())
        .collect();
    // NB: the `<area>` segment of the wire identifier turns out to be the *module*
    // name (`nodes_parser`, `group_parser`) — discovered by running, not from docs.
    assert!(
        identifiers.contains(&"core.nodes_parser.unresolvable-command"),
        "{identifiers:?}"
    );
    assert!(
        identifiers.iter().any(|i| i.contains("stray")),
        "{identifiers:?}"
    );
    assert!(
        identifiers.iter().any(|i| i.contains("unclosed")),
        "{identifiers:?}"
    );

    // Spans are exact: the stray `}` diagnostic covers exactly the delimiter.
    let stray = result
        .diagnostics
        .iter()
        .find(|d| d.identifier().contains("stray"))
        .unwrap();
    assert_eq!(stray.span().content(), "}");

    // Rendered output shows the origin label and line info.
    let rendered = result.diagnostics.render_all();
    assert!(rendered.contains("[essay.tex]"));
    assert!(rendered.contains("(line 2, col 6)"));

    // Our compiler-style line for the unresolvable command underlines `\undefinedmacro `.
    let ours = render_compiler_style(result.diagnostics.iter().next().unwrap());
    assert!(ours.contains("essay.tex:2:6: error:"), "{ours}");
    assert!(ours.contains("Then \\undefinedmacro appears."), "{ours}");

    println!("\ntask4: all assertions passed");
}
