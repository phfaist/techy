//! Golden files of the serialized form (cargo feature `serde`): pinned canonical
//! renderings of three small streams under `tests/golden/serialize/`, so that any
//! change to the wire form — a key name, an ordering, an entry shape, the sharing
//! between entries — shows up as a readable diff of a checked-in file, and so that
//! the rendering is pinned across processes and machines (determinism; cf. [§dd-dr:serialize-sessions-segments]).
//!
//! Each golden file holds the segments of one stream, in order, as pretty-printed
//! JSON documents separated by a blank line (`serde_json::to_string_pretty`; the
//! canonical rendering differs from it only in whitespace, and both read back to
//! the same value — the tests compare the segments as values AND their canonical
//! one-line renderings). Reading a golden file back into a fresh session is part of
//! every test: the files double as pinned readable inputs.
//!
//! **Regenerating** after an intended wire change: run the tests with the
//! environment variable `UPDATE_GOLDEN` set to exactly `1` (any other value leaves the
//! files alone; a banner on standard error announces each regeneration) —
//!
//! ```text
//! UPDATE_GOLDEN=1 cargo test --features serde -p techy --test serialize_golden
//! ```
//!
//! — which rewrites every golden file from the current output (the tests then pass);
//! review the diff of `tests/golden/serialize/` and commit it with the change.
//! Without the variable, a mismatch fails the test and prints the path of the file
//! to compare (`cargo test … -- --nocapture` for the first differing document).

#![cfg(feature = "serde")]

use std::path::PathBuf;
use std::sync::Arc;

use techy::core::node::display_tree;
use techy::core::specs::{Package, SpecsProvider};
use techy::core::{Language, ParseResult, ParsingState};
use techy::error::Recovery;
use techy::latexlike::minidefs::{self, minilatex_package};
use techy::latexlike::serialize::{register, register_package_recipes};
use techy::latexlike::{Latexlike, LatexlikeDriver};
use techy::serialize::{
    KnownProviders, ParseResultSerialization, SerdeSession, Segment, StandardTableInterning,
    StandardTableReading, TableId,
};
use techy::source::Source;

// --- the golden files -------------------------------------------------------------------------

/// The directory of the golden files.
fn golden_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests").join("golden").join("serialize")
}

/// The golden file's content for `segments`: pretty-printed documents, blank-line
/// separated, ending with a newline.
fn render_golden(segments: &[Segment]) -> String {
    let mut out = String::new();
    for segment in segments {
        out.push_str(&serde_json::to_string_pretty(segment).expect("a segment renders"));
        out.push_str("\n\n");
    }
    out
}

/// The segments of a golden file's content, in order.
fn parse_golden(content: &str) -> Vec<Segment> {
    serde_json::Deserializer::from_str(content)
        .into_iter::<Segment>()
        .collect::<Result<Vec<Segment>, _>>()
        .expect("every document of a golden file is a segment")
}

/// Compare `segments` with the golden file `name` (or rewrite it under
/// `UPDATE_GOLDEN`), and return the golden segments as read back from the file.
fn check_golden(name: &str, segments: &[Segment]) -> Vec<Segment> {
    let path = golden_dir().join(name);
    let rendered = render_golden(segments);
    if std::env::var("UPDATE_GOLDEN").ok().as_deref() == Some("1") {
        eprintln!("=== UPDATE_GOLDEN=1: regenerating the golden file {} ===", path.display());
        std::fs::create_dir_all(golden_dir()).expect("the golden directory is writable");
        std::fs::write(&path, &rendered).expect("the golden file is writable");
    }
    let stored = std::fs::read_to_string(&path).unwrap_or_else(|error| {
        panic!(
            "cannot read the golden file {}: {error}\n\
             (regenerate with `UPDATE_GOLDEN=1 cargo test --features serde -p techy --test serialize_golden`)",
            path.display()
        )
    });
    // The pinned form: the file's bytes.
    if stored != rendered {
        let expected = parse_golden(&stored);
        for (i, (want, got)) in expected.iter().zip(segments).enumerate() {
            if want != got {
                eprintln!("golden {name}: segment #{i} differs");
                eprintln!("--- golden ---\n{}", serde_json::to_string(want).unwrap());
                eprintln!("--- current ---\n{}", serde_json::to_string(got).unwrap());
                break;
            }
        }
        panic!(
            "the serialized form differs from the golden file {} \
             (an intended wire change: regenerate with `UPDATE_GOLDEN=1 cargo test --features serde -p techy \
             --test serialize_golden` and review the diff)",
            path.display()
        );
    }
    let stored_segments = parse_golden(&stored);
    // As values, and as canonical one-line renderings, the file agrees with the output.
    assert_eq!(stored_segments, segments);
    for (want, got) in stored_segments.iter().zip(segments) {
        assert_eq!(serde_json::to_string(want).unwrap(), serde_json::to_string(got).unwrap());
    }
    stored_segments
}

// --- fixtures ---------------------------------------------------------------------------------

/// The profile every stream here declares.
const PROFILE: &str = "techy golden files";

/// A minilatex language (tolerant), plus `extra` packages.
fn language(extra: impl IntoIterator<Item = Arc<dyn SpecsProvider<Latexlike>>>) -> Language<Latexlike> {
    let mut packages: Vec<Arc<dyn SpecsProvider<Latexlike>>> = vec![minilatex_package()];
    packages.extend(extra);
    let seed = ParsingState::lang_initial_with_packages(packages).expect("seed state");
    Language::new(LatexlikeDriver::new(Recovery::Tolerant), seed)
}

/// A session with the standard tables, the preset's readers, the profile, and a
/// reading environment holding `language`'s packages by identity plus the preset's
/// recipes (see `tests/serialize_stream.rs`).
fn session(language: &Language<Latexlike>) -> SerdeSession<Latexlike> {
    let mut session = SerdeSession::<Latexlike>::new();
    session.set_profile(PROFILE);
    let mut known = KnownProviders::<Latexlike>::new();
    for provider in language.initial_state().scopes().providers() {
        known.insert(Arc::clone(provider));
    }
    register_package_recipes(&mut known);
    minidefs::register_package_recipes(&mut known);
    session.set_user_data(known);
    register(&mut session).expect("the preset's readers register once");
    session
}

fn parse(language: &Language<Latexlike>, input: &str) -> Arc<ParseResult<Latexlike>> {
    Arc::new(language.parse(input).expect("a tolerant parse completes"))
}

fn emit(writer: &mut SerdeSession<Latexlike>, result: &Arc<ParseResult<Latexlike>>) -> Segment {
    let position = writer.serialize_parse_result(result).expect("the parse result serializes");
    writer.take_segment_with_main(position).expect("the position is the writer's own")
}

/// The parse result a segment named as its main entry, as `push_segment` handed it back.
fn main_parse_result(reader: &mut SerdeSession<Latexlike>, main: Option<(TableId, u32)>) -> Arc<ParseResult<Latexlike>> {
    let (table, index) = main.expect("the segment names its main entry");
    let parse_results = reader.standard_tables().expect("standard tables").parse_results;
    assert_eq!(table, parse_results.id());
    reader.parse_result(parse_results.position(index)).expect("the main parse result reads back")
}

// --- (i) a minimal state + source segment ------------------------------------------------------

/// One source and the language's initial parsing state — the smallest segment with
/// entries in the sources, states, and providers tables (the state's scope stack
/// refers to its packages by identity).
#[test]
fn golden_state_and_source() {
    let language = language([]);
    let mut writer = session(&language);
    let source = Arc::new(Source::new("Hello, \\emph{world}!\n").with_origin(Some(String::from("hello.tex"))));
    writer.intern_source(&source).unwrap();
    writer.intern_state(language.initial_state()).unwrap();
    let segment = writer.take_segment();

    let golden = check_golden("state_and_source.json", std::slice::from_ref(&segment));

    // The golden segment reads back into a fresh session: the source and the state.
    let mut reader = session(&language);
    reader.push_segment(golden.into_iter().next().unwrap()).unwrap();
    let tables = reader.standard_tables().unwrap();
    let back = reader.source(tables.sources.position(0)).unwrap();
    assert_eq!(back.content(), source.content());
    assert_eq!(back.origin().as_deref(), Some("hello.tex"));
    let state = reader.state(tables.states.position(0)).unwrap();
    assert_eq!(state.scopes().providers().len(), language.initial_state().scopes().providers().len());
}

// --- (ii) the schema description's worked example ---------------------------------------------

/// The schema description's example (`dev-docs/serialize_schema.md`): a tolerant
/// parse of `\e{x} {` with a shared package defining `\e{m}`, the parse result as the
/// segment's main entry (the same input the description's generator uses, with this
/// file's profile).
#[test]
fn golden_schema_example() {
    fn defs() -> Arc<dyn SpecsProvider<Latexlike>> {
        Package::<Latexlike>::new_shared("d", |package| {
            package.define_macro("e", ["m"]).unwrap();
        })
    }
    let language = language([defs()]);
    let result = parse(&language, "\\e{x} {");
    assert!(result.diagnostics.has_errors(), "the unclosed group is diagnosed");
    let mut writer = session(&language);
    let segment = emit(&mut writer, &result);

    let golden = check_golden("schema_example.json", std::slice::from_ref(&segment));

    // The golden segment reads back: the parse result, its tree, its diagnostics.
    let mut reader = session(&language);
    let main = reader.push_segment(golden.into_iter().next().unwrap()).unwrap();
    let back = main_parse_result(&mut reader, main);
    assert_eq!(display_tree(back.tree.root()), display_tree(result.tree.root()));
    assert_eq!(back.diagnostics.render_all(), result.diagnostics.render_all());
}

// --- (iii) a multi-segment read-then-append stream ----------------------------------------------

const TEXT_A: &str = "\\emph{a} \\begin{itemize}\\item x~y\\end{itemize}";
const TEXT_B: &str = "b \\textbf{c} {unclosed";
const TEXT_C: &str = "\\textit{c} --- and $m$";

/// Three segments: two parses written by one session, then a third parse appended
/// by a session that absorbed the first two (reading then appending) — the second
/// and third segments carry only what is new (no provider twice, the seed state
/// shared).
#[test]
fn golden_read_then_append_stream() {
    let language = language([]);
    let a = parse(&language, TEXT_A);
    let b = parse(&language, TEXT_B);
    let c = parse(&language, TEXT_C);

    let mut writer = session(&language);
    let first = emit(&mut writer, &a);
    let second = emit(&mut writer, &b);
    let mut appender = session(&language);
    appender.push_segment(first.clone()).unwrap();
    appender.push_segment(second.clone()).unwrap();
    let third = emit(&mut appender, &c);
    let stream = [first, second, third];

    let golden = check_golden("read_then_append.json", &stream);

    // The whole golden stream reads back in order, with the sharing intact.
    let mut reader = session(&language);
    let mains: Vec<Option<(TableId, u32)>> = golden.into_iter().map(|segment| reader.push_segment(segment).unwrap()).collect();
    let back: Vec<Arc<ParseResult<Latexlike>>> = mains.into_iter().map(|main| main_parse_result(&mut reader, main)).collect();
    for (original, rebuilt) in [&a, &b, &c].into_iter().zip(&back) {
        assert_eq!(display_tree(rebuilt.tree.root()), display_tree(original.tree.root()));
        assert_eq!(rebuilt.diagnostics.render_all(), original.diagnostics.render_all());
    }
    // A and B were parsed and written by one session and share their seed state; C
    // was appended by another session, whose parse holds its own state objects.
    assert!(Arc::ptr_eq(back[0].tree.root().parsing_state(), back[1].tree.root().parsing_state()));
    assert!(!Arc::ptr_eq(back[0].tree.root().parsing_state(), back[2].tree.root().parsing_state()));
}
