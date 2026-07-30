//! Task 1 — "editor hover" primitive.
//!
//! Parse a document with the latexlike preset and, for every node in the AST, report:
//! node kind, exact byte span, source text, and line/column start-end (via LineIndex).

use std::sync::Arc;

use techy::engine::Language;
use techy::latexlike::{argument_specs, CallableType, EnvironmentSpec, Latexlike, MacroSpec};
use techy::node::{NodeKind, NodeRef};
use techy::scopes::Package;
use techy::source::LineIndex;

const DOC: &str = "\
Intro text with $x+y$ inline math.
\\section{First}
See \\cite[Lemma 3]{Knuth84} for details. % a comment
\\begin{itemize} one {grouped} two \\end{itemize}
";

/// Structural kind name of a node. There is no ready-made accessor returning a plain
/// kind label; we match the closed `NodeKind` enum ourselves.
fn kind_name(node: &NodeRef<'_, Latexlike>) -> &'static str {
    match node.kind() {
        NodeKind::Chars { .. } => "Chars",
        NodeKind::Group(_) => "Group",
        NodeKind::Callable(_) => "Callable",
        NodeKind::Comment { .. } => "Comment",
        NodeKind::List { .. } => "List",
        // `NodeKind` is a closed enum (no #[non_exhaustive]); still be defensive:
        #[allow(unreachable_patterns)]
        _ => "?",
    }
}

/// One hover line for a node: kind, byte span, line/col range, text preview.
fn hover_line(
    node: &NodeRef<'_, Latexlike>,
    line_index: &mut LineIndex<'_>,
    depth: usize,
) -> String {
    let span = node.span();
    let (sl, sc) = line_index.line_col(span.start()).unwrap_or((0, 0));
    let (el, ec) = line_index.line_col(span.end()).unwrap_or((0, 0));
    let text = node.span_content();
    let preview: String = text
        .chars()
        .take(28)
        .map(|c| if c == '\n' { '␊' } else { c })
        .collect();
    let ellipsis = if text.chars().count() > 28 { "…" } else { "" };
    format!(
        "{:indent$}{:<9} bytes {:>3}..{:<3}  {}:{}-{}:{}  ‘{}{}’",
        "",
        kind_name(node),
        span.start(),
        span.end(),
        sl,
        sc,
        el,
        ec,
        preview,
        ellipsis,
        indent = depth * 2
    )
}

/// Recursive walk carrying depth. `NodeRef::descendants()` is preorder document order
/// but yields no depth information, so a hover/outline printer re-implements the walk.
fn walk(node: NodeRef<'_, Latexlike>, line_index: &mut LineIndex<'_>, depth: usize) {
    println!("{}", hover_line(&node, line_index, depth));
    for child in node.children() {
        walk(child, line_index, depth + 1);
    }
}

fn main() {
    // -- Language setup: the preset ships no macro database; register what the doc uses.
    let mut package = Package::new("hover-demo");
    package.insert(
        CallableType::Macro,
        "section",
        Arc::new(MacroSpec::new(argument_specs(["m"]).unwrap())),
    );
    package.insert(
        CallableType::Macro,
        "cite",
        Arc::new(MacroSpec::new(argument_specs(["o", "m"]).unwrap())),
    );
    package.insert(
        CallableType::Environment,
        "itemize",
        Arc::new(EnvironmentSpec::new(Vec::new())),
    );
    let language = Language::<Latexlike>::default()
        .with_provider(Arc::new(package))
        .unwrap();

    let result = language.parse(DOC).unwrap();
    let root = result.tree.root();

    // One shared LineIndex for the whole (single-source) tree. Nodes only hand out
    // their span's source; we grab the root's.
    let source = Arc::clone(root.span().source());
    let mut line_index = source.line_index();

    println!("=== hover report: {} nodes ===", result.tree.node_count());
    walk(root, &mut line_index, 0);

    // -- Assertions ------------------------------------------------------------
    // The root covers the whole document.
    assert_eq!(root.span().range(), 0..DOC.len());
    assert_eq!(root.span_content(), DOC);

    // Sibling spans partition the parent's span exactly (documented invariant) —
    // exactly what makes hover ranges trustworthy. (Checked at the root level; a
    // callable's children cover argument regions, not its trigger token, so the
    // "children cover the parent" reading holds for sibling *runs*, not callables.)
    let children = root.children();
    let covering = children.span().unwrap();
    assert_eq!(covering.range(), 0..DOC.len());
    let mut pos = 0;
    for child in children {
        assert_eq!(child.span().start(), pos);
        pos = child.span().end();
    }
    assert_eq!(pos, DOC.len());

    // Line/col spot checks: `\section` starts line 2, col 1.
    let section = root
        .descendants()
        .find(|n| n.macro_name() == Some("section"))
        .unwrap();
    assert_eq!(line_index.line_col(section.span().start()), Some((2, 1)));

    // The inline math group sits on line 1.
    let math = root.descendants().find(|n| n.is_math_group()).unwrap();
    let (line, col) = line_index.line_col(math.span().start()).unwrap();
    assert_eq!((line, col), (1, 17));
    assert_eq!(math.span_content(), "$x+y$");

    // An exclusive end offset equal to content length is still answerable.
    assert!(line_index.line_col(DOC.len()).is_some());
    assert_eq!(line_index.line_col(DOC.len() + 1), None);

    println!("\ntask1: all assertions passed");
}
