//! Task 5 — reuse / re-parse contracts, probed.
//!
//! What the docs promise (see FRICTION.md task 5 for the full survey):
//!   - `Language`: "define a language once, parse many documents", owns no per-parse
//!     state; results carry no reference to it.
//!   - `ParserSession`: transient, one parse each, no reuse.
//!   - Sources are immutable once created; `SourceSpan` equality is *identity-based*
//!     (same `Arc<Source>`) plus byte range.
//!   - Nothing anywhere about incremental parsing, cheap re-parse of a subrange, or
//!     mapping spans across edits.
//!
//! Consequence probed here: span stability across re-parses depends on WHICH entry
//! point you use. `parse_source(Arc<Source>)` re-parses the same source, so spans of
//! corresponding nodes compare equal across parses; `parse(content)` mints a fresh
//! anonymous Source per call, so identical spans compare UNEQUAL across parses even
//! for identical content.

use std::sync::Arc;

use techy::engine::Language;
use techy::latexlike::Latexlike;
use techy::source::Source;

const DOC: &str = "one {two} three $x$";

fn main() {
    let language: Language<Latexlike> = Language::default();

    // -- Re-parse of one Arc'd source: spans are stable and comparable. ---------
    let source = Arc::new(Source::new(DOC));
    let first = language.parse_source(Arc::clone(&source)).unwrap();
    let second = language.parse_source(Arc::clone(&source)).unwrap();

    let group_a = first.tree.root().child(1).unwrap();
    let group_b = second.tree.root().child(1).unwrap();
    assert_eq!(group_a.span(), group_b.span(), "same Arc source: spans equal");
    assert!(group_a.span().same_source(group_b.span()));

    // Trees are independent values; the language holds no per-parse state, and the
    // results outlive it:
    drop(language);
    assert_eq!(group_a.span_content(), "{two}");

    // -- parse(content) mints a fresh Source each call. -------------------------
    let language: Language<Latexlike> = Language::default();
    let first = language.parse(DOC).unwrap();
    let second = language.parse(DOC).unwrap();
    let group_a = first.tree.root().child(1).unwrap();
    let group_b = second.tree.root().child(1).unwrap();
    assert_eq!(group_a.span().range(), group_b.span().range());
    assert_ne!(
        group_a.span(),
        group_b.span(),
        "identity-based equality: same bytes, different Source => unequal"
    );
    assert!(!group_a.span().same_source(group_b.span()));

    // NodeIds are likewise per-tree: NodeTree::get() treats a foreign id as a miss in
    // debug builds. (Documented; matters if a tool caches ids across re-parses.)

    println!("task5: all assertions passed");
    println!("- parse_source(same Arc): spans equal & comparable across re-parses");
    println!("- parse(content): fresh Source per call, spans not comparable");
    println!("- no incremental / subrange re-parse API; no span-mapping across edits");
}
