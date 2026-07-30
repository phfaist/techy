//! notely — a small custom markup language built on techy's generic machinery.
//! Runs all walkthrough tasks with assertions; prints AST dumps and diagnostics.

mod lang;
mod specs;
mod title;

use std::sync::Arc;

use techy::constructs::{MissingMandatoryArgument, UnresolvableCommand};
use techy::engine::Language;
use techy::error::{DiagnosticInfo, Recovery};
use techy::node::NodeRef;

use lang::{Notely, NotelyDriver, NotelyGroup};
use specs::notely_std;
use title::notely_ext;

/// Recursive AST dump: generic NodeRef::summary() per node, indented.
fn dump(node: NodeRef<'_, Notely>, indent: usize) {
    println!(
        "{}{}  [{}..{}]",
        "  ".repeat(indent),
        node.summary(),
        node.span().range().start,
        node.span().range().end
    );
    for child in node.children().iter() {
        dump(child, indent + 1);
    }
}

fn build_language(recovery: Recovery) -> Language<Notely> {
    Language::<Notely>::new(NotelyDriver::new(recovery))
        .with_provider(Arc::new(notely_std()))
        .expect("notely-std loads")
        .with_provider(Arc::new(notely_ext()))
        .expect("notely-ext loads")
}

fn task2_parse_and_assert(language: &Language<Notely>) {
    println!("=== Task 2: parse a notely document, dump + assert the AST ===");
    let doc = "Hello [brave] world!\n# note to self\n@bold(really)";
    let result = language.parse(doc).expect("clean notely parse");
    dump(result.tree.root(), 0);
    assert!(result.diagnostics.is_empty());

    let root = result.tree.root();
    // Shape: chars, [group], chars, comment, chars(pre-space), @bold callable.
    let children: Vec<NodeRef<'_, Notely>> = root.children().iter().collect();

    assert!(children[0].is_chars());
    assert_eq!(children[0].chars(), Some("Hello "));

    assert!(children[1].is_group());
    assert_eq!(children[1].group_type(), Some(NotelyGroup::Square));
    assert_eq!(children[1].group_delimiters(), Some(("[", "]")));
    assert_eq!(children[1].child(0).unwrap().chars(), Some("brave"));

    assert_eq!(children[2].chars(), Some(" world!\n"));

    assert!(children[3].is_comment());
    assert_eq!(children[3].comment(), Some(" note to self"));
    assert_eq!(children[3].comment_start(), Some("#"));

    // The @bold callable, with its paren argument's content.
    let bold = children
        .iter()
        .find(|node| node.name() == Some("bold"))
        .expect("@bold parsed as a callable");
    assert!(bold.is_callable());
    assert_eq!(bold.span_content(), "@bold(really)");
    let argument = bold.argument_content_nodes(0).expect("argument 0 provided");
    assert_eq!(argument.source_text(), Some("really"));

    // No math anywhere: '$' is an ordinary character in notely.
    let math_doc = language.parse("price: $5 + $6").expect("dollars are chars");
    assert_eq!(math_doc.tree.root().child_count(), 1);
    assert_eq!(math_doc.tree.root().child(0).unwrap().chars(), Some("price: $5 + $6"));

    // Parens in running text are ordinary characters too (Paren is argument-only).
    let parens = language.parse("just (plain) text").expect("parens are chars");
    assert_eq!(parens.tree.root().child_count(), 1);
    println!("task 2 OK\n");
}

fn task3_custom_callables(language: &Language<Notely>) {
    println!("=== Task 3: two custom callables (@bold, @link) ===");
    let doc = "see @link(https://example.org)(the [linked] site) and @bold(this)";
    let result = language.parse(doc).expect("clean parse");
    dump(result.tree.root(), 0);

    let root = result.tree.root();
    let link = root
        .children()
        .iter()
        .find(|node| node.name() == Some("link"))
        .expect("@link parsed");
    // By-name argument access (the spec named them "url" and "text").
    let url = link.argument_content_nodes_named("url").expect("url provided");
    assert_eq!(url.source_text(), Some("https://example.org"));
    let text = link.argument_content_nodes_named("text").expect("text provided");
    assert_eq!(text.source_text(), Some("the [linked] site"));
    // Nested square group inside the argument still parses as a group.
    assert!(text.iter().any(|node| node.is_group()));

    let bold = root
        .children()
        .iter()
        .find(|node| node.name() == Some("bold"))
        .expect("@bold parsed");
    assert_eq!(bold.argument_content_nodes(0).unwrap().source_text(), Some("this"));

    // The `-->` specials shorthand fires in running text; `--` alone stays chars.
    let arrows = language.parse("go --> there -- or not").expect("clean parse");
    dump(arrows.tree.root(), 0);
    let arrow = arrows
        .tree
        .root()
        .children()
        .iter()
        .find(|node| node.name() == Some("-->"))
        .expect("--> parsed as a callable");
    assert_eq!(arrow.callable_type(), Some(lang::NotelyCallable::Shorthand));
    assert!(arrows
        .tree
        .root()
        .children()
        .iter()
        .any(|node| node.chars() == Some(" there -- or not")));
    println!("task 3 OK\n");
}

fn task4_diagnostics() {
    println!("=== Task 4: diagnostics from malformed notely input ===");

    // (a) Strict: an unresolvable command aborts; display the rendered error.
    let strict = build_language(Recovery::Strict);
    let err = strict.parse("see @missing(now)").expect_err("strict aborts");
    println!("--- strict abort (unresolvable @missing) ---");
    println!("{}", err.render());
    assert_eq!(err.identifier(), UnresolvableCommand::IDENTIFIER);

    // (b) Tolerant: same input recovers as chars, diagnostic on the result.
    let tolerant = build_language(Recovery::Tolerant);
    let result = tolerant.parse("see @missing(now)").expect("tolerant completes");
    println!("--- tolerant diagnostics ---");
    println!("{}", result.diagnostics.render_all());
    assert_eq!(result.diagnostics.len(), 1);
    let diagnostic = result.diagnostics.iter().next().unwrap();
    assert_eq!(diagnostic.identifier(), UnresolvableCommand::IDENTIFIER);
    assert!(diagnostic.message().contains("@missing"));

    // (c) A missing mandatory argument: @bold with no (...).
    let result = tolerant.parse("some @bold text").expect("tolerant completes");
    println!("--- tolerant diagnostics (missing argument) ---");
    println!("{}", result.diagnostics.render_all());
    assert_eq!(result.diagnostics.len(), 1);
    assert_eq!(
        result.diagnostics.iter().next().unwrap().identifier(),
        MissingMandatoryArgument::IDENTIFIER
    );

    // (d) An unclosed group aborts strict with a traceback frame for the group.
    let err = strict.parse("oops [never closed").expect_err("strict aborts");
    println!("--- strict abort (unclosed group) ---");
    println!("{}", err.render());
    println!("task 4 OK\n");
}

fn task5_title(language: &Language<Notely>) {
    println!("=== Task 5: custom ConstructParser — @title takes the rest of the line raw ===");
    let doc = "@title Notes on [stuff] & @things # not a comment\nBody with @bold(text).";
    let result = language.parse(doc).expect("clean parse");
    dump(result.tree.root(), 0);

    let root = result.tree.root();
    let title = root
        .children()
        .iter()
        .find(|node| node.name() == Some("title"))
        .expect("@title parsed");
    // The whole rest of the line, raw: brackets, @, # all inert.
    let line = title.slot_content_nodes_named("line").expect("line slot");
    assert_eq!(
        line.source_text(),
        Some("Notes on [stuff] & @things # not a comment")
    );
    // The body after the newline parses as normal notely again.
    assert!(root.children().iter().any(|node| node.name() == Some("bold")));
    println!("task 5 OK\n");
}

fn main() {
    let language = build_language(Recovery::Strict);
    task2_parse_and_assert(&language);
    task3_custom_callables(&language);
    task4_diagnostics();
    task5_title(&language);
    println!("all notely tasks passed");
}
