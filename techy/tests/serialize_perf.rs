//! Performance sanity of the serialization engine (cargo feature `serde`) — a test,
//! not a benchmark: a large parse (a ~200 KB latexlike source) and a long stream (many
//! segments read then appended) go through the writer, the JSON rendering, and the
//! reader within a generous wall-clock ceiling, so that nothing quadratic in the
//! number of nodes, entries, or segments creeps into the engine (the interning map,
//! the segment directory checks, the memo journal, the tree reader's claim sweep).
//! The measured times print with `--nocapture`.

#![cfg(feature = "serde")]

use std::sync::Arc;
use std::time::{Duration, Instant};

use techy::core::specs::SpecsProvider;
use techy::core::{Language, ParseResult, ParsingState};
use techy::error::Recovery;
use techy::latexlike::minidefs::{self, minilatex_package};
use techy::latexlike::serialize::{register, register_package_recipes};
use techy::latexlike::{Latexlike, LatexlikeDriver};
use techy::serialize::{KnownProviders, ParseResultSerialization, SerdeSession, Segment, TableId};

/// The ceiling for each measured phase (debug builds are slow; the point is the
/// order of growth, not the constant).
const CEILING: Duration = Duration::from_secs(10);

fn language() -> Language<Latexlike> {
    let seed = ParsingState::lang_initial_with_packages([minilatex_package() as Arc<dyn SpecsProvider<Latexlike>>])
        .expect("seed state");
    Language::new(LatexlikeDriver::new(Recovery::Tolerant), seed)
}

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

/// Time `f`, print the phase, and assert the ceiling.
fn timed<T>(phase: &str, f: impl FnOnce() -> T) -> T {
    let start = Instant::now();
    let result = f();
    let elapsed = start.elapsed();
    eprintln!("perf: {phase}: {elapsed:?}");
    assert!(elapsed < CEILING, "{phase} took {elapsed:?}, more than {CEILING:?}");
    result
}

/// A latexlike paragraph of about 1 KB with macros, an environment, math, and a
/// comment; repeated to make a large source.
const PARAGRAPH: &str = "\
Lorem ipsum \\emph{dolor sit amet}, consectetur \\textbf{adipiscing} elit, sed do
eiusmod tempor incididunt ut labore et dolore magna aliqua~$x^2 + y_i$. Ut enim ad
minim veniam, quis nostrud exercitation ullamco laboris nisi ut aliquip ex ea commodo
consequat. % a comment on the paragraph
\\begin{itemize}
\\item Duis aute irure dolor in \\textit{reprehenderit} in voluptate velit esse cillum
\\item dolore eu fugiat nulla pariatur --- excepteur sint occaecat cupidatat non proident
\\item sunt in culpa qui officia deserunt mollit anim id est laborum $\\alpha_1$
\\end{itemize}
Sed ut perspiciatis unde omnis iste natus error sit voluptatem accusantium doloremque
laudantium, totam rem aperiam, eaque ipsa quae ab illo inventore veritatis et quasi
architecto beatae vitae dicta sunt explicabo. Nemo enim ipsam \\emph{voluptatem quia
voluptas} sit aspernatur aut odit aut fugit, sed quia consequuntur magni dolores.

";

/// A ~200 KB source: parsed, serialized (one segment), rendered to JSON and back,
/// absorbed, and read back — each phase within the ceiling, the tree the same size.
#[test]
fn a_large_parse_serializes_and_reads_back_in_linear_time() {
    let text = PARAGRAPH.repeat(200_000 / PARAGRAPH.len() + 1);
    assert!(text.len() >= 200_000);
    let language = language();
    let result = timed("parse 200 KB", || Arc::new(language.parse(&text).expect("a tolerant parse completes")));
    let node_count = result.tree.node_count();
    eprintln!("perf: {} nodes, {} diagnostics", node_count, result.diagnostics.len());
    assert!(node_count > 5_000, "the parse is large enough to measure: {node_count} nodes");

    let mut writer = session(&language);
    let segment = timed("serialize the parse result", || {
        let position = writer.serialize_parse_result(&result).expect("serializes");
        writer.take_segment_with_main(position).expect("the writer's own position")
    });
    let json = timed("render to JSON", || serde_json::to_string(&segment).expect("renders"));
    eprintln!("perf: JSON line of {} bytes", json.len());
    let back: Segment = timed("read from JSON", || serde_json::from_str(&json).expect("a segment"));
    assert_eq!(back, segment);

    let mut reader = session(&language);
    let main = timed("absorb the segment", || reader.push_segment(back).expect("absorbs"));
    let rebuilt = timed("read the parse result back", || {
        let (table, index) = main.expect("a main entry");
        let parse_results = reader.standard_tables().unwrap().parse_results;
        assert_eq!(table, parse_results.id());
        reader.parse_result(parse_results.position(index)).expect("reads back")
    });
    assert_eq!(rebuilt.tree.node_count(), node_count);
    assert_eq!(rebuilt.diagnostics.len(), result.diagnostics.len());
}

/// A long stream: one session writes 200 segments (one small parse each), a second
/// absorbs all 200 in order and appends 200 more of its own, and a third absorbs all
/// 400 — the whole within the ceiling per phase, and each appended segment carrying
/// only what is new (no provider re-emitted, one parse result each).
#[test]
fn a_long_stream_is_read_then_appended_in_linear_time() {
    const SEGMENTS: usize = 200;
    let language = language();
    let inputs: Vec<String> =
        (0..SEGMENTS).map(|i| format!("Line {i}: \\emph{{a{i}}} $x_{i}$ \\begin{{itemize}}\\item b{i}\\end{{itemize}}")).collect();
    let parses: Vec<Arc<ParseResult<Latexlike>>> = timed("parse 200 inputs", || {
        inputs.iter().map(|input| Arc::new(language.parse(input).expect("parses"))).collect()
    });

    let mut writer = session(&language);
    let stream: Vec<String> = timed("write 200 segments", || {
        parses
            .iter()
            .map(|result| {
                let position = writer.serialize_parse_result(result).unwrap();
                serde_json::to_string(&writer.take_segment_with_main(position).unwrap()).unwrap()
            })
            .collect()
    });

    let mut appender = session(&language);
    let mains: Vec<Option<(TableId, u32)>> = timed("absorb 200 segments", || {
        stream.iter().map(|line| appender.push_segment(serde_json::from_str(line).unwrap()).unwrap()).collect()
    });
    assert!(mains.iter().all(Option::is_some));
    let appended: Vec<String> = timed("append 200 segments", || {
        (0..SEGMENTS)
            .map(|i| {
                let result = Arc::new(language.parse(format!("Appended {i}: \\textbf{{c{i}}}")).unwrap());
                let position = appender.serialize_parse_result(&result).unwrap();
                let segment = appender.take_segment_with_main(position).unwrap();
                let providers = segment.tables().iter().find(|t| t.name() == "providers").unwrap();
                assert!(providers.entries().is_empty(), "the packages were absorbed, not re-emitted");
                let results = segment.tables().iter().find(|t| t.name() == "parse-results").unwrap();
                assert_eq!((results.entries().len(), results.start() as usize), (1, SEGMENTS + i));
                serde_json::to_string(&segment).unwrap()
            })
            .collect()
    });

    let mut reader = session(&language);
    timed("absorb 400 segments", || {
        for line in stream.iter().chain(&appended) {
            reader.push_segment(serde_json::from_str(line).unwrap()).unwrap();
        }
    });
    let parse_results = reader.standard_tables().unwrap().parse_results;
    let last = reader.parse_result(parse_results.position((2 * SEGMENTS - 1) as u32)).unwrap();
    assert!(last.tree.root().children().len() > 1);
    let first = reader.parse_result(parse_results.position(0)).unwrap();
    assert_eq!(first.diagnostics.len(), parses[0].diagnostics.len());
}
