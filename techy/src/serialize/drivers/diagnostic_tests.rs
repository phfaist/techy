//! Tests of the diagnostic and parse-result drivers (M6): diagnostic round trips of
//! every severity with a real derived condition and traceback frames, the
//! deserialized condition's contract (identifier, projection, message; no downcast to
//! the original type), sources shared between a tree and its diagnostics, parse-result
//! round trips (a toy language with a non-unit session ext; suppressed diagnostics;
//! identity interning), the embedding of `DiagnosticValue`, `Severity`'s value
//! conversions, and the hostile-input battery (bad severity, byte strings and table
//! positions inside a projection, spans out of range, references into the wrong table
//! or out of range, inconsistent counts, a parse result naming a tree of another
//! annotation type).

use alloc::string::{String, ToString};
use alloc::sync::Arc;
use alloc::vec;
use alloc::vec::Vec;

use crate::constructs::{UnclosedGroup, UnclosedGroupFound};
use crate::engine::{Language, ParseResult, StdParseDriver};
use crate::error::{
    Diagnostic, DiagnosticData, DiagnosticInfo, DiagnosticValue, Diagnostics, Recovery, Severity,
    TraceFrame,
};
use crate::node::{NodeBuildError, NodeKind};
use crate::serialize::tree_support::assert_trees_equivalent;
use crate::serialize::{
    DeserializableValue, DeserializeContext, DeserializeError, DeserializedCondition,
    DiagnosticSerialization, ParseResultSerialization, SerdeSession, Segment, SerialIndex,
    SerialValue, SerializableLang, SerializableValue, SerializeContext, SerializeError,
    StandardTableInterning, TableId, TreeSerialization,
};
use crate::source::{Source, SourceSpan};
use crate::state::{Lang, ParsingState, StateData};
use crate::token::{
    CommandRules, CommentRule, CommentRules, ForbiddenCharsRules, GroupRule, GroupRules,
    ParagraphRules, SpecialsRules, TokenRules, WhitespaceRules,
};

// --- a toy lang with a non-unit session ext -------------------------------------------

/// `{}` groups, `%` comments, whitespace; no commands (no resolver needed); a `u32`
/// session ext.
#[derive(Debug, Clone, Copy)]
struct DiagLang;

impl Lang for DiagLang {
    type Features = crate::state::AllLangFeatures;
    type GroupTypeId = u32;
    type CallableTypeId = u32;
    type ModeId = ();
    type StateExt = ();
    type Event = ();
    type SessionExt = u32;
    type SourceOrigin = Option<String>;
    type NodeExts = ();
    type InvocationSyntax = ();
    type Driver = StdParseDriver;

    fn initial_state_data() -> Result<StateData<Self>, crate::state::FinalizeError> {
        let rules = TokenRules {
            whitespace: WhitespaceRules { enabled: true, chars: " \t\n".into() },
            paragraphs: ParagraphRules { enabled: true },
            groups: GroupRules {
                enabled: true,
                rules: Vec::from([Arc::new(GroupRule { group_type: 0, open: "{".into(), close: "}".into() })]),
                temporary: Vec::new(),
                expecting_close: None,
            },
            commands: CommandRules { enabled: false, rules: Vec::new() },
            comments: CommentRules { enabled: true, rules: Vec::from([Arc::new(CommentRule { start: "%".into() })]) },
            specials: SpecialsRules { enabled: false },
            forbidden_chars: ForbiddenCharsRules { chars: "".into() },
        };
        Ok(StateData { rules, scopes: crate::scopes::ScopeStack::new(), mode: (), ext: () })
    }

    fn make_node_ext(
        _kind: &NodeKind<Self>,
        _span: &SourceSpan<Option<String>>,
        _state: &Arc<ParsingState<Self>>,
        _children: crate::node::StagedChildren<'_, Self>,
    ) -> Result<(), NodeBuildError> {
        Ok(())
    }
}

impl SerializableLang for DiagLang {}

fn language(recovery: Recovery) -> Language<DiagLang> {
    Language::new(StdParseDriver::new(recovery, ()), ParsingState::lang_initial().expect("seed state"))
}

fn setup() -> SerdeSession<DiagLang> {
    SerdeSession::<DiagLang>::new()
}

/// A tolerant parse of `input` (nothing in this toy language aborts).
fn parse(input: &str) -> ParseResult<DiagLang> {
    language(Recovery::Tolerant).parse(input).expect("a tolerant parse completes")
}

/// The segment as the reading side receives it: through JSON under the `serde`
/// feature, as is otherwise.
fn pass_through(segment: Segment) -> Segment {
    #[cfg(feature = "serde")]
    {
        let json = serde_json::to_string(&segment).expect("segment to JSON");
        serde_json::from_str(&json).expect("segment from JSON")
    }
    #[cfg(not(feature = "serde"))]
    {
        segment
    }
}

fn source(text: &str) -> Arc<Source<Option<String>>> {
    Arc::new(Source::new(text).with_origin(Some(String::from("t.tex"))))
}

/// A hand-built diagnostic of `severity` over `src`: an unclosed-group condition (a
/// derived condition with a two-field projection) at `1..2` with two traceback
/// frames.
fn unclosed_group_diagnostic(severity: Severity, src: &Arc<Source<Option<String>>>) -> Diagnostic {
    let condition = UnclosedGroup { expected_close: String::from("}"), found: UnclosedGroupFound::EndOfInput };
    let frames = vec![
        TraceFrame::new("group ‘{’", SourceSpan::new(src, 1..2)),
        TraceFrame::new("group ‘{’", SourceSpan::new(src, 0..1)),
    ];
    Diagnostic::from_parts(severity, alloc::boxed::Box::new(condition), SourceSpan::new(src, 1..2), frames)
}

/// Serialize `diagnostic` in a fresh session, pass the segment through, read it back
/// in a fresh session.
fn round_trip_diagnostic(diagnostic: &Diagnostic) -> Diagnostic {
    let mut writer = setup();
    let position = writer.serialize_diagnostic(diagnostic).unwrap();
    let segment = pass_through(writer.take_segment());
    let mut reader = setup();
    reader.push_segment(segment).unwrap();
    let diagnostics = reader.standard_tables().unwrap().diagnostics;
    reader.diagnostic(diagnostics.position(position.index())).unwrap()
}

/// The two diagnostics agree on everything the wire carries.
fn assert_diagnostics_equivalent(original: &Diagnostic, back: &Diagnostic) {
    assert_eq!(back.severity(), original.severity());
    assert_eq!(back.identifier(), original.identifier());
    assert_eq!(back.data().serializable_data(), original.data().serializable_data());
    assert_eq!(back.message(), original.message());
    assert_eq!(back.to_string(), original.to_string());
    assert_eq!(back.render(), original.render());
    assert_eq!((back.span().start(), back.span().end()), (original.span().start(), original.span().end()));
    assert_eq!(back.span().source().content(), original.span().source().content());
    assert_eq!(back.frames().len(), original.frames().len());
    for (a, b) in original.frames().iter().zip(back.frames()) {
        assert_eq!(a.title(), b.title());
        assert_eq!((a.span().start(), a.span().end()), (b.span().start(), b.span().end()));
        assert!(b.span().same_source(back.span()) == a.span().same_source(original.span()));
    }
}

// --- diagnostic round trips -------------------------------------------------------------

#[test]
fn a_diagnostic_of_each_severity_round_trips_as_a_deserialized_condition() {
    let src = source("{{x");
    for severity in [Severity::Note, Severity::Warning, Severity::Error] {
        let original = unclosed_group_diagnostic(severity, &src);
        let back = round_trip_diagnostic(&original);
        assert_diagnostics_equivalent(&original, &back);
        // The contract across the boundary: the identifier and the projection — not
        // the concrete condition type.
        assert_eq!(back.identifier(), "core.groups.unclosed-group");
        assert_eq!(
            back.data().serializable_data(),
            DiagnosticValue::Map(vec![
                (String::from("expected_close"), DiagnosticValue::Str(String::from("}"))),
                (String::from("found"), DiagnosticValue::Str(String::from("end-of-input"))),
            ])
        );
        assert!(back.data().downcast_ref::<UnclosedGroup>().is_none(), "the original type is not rebuilt");
        let condition = back.data().downcast_ref::<DeserializedCondition>().expect("the adapter type");
        assert_eq!(DiagnosticData::identifier(condition), "core.groups.unclosed-group");
        assert_eq!(condition.data(), &original.data().serializable_data());
        assert_eq!(condition.to_string(), original.message());
        // The adapter type's own identity stays the type's.
        assert_eq!(<DeserializedCondition as DiagnosticInfo>::IDENTIFIER, "core.serialization.deserialized-condition");
        // The frames survive innermost first.
        assert_eq!(back.frames()[0].span().start(), 1);
        assert_eq!(back.frames()[1].span().start(), 0);
    }
}

#[test]
fn a_diagnostic_from_a_real_tolerant_parse_round_trips() {
    let result = parse("a{b");
    assert_eq!(result.diagnostics.len(), 1, "{:?}", result.diagnostics);
    let original = &result.diagnostics.as_slice()[0];
    assert_eq!(original.identifier(), "core.groups.unclosed-group");
    // (A top-level group of this toy language is parsed without a traceback frame;
    // the preset's tests round-trip parses whose diagnostics carry frames.)
    let back = round_trip_diagnostic(original);
    assert_diagnostics_equivalent(original, &back);
}

#[test]
fn a_deserialized_diagnostic_is_found_by_identifier_but_not_by_condition_type() {
    let result = parse("a{b");
    let back = round_trip_diagnostic(&result.diagnostics.as_slice()[0]);
    let mut collection = Diagnostics::new();
    collection.push(back);
    assert_eq!(collection.with_identifier("core.groups.unclosed-group").count(), 1);
    assert_eq!(collection.conditions::<UnclosedGroup>().count(), 0);
    assert_eq!(collection.conditions::<DeserializedCondition>().count(), 1);
}

#[test]
fn diagnostics_share_their_sources_with_the_tree() {
    let result = parse("{a {b");
    assert_eq!(result.diagnostics.len(), 2, "{:?}", result.diagnostics);
    let mut writer = setup();
    let tree_position = writer.serialize_tree(&result.tree).unwrap();
    let positions: Vec<_> = result.diagnostics.iter().map(|d| writer.serialize_diagnostic(d).unwrap()).collect();
    assert_ne!(positions[0], positions[1], "a diagnostic is a value: one entry per call");
    let segment = writer.take_segment();
    let sources = segment.tables().iter().find(|t| t.name() == "sources").unwrap();
    assert_eq!(sources.entries().len(), 1, "one source, shared by the tree and both diagnostics");

    let mut reader = setup();
    reader.push_segment(pass_through(segment)).unwrap();
    let tables = reader.standard_tables().unwrap();
    let tree: crate::node::NodeTree<DiagLang> = reader.tree(tables.trees.position(tree_position.index())).unwrap();
    for position in positions {
        let diagnostic = reader.diagnostic(tables.diagnostics.position(position.index())).unwrap();
        assert!(diagnostic.span().same_source(tree.root().span()), "the very source Arc");
        for frame in diagnostic.frames() {
            assert!(frame.span().same_source(tree.root().span()));
        }
    }
}

#[test]
fn serializing_the_same_diagnostic_twice_writes_two_entries() {
    let src = source("{x");
    let diagnostic = unclosed_group_diagnostic(Severity::Error, &src);
    let mut writer = setup();
    let first = writer.serialize_diagnostic(&diagnostic).unwrap();
    let second = writer.serialize_diagnostic(&diagnostic).unwrap();
    assert_ne!(first, second);
    assert_eq!(first.index(), 0);
    assert_eq!(second.index(), 1);
}

// --- parse results ------------------------------------------------------------------------

/// Serialize `result` in a fresh session, pass the segment through, read it back in a
/// fresh session.
fn round_trip_parse_result(result: &Arc<ParseResult<DiagLang>>) -> Arc<ParseResult<DiagLang>> {
    let mut writer = setup();
    let position = writer.serialize_parse_result(result).unwrap();
    let segment = pass_through(writer.take_segment());
    let mut reader = setup();
    reader.push_segment(segment).unwrap();
    let parse_results = reader.standard_tables().unwrap().parse_results;
    reader.parse_result(parse_results.position(position.index())).unwrap()
}

fn assert_parse_results_equivalent(original: &ParseResult<DiagLang>, back: &ParseResult<DiagLang>) {
    assert_trees_equivalent(&original.tree, &back.tree, |(), ()| true);
    assert_eq!(back.session_ext, original.session_ext);
    assert_eq!(back.diagnostics.len(), original.diagnostics.len());
    assert_eq!(back.diagnostics.limit(), original.diagnostics.limit());
    assert_eq!(back.diagnostics.suppressed(), original.diagnostics.suppressed());
    assert_eq!(back.diagnostics.error_count(), original.diagnostics.error_count());
    assert_eq!(back.diagnostics.has_errors(), original.diagnostics.has_errors());
    assert_eq!(back.diagnostics.is_empty(), original.diagnostics.is_empty());
    for (a, b) in original.diagnostics.iter().zip(back.diagnostics.iter()) {
        assert_diagnostics_equivalent(a, b);
        // The diagnostic's source is the rebuilt tree's very source.
        assert!(b.span().same_source(back.tree.root().span()));
    }
    assert_eq!(back.diagnostics.render_all(), original.diagnostics.render_all());
}

#[test]
fn a_parse_result_with_diagnostics_and_a_session_ext_round_trips() {
    let mut result = parse("{a % note\n {b} c");
    assert!(result.diagnostics.has_errors());
    result.session_ext = 42;
    let result = Arc::new(result);
    let back = round_trip_parse_result(&result);
    assert_parse_results_equivalent(&result, &back);
    assert_eq!(back.session_ext, 42);
}

#[test]
fn a_clean_parse_result_round_trips() {
    let result = Arc::new(parse("plain {text} % and a comment\n"));
    assert!(result.diagnostics.is_empty());
    let back = round_trip_parse_result(&result);
    assert_parse_results_equivalent(&result, &back);
    assert_eq!(back.diagnostics.limit(), Diagnostics::<Option<String>>::DEFAULT_LIMIT);
}

#[test]
fn suppressed_diagnostics_are_counted_and_the_counts_survive() {
    let parsed = parse("xyz");
    let src = parsed.tree.root().span().source().clone();
    let mut diagnostics = Diagnostics::with_limit(1);
    diagnostics.push(unclosed_group_diagnostic(Severity::Error, &src));
    diagnostics.push(unclosed_group_diagnostic(Severity::Warning, &src));
    diagnostics.push(unclosed_group_diagnostic(Severity::Error, &src));
    assert_eq!((diagnostics.len(), diagnostics.suppressed(), diagnostics.error_count()), (1, 2, 2));
    let result = Arc::new(ParseResult { tree: parsed.tree, diagnostics, session_ext: 7 });
    let back = round_trip_parse_result(&result);
    assert_parse_results_equivalent(&result, &back);
    assert_eq!((back.diagnostics.len(), back.diagnostics.suppressed(), back.diagnostics.error_count()), (1, 2, 2));
    assert_eq!(back.diagnostics.limit(), 1);
    assert!(back.diagnostics.has_errors());
}

#[test]
fn a_parse_result_is_interned_by_identity_and_read_back_shared() {
    let result = Arc::new(parse("{a"));
    let other = Arc::new(parse("{a"));
    let mut writer = setup();
    let first = writer.serialize_parse_result(&result).unwrap();
    assert_eq!(writer.serialize_parse_result(&result).unwrap(), first, "the same Arc: the existing position");
    let second = writer.serialize_parse_result(&other).unwrap();
    assert_ne!(first, second);
    let segment = writer.take_segment();
    let table = |name: &str| segment.tables().iter().find(|t| t.name() == name).unwrap().entries().len();
    assert_eq!(table("parse-results"), 2);
    assert_eq!(table("trees"), 2, "one tree entry per parse result");
    assert_eq!(table("diagnostics"), 2);
    assert_eq!(table("sources"), 2, "two parses, two sources");

    let mut reader = setup();
    reader.push_segment(pass_through(segment)).unwrap();
    let parse_results = reader.standard_tables().unwrap().parse_results;
    let a = reader.parse_result(parse_results.position(0)).unwrap();
    let again = reader.parse_result(parse_results.position(0)).unwrap();
    assert!(Arc::ptr_eq(&a, &again), "the same position yields the same Arc");
    let b = reader.parse_result(parse_results.position(1)).unwrap();
    assert!(!Arc::ptr_eq(&a, &b));
}

#[test]
fn a_parse_result_serialized_through_the_general_intern_reads_back_through_the_sugar() {
    let result = Arc::new(parse("{"));
    let mut writer = setup();
    let handle = writer.standard_tables().unwrap().parse_results;
    let position = writer.intern(handle, &result).unwrap();
    let mut reader = setup();
    reader.push_segment(pass_through(writer.take_segment())).unwrap();
    let back = reader.parse_result(reader.standard_tables().unwrap().parse_results.position(position.index())).unwrap();
    assert_parse_results_equivalent(&result, &back);
}

// --- the value model pieces ---------------------------------------------------------------

#[test]
fn a_diagnostic_value_embeds_into_the_same_shaped_serial_value() {
    let value = DiagnosticValue::Map(vec![
        (String::from("n"), DiagnosticValue::Null),
        (String::from("b"), DiagnosticValue::Bool(true)),
        (String::from("i"), DiagnosticValue::Int(-3)),
        (String::from("s"), DiagnosticValue::Str(String::from("x"))),
        (String::from("l"), DiagnosticValue::List(vec![DiagnosticValue::Int(1), DiagnosticValue::empty_map()])),
    ]);
    let expected = SerialValue::Map(vec![
        (String::from("n"), SerialValue::Null),
        (String::from("b"), SerialValue::Bool(true)),
        (String::from("i"), SerialValue::Int(-3)),
        (String::from("s"), SerialValue::Str(String::from("x"))),
        (String::from("l"), SerialValue::List(vec![SerialValue::Int(1), SerialValue::Map(Vec::new())])),
    ]);
    assert_eq!(SerialValue::from(&value), expected);
    assert_eq!(SerialValue::from(value), expected);
}

#[test]
fn severity_converts_as_its_string() {
    let mut w = setup();
    let mut r = setup();
    use crate::engine::{DescentGuard, StdDescentGuard, StdDescentGuardInit};
    let mut gw = StdDescentGuard::init(&StdDescentGuardInit::default());
    let mut gr = StdDescentGuard::init(&StdDescentGuardInit::default());
    let mut wcx = SerializeContext::new(&mut w, &mut gw);
    let mut rcx = DeserializeContext::new(&mut r, &mut gr, None);
    for (severity, text) in [(Severity::Note, "note"), (Severity::Warning, "warning"), (Severity::Error, "error")] {
        let value = severity.serialize_value(&mut wcx).unwrap();
        assert_eq!(value, SerialValue::Str(String::from(text)));
        assert_eq!(<Severity as DeserializableValue<DiagLang>>::deserialize_value(&value, &mut rcx).unwrap(), severity);
    }
    let error = <Severity as DeserializableValue<DiagLang>>::deserialize_value(
        &SerialValue::Str(String::from("fatal")),
        &mut rcx,
    )
    .unwrap_err();
    assert!(matches!(error, DeserializeError::Value(crate::serialize::SerialValueError::UnknownVariant { .. })), "{error}");
}

#[cfg(feature = "serde")]
#[test]
fn a_diagnostic_value_renders_through_serde_as_its_embedding() {
    let value = DiagnosticValue::Map(vec![
        (String::from("expected_close"), DiagnosticValue::Str(String::from("}"))),
        (String::from("odd$"), DiagnosticValue::List(vec![DiagnosticValue::Int(1), DiagnosticValue::Null])),
        (String::from("flag"), DiagnosticValue::Bool(true)),
    ]);
    let json = serde_json::to_string(&value).unwrap();
    assert_eq!(json, r#"{"expected_close":"}","odd$":[1,null],"flag":true}"#);
    assert_eq!(json, serde_json::to_string(&SerialValue::from(&value)).unwrap());
    // And it reads back — through JSON, through the bridge, through a compact format.
    assert_eq!(serde_json::from_str::<DiagnosticValue>(&json).unwrap(), value);
    assert_eq!(crate::serialize::from_value::<DiagnosticValue>(&SerialValue::from(&value)).unwrap(), value);
    let bytes = postcard::to_allocvec(&value).unwrap();
    assert_eq!(postcard::from_bytes::<DiagnosticValue>(&bytes).unwrap(), value);
    // Only the kinds a DiagnosticValue holds: a byte string or a table position in
    // the rendering is an error naming its path.
    let error = serde_json::from_str::<DiagnosticValue>(r#"{"a":[0,{"$bytes":"AA=="}]}"#).unwrap_err();
    assert!(error.to_string().contains("kind `bytes` at `a[1]`"), "{error}");
    let error = serde_json::from_str::<DiagnosticValue>(r#"{"$index":[0,1]}"#).unwrap_err();
    assert!(error.to_string().contains("kind `index` at its root"), "{error}");
    assert!(crate::serialize::from_value::<DiagnosticValue>(&SerialValue::Bytes(vec![])).is_err());
    // The nesting bound of the value model applies (the value is read as a
    // `SerialValue` first): one level beyond it is refused, however it is encoded.
    let limit = SerialValue::MAX_NESTING_DEPTH;
    let text = format!("{}null{}", "[".repeat(limit + 1), "]".repeat(limit + 1));
    let error = serde_json::from_str::<DiagnosticValue>(&text).unwrap_err().to_string();
    assert!(error.contains("nests deeper than 64 levels"), "{error}");
    let text = format!("{}null{}", "[".repeat(limit), "]".repeat(limit));
    let at_bound = serde_json::from_str::<DiagnosticValue>(&text).unwrap();
    assert_eq!(SerialValue::from(&at_bound).nesting_depth(), limit);
}

// --- hostile input ------------------------------------------------------------------------

/// The failure under every location wrapper (`InEntry`, `InNode`).
fn innermost(error: &DeserializeError) -> &DeserializeError {
    match error {
        DeserializeError::InEntry { cause, .. } | DeserializeError::InNode { cause, .. } => innermost(cause),
        other => other,
    }
}

fn map_field_mut<'a>(value: &'a mut SerialValue, key: &str) -> &'a mut SerialValue {
    match value {
        SerialValue::Map(fields) => {
            &mut fields.iter_mut().find(|(k, _)| k == key).unwrap_or_else(|| panic!("no key `{key}`")).1
        }
        _ => panic!("not a map (expected key `{key}`)"),
    }
}

fn list_mut(value: &mut SerialValue) -> &mut Vec<SerialValue> {
    match value {
        SerialValue::List(items) => items,
        _ => panic!("not a list"),
    }
}

/// The entry list of the table named `name` in a segment's serial value.
fn table_entries_mut<'a>(segment: &'a mut SerialValue, name: &str) -> &'a mut Vec<SerialValue> {
    let table = list_mut(map_field_mut(segment, "tables"))
        .iter_mut()
        .find(|table| matches!(table, SerialValue::Map(fields)
            if fields.iter().any(|(k, v)| k == "name" && matches!(v, SerialValue::Str(n) if n == name))))
        .unwrap();
    list_mut(map_field_mut(table, "entries"))
}

/// Serialize `diagnostic` in a fresh writer, edit the segment value, and push it into a
/// fresh reader, returning the error.
fn push_edited_diagnostic(diagnostic: &Diagnostic, edit: impl FnOnce(&mut SerialValue)) -> DeserializeError {
    let mut writer = setup();
    writer.serialize_diagnostic(diagnostic).unwrap();
    let mut value = writer.take_segment().to_serial_value();
    edit(&mut value);
    let edited = Segment::from_serial_value(&value).expect("the edited segment still parses as a segment");
    let mut reader = setup();
    reader.push_segment(edited).unwrap_err()
}

/// Serialize `result` in a fresh writer, edit the segment value, and push it into a
/// fresh reader, returning the error.
fn push_edited_parse_result(
    result: &Arc<ParseResult<DiagLang>>,
    edit: impl FnOnce(&mut SerialValue),
) -> DeserializeError {
    let mut writer = setup();
    writer.serialize_parse_result(result).unwrap();
    let mut value = writer.take_segment().to_serial_value();
    edit(&mut value);
    let edited = Segment::from_serial_value(&value).expect("the edited segment still parses as a segment");
    let mut reader = setup();
    reader.push_segment(edited).unwrap_err()
}

fn sample_diagnostic() -> Diagnostic {
    unclosed_group_diagnostic(Severity::Error, &source("{{x"))
}

#[test]
fn an_unknown_severity_string_is_rejected() {
    let error = push_edited_diagnostic(&sample_diagnostic(), |segment| {
        let entry = &mut table_entries_mut(segment, "diagnostics")[0];
        *map_field_mut(entry, "severity") = SerialValue::Str(String::from("fatal"));
    });
    assert!(
        matches!(innermost(&error), DeserializeError::Value(crate::serialize::SerialValueError::UnknownVariant { name, .. }) if name == "fatal"),
        "{error}"
    );
    assert!(matches!(error, DeserializeError::InEntry { table: "diagnostics", index: 0, .. }), "{error}");
}

#[test]
fn a_byte_string_inside_the_projection_is_rejected() {
    let error = push_edited_diagnostic(&sample_diagnostic(), |segment| {
        let entry = &mut table_entries_mut(segment, "diagnostics")[0];
        *map_field_mut(map_field_mut(entry, "data"), "expected_close") = SerialValue::Bytes(vec![1, 2]);
    });
    assert!(
        matches!(innermost(&error), DeserializeError::UnrepresentableDiagnosticValue { kind: "bytes", path } if path == "expected_close"),
        "{error}"
    );
    assert!(error.to_string().contains("kind `bytes` at `expected_close`"), "{error}");
}

#[test]
fn a_table_position_nested_inside_the_projection_is_rejected() {
    let error = push_edited_diagnostic(&sample_diagnostic(), |segment| {
        let entry = &mut table_entries_mut(segment, "diagnostics")[0];
        *map_field_mut(map_field_mut(entry, "data"), "found") =
            SerialValue::List(vec![SerialValue::Null, SerialValue::Index { table: TableId::new(0), index: 0 }]);
    });
    assert!(
        matches!(innermost(&error), DeserializeError::UnrepresentableDiagnosticValue { kind: "index", path } if path == "found[1]"),
        "{error}"
    );
    // The projection itself: an empty path, spelled "at its root".
    let error = push_edited_diagnostic(&sample_diagnostic(), |segment| {
        let entry = &mut table_entries_mut(segment, "diagnostics")[0];
        *map_field_mut(entry, "data") = SerialValue::Bytes(vec![]);
    });
    assert!(
        matches!(innermost(&error), DeserializeError::UnrepresentableDiagnosticValue { kind: "bytes", path } if path.is_empty()),
        "{error}"
    );
    assert!(error.to_string().contains("kind `bytes` at its root"), "{error}");
}

#[test]
fn a_frame_span_out_of_range_is_rejected() {
    let error = push_edited_diagnostic(&sample_diagnostic(), |segment| {
        let entry = &mut table_entries_mut(segment, "diagnostics")[0];
        let frame = &mut list_mut(map_field_mut(entry, "frames"))[1];
        *map_field_mut(map_field_mut(frame, "span"), "end") = SerialValue::Int(99);
    });
    assert!(matches!(innermost(&error), DeserializeError::SpanOutOfBounds { start: 0, end: 99, len: 3 }), "{error}");
}

#[test]
fn a_diagnostic_span_into_the_wrong_table_is_rejected() {
    let error = push_edited_diagnostic(&sample_diagnostic(), |segment| {
        let entry = &mut table_entries_mut(segment, "diagnostics")[0];
        // Table 1 is the states table.
        *map_field_mut(map_field_mut(entry, "span"), "source") = SerialValue::Index { table: TableId::new(1), index: 0 };
    });
    assert!(matches!(innermost(&error), DeserializeError::WrongTable { expected: "sources", .. }), "{error}");
}

#[test]
fn a_diagnostic_span_beyond_its_source_is_rejected() {
    let error = push_edited_diagnostic(&sample_diagnostic(), |segment| {
        let entry = &mut table_entries_mut(segment, "diagnostics")[0];
        *map_field_mut(map_field_mut(entry, "span"), "start") = SerialValue::Int(3);
    });
    assert!(matches!(innermost(&error), DeserializeError::SpanOutOfBounds { start: 3, end: 2, .. }), "{error}");
}

#[test]
fn a_diagnostic_entry_of_the_wrong_shape_is_rejected() {
    let error = push_edited_diagnostic(&sample_diagnostic(), |segment| {
        let entry = &mut table_entries_mut(segment, "diagnostics")[0];
        *entry = SerialValue::Str(String::from("not a diagnostic"));
    });
    assert!(matches!(innermost(&error), DeserializeError::Value(crate::serialize::SerialValueError::TypeMismatch { .. })), "{error}");
}

#[test]
fn a_parse_result_listing_a_diagnostic_out_of_range_is_rejected() {
    let result = Arc::new(parse("{a"));
    let error = push_edited_parse_result(&result, |segment| {
        let entry = &mut table_entries_mut(segment, "parse-results")[0];
        list_mut(map_field_mut(map_field_mut(entry, "diagnostics"), "items"))[0] =
            SerialValue::Index { table: TableId::new(5), index: 7 };
    });
    assert!(matches!(innermost(&error), DeserializeError::IndexOutOfRange { table: "diagnostics", index: 7, len: 1 }), "{error}");
}

#[test]
fn a_parse_result_listing_a_diagnostic_of_the_wrong_table_is_rejected() {
    let result = Arc::new(parse("{a"));
    let error = push_edited_parse_result(&result, |segment| {
        let entry = &mut table_entries_mut(segment, "parse-results")[0];
        // Table 4 is the trees table.
        list_mut(map_field_mut(map_field_mut(entry, "diagnostics"), "items"))[0] =
            SerialValue::Index { table: TableId::new(4), index: 0 };
    });
    assert!(matches!(innermost(&error), DeserializeError::WrongTable { expected: "diagnostics", .. }), "{error}");
}

#[test]
fn a_parse_result_naming_a_tree_of_another_annotation_type_is_rejected() {
    let result = Arc::new(parse("{a"));
    let setup_annotated = || {
        let mut session = setup();
        let trees = session.standard_tables().unwrap().trees;
        trees.register_annotation::<String>(&mut session, "test.string").unwrap();
        session
    };
    let mut writer = setup_annotated();
    // Entry 0 of the trees table: a String-annotated tree; entry 1: the parse result's.
    let annotated = result.tree.annotate(|_| String::from("a"));
    writer.serialize_tree(&annotated).unwrap();
    writer.serialize_parse_result(&result).unwrap();
    let mut value = writer.take_segment().to_serial_value();
    let entry = &mut table_entries_mut(&mut value, "parse-results")[0];
    *map_field_mut(entry, "tree") = SerialValue::Index { table: TableId::new(4), index: 0 };
    let mut reader = setup_annotated();
    let error = reader.push_segment(Segment::from_serial_value(&value).unwrap()).unwrap_err();
    match innermost(&error) {
        DeserializeError::Failed { detail, .. } => {
            assert!(detail.contains("serialized under `test.string`"), "{detail}");
            assert!(detail.contains("(`core.tree`)"), "{detail}");
        }
        other => panic!("unexpected error: {other}"),
    }
    assert!(matches!(error, DeserializeError::InEntry { table: "parse-results", index: 0, .. }), "{error}");
}

#[test]
fn inconsistent_diagnostic_counts_are_rejected() {
    let result = Arc::new(parse("{a {b"));
    assert_eq!(result.diagnostics.len(), 2);
    let counts = |error: &DeserializeError| match innermost(error) {
        DeserializeError::InconsistentDiagnosticCounts { retained, retained_errors, limit, suppressed, error_count } => {
            (*retained, *retained_errors, *limit, *suppressed, *error_count)
        }
        other => panic!("unexpected error: {other}"),
    };
    let edit_counts = |limit: i64, suppressed: i64, error_count: i64| {
        push_edited_parse_result(&result, |segment| {
            let entry = &mut table_entries_mut(segment, "parse-results")[0];
            let diagnostics = map_field_mut(entry, "diagnostics");
            *map_field_mut(diagnostics, "limit") = SerialValue::Int(limit);
            *map_field_mut(diagnostics, "suppressed") = SerialValue::Int(suppressed);
            *map_field_mut(diagnostics, "error_count") = SerialValue::Int(error_count);
        })
    };
    // More retained than the cap.
    assert_eq!(counts(&edit_counts(1, 0, 2)), (2, 2, 1, 0, 2));
    // Suppressed pushes although the cap was not reached.
    assert_eq!(counts(&edit_counts(5, 1, 2)), (2, 2, 5, 1, 2));
    // Fewer errors in all than retained errors.
    assert_eq!(counts(&edit_counts(1000, 0, 1)), (2, 2, 1000, 0, 1));
    // More errors in all than retained errors plus suppressed pushes.
    assert_eq!(counts(&edit_counts(2, 3, 6)), (2, 2, 2, 3, 6));
    // Consistent variants of the same edits read fine: the cap reached with suppressed
    // errors, and a large cap (one that fits a `usize` on every target).
    let mut writer = setup();
    writer.serialize_parse_result(&result).unwrap();
    for (limit, suppressed, error_count) in [(2, 3, 5), (2, 3, 2), (1 << 20, 0, 2)] {
        let mut value = writer.take_segment().to_serial_value();
        {
            let entry = &mut table_entries_mut(&mut value, "parse-results")[0];
            let diagnostics = map_field_mut(entry, "diagnostics");
            *map_field_mut(diagnostics, "limit") = SerialValue::Int(limit);
            *map_field_mut(diagnostics, "suppressed") = SerialValue::Int(suppressed);
            *map_field_mut(diagnostics, "error_count") = SerialValue::Int(error_count);
        }
        let mut reader = setup();
        reader.push_segment(Segment::from_serial_value(&value).unwrap()).unwrap();
        let back = reader.parse_result(reader.standard_tables().unwrap().parse_results.position(0)).unwrap();
        assert_eq!(back.diagnostics.limit() as i64, limit);
        assert_eq!(back.diagnostics.suppressed() as i64, suppressed);
        assert_eq!(back.diagnostics.error_count() as i64, error_count);
        // Re-serialize for the next iteration.
        writer = setup();
        writer.serialize_parse_result(&result).unwrap();
    }
    // A negative count is not a count.
    let error = edit_counts(-1, 0, 2);
    assert!(matches!(innermost(&error), DeserializeError::Value(crate::serialize::SerialValueError::IntegerOutOfRange { .. })), "{error}");
}

#[test]
fn a_session_ext_of_the_wrong_shape_is_rejected() {
    let result = Arc::new(parse("x"));
    let error = push_edited_parse_result(&result, |segment| {
        let entry = &mut table_entries_mut(segment, "parse-results")[0];
        *map_field_mut(entry, "session_ext") = SerialValue::Str(String::from("not a u32"));
    });
    assert!(matches!(innermost(&error), DeserializeError::Value(crate::serialize::SerialValueError::TypeMismatch { .. })), "{error}");
}

#[test]
fn the_sugar_names_a_missing_table() {
    let mut empty = SerdeSession::<DiagLang>::empty();
    let diagnostic = sample_diagnostic();
    assert!(matches!(
        empty.serialize_diagnostic(&diagnostic),
        Err(SerializeError::UnknownTableName { name }) if name == "diagnostics"
    ));
    let result = Arc::new(parse("x"));
    assert!(matches!(
        empty.serialize_parse_result(&result),
        Err(SerializeError::UnknownTableName { name }) if name == "parse-results"
    ));
    let handles = setup().standard_tables().unwrap();
    assert!(matches!(
        empty.diagnostic(handles.diagnostics.position(0)),
        Err(DeserializeError::UnknownTableName { name }) if name == "diagnostics"
    ));
    assert!(matches!(
        empty.parse_result(handles.parse_results.position(0)),
        Err(DeserializeError::UnknownTableName { name }) if name == "parse-results"
    ));
    // The interning accessors are unaffected by the two new tables.
    let state = Arc::new(ParsingState::<DiagLang>::lang_initial().unwrap());
    assert!(matches!(empty.intern_state(&state), Err(SerializeError::UnknownTableName { .. })));
}

#[test]
fn two_sessions_emit_identical_segments_for_a_parse_result() {
    let result = Arc::new(parse("{a % c\n b"));
    let mut a = setup();
    let mut b = setup();
    a.serialize_parse_result(&result).unwrap();
    b.serialize_parse_result(&result).unwrap();
    assert_eq!(a.take_segment(), b.take_segment());
}

// --- under the serde feature: a pinned rendering ------------------------------------------

#[cfg(feature = "serde")]
#[test]
fn a_diagnostic_and_a_parse_result_have_a_pinned_json_rendering() {
    let mut result = parse("{a");
    result.session_ext = 3;
    let result = Arc::new(result);
    let mut writer = setup();
    writer.serialize_parse_result(&result).unwrap();
    let json = serde_json::to_string(&writer.take_segment()).unwrap();
    let expected = concat!(
        r#"{"version":1,"meta":{},"tables":["#,
        r#"{"name":"sources","table":0,"start":0,"entries":[{"origin":null,"provenance":"primary","line_number_offset":1,"column_number_offset":1,"text":{"embedded":"{a"}}]},"#,
        r#"{"name":"states","table":1,"start":0,"entries":["#,
        r#"{"token_rules":{"whitespace":{"enabled":true,"chars":" \t\n"},"paragraphs":{"enabled":true},"groups":{"enabled":true,"rules":[{"group_type":0,"open":"{","close":"}"}],"temporary":[]},"commands":{"enabled":false,"rules":[]},"comments":{"enabled":true,"rules":[{"start":"%"}]},"specials":{"enabled":false},"forbidden_chars":{"chars":""}},"mode":null,"ext":null,"scopes":[]},"#,
        r#"{"token_rules":{"whitespace":{"enabled":true,"chars":" \t\n"},"paragraphs":{"enabled":true},"groups":{"enabled":true,"rules":[{"group_type":0,"open":"{","close":"}"}],"temporary":[],"expecting_close":{"group_type":0,"open":"{","close":"}"}},"commands":{"enabled":false,"rules":[]},"comments":{"enabled":true,"rules":[{"start":"%"}]},"specials":{"enabled":false},"forbidden_chars":{"chars":""}},"mode":null,"ext":null,"scopes":[]}]},"#,
        r#"{"name":"specs","table":2,"start":0,"entries":[]},"#,
        r#"{"name":"providers","table":3,"start":0,"entries":[]},"#,
        r#"{"name":"trees","table":4,"start":0,"entries":[{"identifier":"core.tree","data":{"nodes":["#,
        r#"{"kind":"list","span":{"source":{"$index":[0,0]},"start":0,"end":2},"state":{"$index":[1,0]},"ext":null,"children":{"start":1,"end":2}},"#,
        r#"{"kind":{"group":{"group_type":0,"open":{"spanned":{"start":0,"end":1}},"close":{"owned":""}}},"span":{"source":{"$index":[0,0]},"start":0,"end":2},"state":{"$index":[1,0]},"ext":null,"children":{"start":2,"end":3}},"#,
        r#"{"kind":{"chars":{"content":{"spanned":{"start":1,"end":2}}}},"span":{"source":{"$index":[0,0]},"start":1,"end":2},"state":{"$index":[1,1]},"ext":null,"children":{"start":3,"end":3}}]}}]},"#,
        r#"{"name":"diagnostics","table":5,"start":0,"entries":[{"severity":"error","identifier":"core.groups.unclosed-group","message":"unclosed group: expected ‘}’ before end of input","data":{"expected_close":"}","found":"end-of-input"},"span":{"source":{"$index":[0,0]},"start":0,"end":1},"frames":[]}]},"#,
        r#"{"name":"parse-results","table":6,"start":0,"entries":[{"tree":{"$index":[4,0]},"diagnostics":{"items":[{"$index":[5,0]}],"limit":1000,"suppressed":0,"error_count":1},"session_ext":3}]}]}"#,
    );
    assert_eq!(json, expected);
}

// --- property: random diagnostics collections round-trip ---------------------------------

/// Random `Diagnostics` — a random retention limit, a random sequence of pushed
/// diagnostics of random severities, projections, spans, and frame counts (some
/// suppressed by the limit) — round-trip inside a parse result: the retained
/// diagnostics equivalent one by one, the limit and the counts (`suppressed`,
/// `error_count`) as pushed.
mod diagnostics_properties {
    use proptest::prelude::*;

    use super::*;

    fn severity() -> impl Strategy<Value = Severity> {
        prop_oneof![Just(Severity::Note), Just(Severity::Warning), Just(Severity::Error)]
    }

    /// One pushed diagnostic: severity, the projection's `expected_close` text, the
    /// span's start (within `xyz`), and the number of traceback frames.
    fn pushed() -> impl Strategy<Value = (Severity, String, usize, usize)> {
        (severity(), "[}\\]\\)a-c\u{e9}]{0,3}", 0..3usize, 0..3usize)
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(48))]

        #[test]
        fn a_random_diagnostics_collection_round_trips(
            limit in 0..6usize,
            pushes in proptest::collection::vec(pushed(), 0..10),
            session_ext in any::<u32>(),
        ) {
            let parsed = parse("xyz");
            let src = parsed.tree.root().span().source().clone();
            let mut diagnostics = Diagnostics::with_limit(limit);
            let mut errors = 0;
            for (severity, expected_close, start, frame_count) in &pushes {
                let condition = UnclosedGroup { expected_close: expected_close.clone(), found: UnclosedGroupFound::EndOfInput };
                let frames: Vec<TraceFrame<Option<String>>> = (0..*frame_count)
                    .map(|i| TraceFrame::new(alloc::format!("frame {i}"), SourceSpan::new(&src, i..i + 1)))
                    .collect();
                let span = SourceSpan::new(&src, *start..*start + 1);
                diagnostics.push(Diagnostic::from_parts(*severity, alloc::boxed::Box::new(condition), span, frames));
                if *severity == Severity::Error {
                    errors += 1;
                }
            }
            prop_assert_eq!(diagnostics.len(), pushes.len().min(limit));
            prop_assert_eq!(diagnostics.suppressed(), pushes.len().saturating_sub(limit));
            prop_assert_eq!(diagnostics.error_count(), errors);
            let result = Arc::new(ParseResult { tree: parsed.tree, diagnostics, session_ext });
            let back = round_trip_parse_result(&result);
            assert_parse_results_equivalent(&result, &back);
            prop_assert_eq!(back.diagnostics.limit(), limit);
            prop_assert_eq!(back.diagnostics.suppressed(), pushes.len().saturating_sub(limit));
            prop_assert_eq!(back.diagnostics.error_count(), errors);
        }
    }
}
