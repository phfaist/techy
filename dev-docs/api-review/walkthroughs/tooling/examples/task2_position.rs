//! Task 2 — reverse lookup: byte offset (cursor position) → innermost covering node,
//! with its ancestry chain.
//!
//! The public API offers no position→node query and `NodeRef` has no `parent()`
//! accessor, so the ancestry chain must be built during a root-to-leaf descent using
//! only `children()` and spans. The span partition invariant makes the descent
//! deterministic: at most one child of a sibling run covers a given offset.

use std::sync::Arc;

use techy::engine::Language;
use techy::latexlike::{argument_specs, CallableType, EnvironmentSpec, Latexlike, MacroSpec};
use techy::node::{NodeKind, NodeRef};
use techy::scopes::Package;

const DOC: &str = "\
Intro text with $x+y$ inline math.
\\section{First}
See \\cite[Lemma 3]{Knuth84} for details. % a comment
\\begin{itemize} one {grouped} two \\end{itemize}
";

fn kind_name(node: &NodeRef<'_, Latexlike>) -> &'static str {
    match node.kind() {
        NodeKind::Chars { .. } => "Chars",
        NodeKind::Group(_) => "Group",
        NodeKind::Callable(_) => "Callable",
        NodeKind::Comment { .. } => "Comment",
        NodeKind::List { .. } => "List",
        #[allow(unreachable_patterns)]
        _ => "?",
    }
}

/// Does `node`'s span cover byte `offset`? Half-open `start..end`, like the spans.
fn covers(node: &NodeRef<'_, Latexlike>, offset: usize) -> bool {
    let span = node.span();
    span.start() <= offset && offset < span.end()
}

/// The ancestry chain (outermost first, innermost last) of the innermost node whose
/// span covers `offset`. Implemented with public traversal only: descend from the
/// root, at each level picking the (unique, by the partition invariant) covering
/// child. Children are in source order, so a binary search over `child(i)` spans
/// would give O(log n) per level; linear is fine for a demo.
fn ancestry_at(root: NodeRef<'_, Latexlike>, offset: usize) -> Vec<NodeRef<'_, Latexlike>> {
    let mut chain = Vec::new();
    if !covers(&root, offset) {
        return chain;
    }
    let mut node = root;
    loop {
        chain.push(node);
        let next = node.children().iter().find(|child| covers(child, offset));
        match next {
            Some(child) => node = child,
            None => return chain,
        }
    }
}

fn describe(node: &NodeRef<'_, Latexlike>) -> String {
    let extra = node
        .macro_name()
        .map(|n| format!(" \\{n}"))
        .or_else(|| node.environment_name().map(|n| format!(" env {n}")))
        .or_else(|| {
            node.group_delimiters()
                .map(|(open, close)| format!(" {open}…{close}"))
        })
        .unwrap_or_default();
    format!(
        "{}{} @{}..{}",
        kind_name(node),
        extra,
        node.span().start(),
        node.span().end()
    )
}

fn main() {
    let mut package = Package::new("cursor-demo");
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

    // `LineIndex` borrows the source content, so the Arc needs its own binding
    // (`Arc::clone(...).line_index()` in one expression is rejected by borrowck).
    let source = Arc::clone(root.span().source());
    let mut line_index = source.line_index();

    // Simulated cursor positions.
    let probes: &[usize] = &[
        2,            // inside "Intro text…"
        18,           // inside $x+y$
        37,           // inside the `\section` trigger token itself
        63,           // inside "Lemma 3" (optional argument of \cite)
        126,          // inside "grouped" inside the itemize body
        140,          // inside `\end{itemize}` (covered by the env node, no child)
        DOC.len() - 1, // final newline
    ];

    for &offset in probes {
        let chain = ancestry_at(root, offset);
        let (line, col) = line_index.line_col(offset).unwrap();
        println!("offset {offset} ({line}:{col}):");
        for (depth, node) in chain.iter().enumerate() {
            println!("  {:indent$}{}", "", describe(node), indent = depth * 2);
        }
    }

    // -- Assertions ------------------------------------------------------------
    // Cursor in math content: List > Group($…$) > Chars.
    let chain = ancestry_at(root, 18);
    assert_eq!(chain.len(), 3);
    assert!(chain[1].is_math_group());
    assert_eq!(chain[2].chars(), Some("x+y"));

    // Cursor on the `\section` trigger token: innermost is the Callable itself —
    // a callable's children cover only argument regions, not the trigger spelling.
    let chain = ancestry_at(root, 37);
    assert_eq!(chain.last().unwrap().macro_name(), Some("section"));

    // Cursor in `Lemma 3`: Callable \cite > Group […] > Chars.
    let chain = ancestry_at(root, 63);
    let kinds: Vec<&str> = chain.iter().map(kind_name).collect();
    assert_eq!(kinds, ["List", "Callable", "Group", "Chars"]);
    assert_eq!(chain[1].macro_name(), Some("cite"));

    // Cursor in the environment body's group: env > body List > Group > Chars.
    let chain = ancestry_at(root, 126);
    assert_eq!(chain[1].environment_name(), Some("itemize"));
    let kinds: Vec<&str> = chain.iter().map(kind_name).collect();
    assert_eq!(kinds, ["List", "Callable", "List", "Group", "Chars"]);

    // Cursor inside `\end{itemize}`: covered by the environment node, but by no child
    // of it (the terminator is not a child node) — innermost is the Callable.
    let chain = ancestry_at(root, 140);
    assert_eq!(chain.last().unwrap().environment_name(), Some("itemize"));

    // Out of range: empty chain.
    assert!(ancestry_at(root, DOC.len() + 10).is_empty());

    println!("\ntask2: all assertions passed");
}
