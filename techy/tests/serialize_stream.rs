//! Streams of segments through the public API (cargo feature `serde`): the JSON Lines
//! convention — one [`Segment`] per line, each line an independently valid value, the
//! stream ending with the input, appended to by appending lines — and the same stream
//! through a binary format (postcard, length-prefixed frames). Two parses are written
//! by one session, read by another that then appends a third parse of its own
//! (reading then appending), and the whole stream is read by a third session with the
//! sharing between the parses intact.

#![cfg(feature = "serde")]

use std::sync::Arc;

use techy::core::node::{display_tree, NodeTree};
use techy::core::specs::SpecsProvider;
use techy::core::{Language, ParseResult, ParsingState};
use techy::error::Recovery;
use techy::latexlike::minidefs::{self, minilatex_package};
use techy::latexlike::serialize::{register, register_package_recipes};
use techy::latexlike::{source_recomposer, Latexlike, LatexlikeDriver, MacroSpec};
use techy::recompose::TreeRecomposer;
use techy::serialize::{
    DeserializeError, KnownProviders, ParseResultIndex, ParseResultSerialization, SerdeSession,
    Segment, SerialIndex,
};

// --- fixtures -----------------------------------------------------------------------------

/// The minilatex language, tolerant (so a document with a mistake still yields a parse
/// result, with diagnostics).
fn language() -> Language<Latexlike> {
    let seed = ParsingState::lang_initial_with_packages([minilatex_package() as Arc<dyn SpecsProvider<Latexlike>>])
        .expect("seed state");
    Language::new(LatexlikeDriver::new(Recovery::Tolerant), seed)
}

/// A session with the standard tables, the preset's readers, and a reading
/// environment holding `language`'s own packages by identity (the flow of a program
/// that reads a stream and appends to it: what it parses with is what the stream's
/// provider entries resolve to, so its own parses intern the same instances and
/// nothing is written twice), with the preset's recipes as the fallback for a package
/// the language does not hold in its seed (the nested `minilatex.item`).
fn session(language: &Language<Latexlike>) -> SerdeSession<Latexlike> {
    let mut session = SerdeSession::<Latexlike>::new();
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

/// The package instance a `\name` node's spec was defined in (through its
/// provenance stamp), for identity checks across parse results.
fn package_of(result: &ParseResult<Latexlike>, name: &str) -> Arc<dyn SpecsProvider<Latexlike>> {
    let node = result
        .tree
        .root()
        .children()
        .iter()
        .find(|node| node.macro_name() == Some(name))
        .unwrap_or_else(|| panic!("a `\\{name}` node"));
    let spec = node.spec().expect("a callable node has a spec");
    let macro_spec = (&**spec as &dyn std::any::Any).downcast_ref::<MacroSpec<Latexlike>>().expect("a MacroSpec");
    macro_spec.provenance().expect("stamped").provider().expect("the package is alive")
}

const DOC_A: &str = "\\emph{a} \\begin{itemize}\\item x~y\\end{itemize}";
const DOC_B: &str = "b \\textbf{c} {unclosed";
const DOC_C: &str = "\\textit{c} --- and $m$";

fn parse(language: &Language<Latexlike>, input: &str) -> Arc<ParseResult<Latexlike>> {
    Arc::new(language.parse(input).expect("a tolerant parse completes"))
}

/// The source spelling the tree re-emits.
fn recomposed(tree: &NodeTree<Latexlike>) -> String {
    TreeRecomposer::new(&mut source_recomposer::<Latexlike>()).recompose(tree, ()).expect("a parsed tree recomposes")
}

/// The rebuilt parse result matches the original: the tree's structure (its display
/// rendering) and spelling, and the diagnostics' identifiers and messages.
fn assert_matches(original: &ParseResult<Latexlike>, back: &ParseResult<Latexlike>) {
    assert_eq!(display_tree(back.tree.root()), display_tree(original.tree.root()));
    assert_eq!(recomposed(&back.tree), recomposed(&original.tree));
    let (a, b) = (&original.diagnostics, &back.diagnostics);
    assert_eq!(b.len(), a.len());
    assert_eq!(b.render_all(), a.render_all());
    for (x, y) in a.iter().zip(b.iter()) {
        assert_eq!(y.identifier(), x.identifier());
        assert_eq!(y.message(), x.message());
    }
}

/// The parse result at `index` of a reader's parse-results table.
fn parse_result_at(reader: &mut SerdeSession<Latexlike>, index: u32) -> Arc<ParseResult<Latexlike>> {
    let table = reader.standard_tables().expect("standard tables").parse_results;
    reader.parse_result(table.position(index)).expect("the parse result reads back")
}

/// The number of entries the segment carries for table `name`, and their start.
fn part(segment: &Segment, name: &str) -> (usize, u32) {
    let table = segment.tables().iter().find(|t| t.name() == name).expect("the table is in the directory");
    (table.entries().len(), table.start())
}

// --- JSON Lines -----------------------------------------------------------------------------

/// One segment per line: encode with `serde_json::to_string`, append a newline.
fn write_line(stream: &mut String, segment: &Segment) {
    let line = serde_json::to_string(segment).expect("a segment encodes as JSON");
    assert!(!line.contains('\n'), "the canonical rendering has no raw line breaks");
    stream.push_str(&line);
    stream.push('\n');
}

/// Absorb every line of `stream` into `reader`, in order.
fn read_lines(reader: &mut SerdeSession<Latexlike>, stream: &str) {
    for line in stream.lines() {
        let segment: Segment = serde_json::from_str(line).expect("each line is a segment");
        reader.push_segment(segment).expect("the segments continue the stream in order");
    }
}

#[test]
fn a_json_lines_stream_is_read_then_appended_then_read_again() {
    let language = language();
    let a = parse(&language, DOC_A);
    let b = parse(&language, DOC_B);
    assert!(b.diagnostics.has_errors(), "document B recovers from a mistake");

    // Write: two parses, one segment (one line) each.
    let mut stream = String::new();
    let mut writer = session(&language);
    let a_position = writer.serialize_parse_result(&a).unwrap();
    let first = writer.take_segment();
    write_line(&mut stream, &first);
    let b_position = writer.serialize_parse_result(&b).unwrap();
    let second = writer.take_segment();
    write_line(&mut stream, &second);
    assert_eq!((a_position.index(), b_position.index()), (0, 1));
    // The second segment carries only what is new: B's own source and tree, no
    // provider (the packages went out with A), and it starts where the first ended.
    assert_eq!(part(&second, "sources"), (1, 1));
    assert_eq!(part(&second, "providers").0, 0);
    assert_eq!(part(&second, "trees"), (1, 1));
    assert_eq!(part(&second, "parse-results"), (1, 1));
    assert_eq!(stream.lines().count(), 2);

    // Read, then append: a second session absorbs both lines, parses a third
    // document, and emits a third line continuing the stream.
    let mut reader = session(&language);
    read_lines(&mut reader, &stream);
    let back_a = parse_result_at(&mut reader, 0);
    let back_b = parse_result_at(&mut reader, 1);
    assert_matches(&a, &back_a);
    assert_matches(&b, &back_b);
    // Sharing across the parses, as written by one session: both trees' roots hold the
    // very same seed state (one entry), and both `\emph` and `\textbf` come from the
    // one `minilatex` instance the environment holds.
    assert!(Arc::ptr_eq(back_a.tree.root().parsing_state(), back_b.tree.root().parsing_state()));
    assert!(Arc::ptr_eq(&package_of(&back_a, "emph"), &package_of(&back_b, "textbf")));
    assert!(Arc::ptr_eq(&package_of(&back_a, "emph"), &language.initial_state().scopes().providers()[1]));

    // Append: the reader parses a third document with the same language and emits a
    // segment continuing the stream. Its packages are the instances the absorbed
    // provider entries resolved to, so no provider is written again; its states are
    // the parse's own live objects — new entries (a rebuilt state is a different
    // object from the language's live one).
    let c = parse(&language, DOC_C);
    let c_position = reader.serialize_parse_result(&c).unwrap();
    assert_eq!(c_position.index(), 2, "the appended parse result continues the stream's numbering");
    let third = reader.take_segment();
    assert_eq!(part(&third, "providers").0, 0, "the packages were absorbed, not re-emitted");
    assert_eq!(part(&third, "parse-results"), (1, 2));
    assert!(part(&third, "states").0 >= 1);
    write_line(&mut stream, &third);
    assert_eq!(stream.lines().count(), 3);

    // Read the whole stream in a third session.
    let mut third_reader = session(&language);
    read_lines(&mut third_reader, &stream);
    let final_a = parse_result_at(&mut third_reader, 0);
    let final_b = parse_result_at(&mut third_reader, 1);
    let final_c = parse_result_at(&mut third_reader, 2);
    assert_matches(&a, &final_a);
    assert_matches(&b, &final_b);
    assert_matches(&c, &final_c);
    // A and B share their seed state entry; C's root state is its own entry (see
    // above); all three resolve their packages to the environment's one instance.
    assert!(Arc::ptr_eq(final_a.tree.root().parsing_state(), final_b.tree.root().parsing_state()));
    assert!(!Arc::ptr_eq(final_a.tree.root().parsing_state(), final_c.tree.root().parsing_state()));
    assert!(Arc::ptr_eq(&package_of(&final_a, "emph"), &package_of(&final_c, "textit")));
}

#[test]
fn every_line_is_a_valid_segment_but_positions_are_stream_scoped() {
    let language = language();
    let mut stream = String::new();
    let mut writer = session(&language);
    writer.serialize_parse_result(&parse(&language, DOC_A)).unwrap();
    write_line(&mut stream, &writer.take_segment());
    writer.serialize_parse_result(&parse(&language, DOC_B)).unwrap();
    write_line(&mut stream, &writer.take_segment());

    // The second line alone is a well-formed segment (its own version and directory)…
    let second_line = stream.lines().nth(1).unwrap();
    let second: Segment = serde_json::from_str(second_line).expect("an independently valid value");
    assert_eq!(second.version(), Segment::VERSION);
    // …but it continues a stream: pushed into a session that has not absorbed the
    // first line, it is out of order.
    let mut reader = session(&language);
    let error = reader.push_segment(second).unwrap_err();
    assert!(matches!(error, DeserializeError::SegmentOutOfOrder { .. }), "{error}");
    // The session is unchanged and reads the stream fine from the start.
    read_lines(&mut reader, &stream);
    assert_eq!(parse_result_at(&mut reader, 1).diagnostics.len(), 1);
}

#[test]
fn a_truncated_stream_loses_only_its_last_line() {
    let language = language();
    let mut stream = String::new();
    let mut writer = session(&language);
    for input in [DOC_A, DOC_B, DOC_C] {
        writer.serialize_parse_result(&parse(&language, input)).unwrap();
        write_line(&mut stream, &writer.take_segment());
    }
    // Cut the stream in the middle of its last line (a partial write).
    let cut = stream.len() - 20;
    let truncated = &stream[..cut];
    let mut reader = session(&language);
    let mut absorbed = 0;
    for line in truncated.lines() {
        match serde_json::from_str::<Segment>(line) {
            Ok(segment) => {
                reader.push_segment(segment).unwrap();
                absorbed += 1;
            }
            Err(_) => break,
        }
    }
    assert_eq!(absorbed, 2, "the two complete lines are absorbed; the partial one is not a segment");
    assert!(parse_result_at(&mut reader, 1).diagnostics.has_errors());
    let missing: Result<Arc<ParseResult<Latexlike>>, _> = {
        let table = reader.standard_tables().unwrap().parse_results;
        reader.parse_result(table.position(2))
    };
    assert!(matches!(missing, Err(DeserializeError::IndexOutOfRange { table: "parse-results", index: 2, len: 2 })));
}

// --- a binary stream ---------------------------------------------------------------------------

/// One segment per length-prefixed frame (a `u32` little-endian byte count, then the
/// postcard encoding): the binary counterpart of a line.
fn write_frame(stream: &mut Vec<u8>, segment: &Segment) {
    let bytes = postcard::to_allocvec(segment).expect("a segment encodes through postcard");
    let len = u32::try_from(bytes.len()).expect("a frame fits u32");
    stream.extend_from_slice(&len.to_le_bytes());
    stream.extend_from_slice(&bytes);
}

/// The frames of `stream`, in order.
fn frames(stream: &[u8]) -> Vec<&[u8]> {
    let mut frames = Vec::new();
    let mut rest = stream;
    while !rest.is_empty() {
        let (len, tail) = rest.split_at(4);
        let len = u32::from_le_bytes(len.try_into().unwrap()) as usize;
        let (frame, tail) = tail.split_at(len);
        frames.push(frame);
        rest = tail;
    }
    frames
}

fn read_frames(reader: &mut SerdeSession<Latexlike>, stream: &[u8]) {
    for frame in frames(stream) {
        let segment: Segment = postcard::from_bytes(frame).expect("each frame is a segment");
        reader.push_segment(segment).expect("the segments continue the stream in order");
    }
}

#[test]
fn a_postcard_stream_is_read_then_appended_then_read_again() {
    let language = language();
    let a = parse(&language, DOC_A);
    let b = parse(&language, DOC_B);
    let mut stream: Vec<u8> = Vec::new();
    let mut writer = session(&language);
    writer.serialize_parse_result(&a).unwrap();
    write_frame(&mut stream, &writer.take_segment());
    writer.serialize_parse_result(&b).unwrap();
    write_frame(&mut stream, &writer.take_segment());

    let mut reader = session(&language);
    read_frames(&mut reader, &stream);
    assert_matches(&a, &parse_result_at(&mut reader, 0));
    assert_matches(&b, &parse_result_at(&mut reader, 1));
    let c = parse(&language, DOC_C);
    let position: ParseResultIndex = reader.serialize_parse_result(&c).unwrap();
    assert_eq!(position.index(), 2);
    write_frame(&mut stream, &reader.take_segment());

    let mut third_reader = session(&language);
    read_frames(&mut third_reader, &stream);
    let final_a = parse_result_at(&mut third_reader, 0);
    let final_b = parse_result_at(&mut third_reader, 1);
    let final_c = parse_result_at(&mut third_reader, 2);
    assert_matches(&a, &final_a);
    assert_matches(&c, &final_c);
    assert!(Arc::ptr_eq(final_a.tree.root().parsing_state(), final_b.tree.root().parsing_state()));
    assert!(Arc::ptr_eq(&package_of(&final_a, "emph"), &package_of(&final_c, "textit")));

    // The binary and the JSON renderings of one stream carry the same segments.
    let json_stream = {
        let mut json = String::new();
        let mut w = session(&language);
        for result in [&a, &b] {
            w.serialize_parse_result(result).unwrap();
            write_line(&mut json, &w.take_segment());
        }
        json
    };
    let from_json: Vec<Segment> = json_stream.lines().map(|line| serde_json::from_str(line).unwrap()).collect();
    let from_postcard: Vec<Segment> = frames(&stream).into_iter().map(|f| postcard::from_bytes(f).unwrap()).collect();
    assert_eq!(from_json, from_postcard[..2]);
}
