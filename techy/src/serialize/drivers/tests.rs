//! Tests of the standard-table drivers: sources (embedded and referenced text, digest
//! verification, provenance chains, span validation), states (every rules section,
//! mode/ext, scope stacks over toy providers, cache rebuilding, sharing), the
//! standard-tables constructor and the by-kind accessors, the value traits' core
//! impls, determinism, and — with the `serde` feature — a pinned JSON rendering.

use alloc::string::{String, ToString};
use alloc::sync::Arc;
use alloc::vec::Vec;

use crate::engine::{DescentGuard, StdDescentGuard, StdDescentGuardInit};
use crate::scopes::{CallableQuery, ProviderError, ScopeStack, SpecsProvider};
use crate::source::{Source, SourceProvenance, SourceSpan};
use crate::spec::CallableSpec;
use crate::state::{
    GroupOverrides, Lang, NoLangFeatures, ParsingState, ParsingStateDelta, StateData,
    TokenRulesOverrides, TrivialLang,
};
use crate::token::{
    CommandRule, CommandRules, CommentRule, CommentRules, ForbiddenCharsRules, GroupRule,
    GroupRules, ParagraphRules, SpecialsRules, TokenRules, WhitespaceRules,
};

use crate::serialize::{
    DeserializableObject, DeserializableValue, DeserializeContext, DeserializeError,
    ReferencedSource, SerdeSession, Segment, SerialEntry, SerialIndex, SerialValue,
    SerialValueError, SerializableLang, SerializableObject, SerializableValue,
    SerializeContext, SerializeError, SourceDigest, SourceSerdeDriver, SourceTextForm,
    SourceTextPolicy, SourceTextSupplier, StandardTableInterning, StandardTableReading,
    StateIndex,
};

// --- the toy langs --------------------------------------------------------------------

/// Every feature present, every type defaulted (`TrivialLang`): the empty
/// `SerializableLang` impl is all it needs.
#[derive(Debug, Clone, Copy)]
struct ToyLang;
impl TrivialLang for ToyLang {}
impl SerializableLang for ToyLang {}

/// Every feature absent: the target of the feature-mismatch tests.
#[derive(Debug, Clone, Copy)]
struct PlainLang;
impl Lang for PlainLang {
    type Features = NoLangFeatures;
    type GroupTypeId = u32;
    type CallableTypeId = u32;
    type ModeId = ();
    type StateExt = ();
    type Event = ();
    type SessionExt = ();
    type SourceOrigin = Option<String>;
    type NodeExts = ();
    type InvocationSyntax = ();
    type Driver = crate::engine::StdParseDriver;

    fn make_node_ext(
        _kind: &crate::node::NodeKind<Self>,
        _span: &SourceSpan<Option<String>>,
        _state: &Arc<ParsingState<Self>>,
        _children: crate::node::StagedChildren<'_, Self>,
    ) -> Result<(), crate::node::NodeBuildError> {
        Ok(())
    }
}
impl SerializableLang for PlainLang {}

// --- the toy provider (identity-resolved through the reading environment) -------------

/// A provider that defines nothing; serialized as its name and looked up by name in
/// the reading environment ([`ProviderEnvironment`], the session's user data).
#[derive(Debug)]
struct ToyProvider {
    name: String,
}

impl ToyProvider {
    fn new(name: &str) -> Arc<ToyProvider> {
        Arc::new(ToyProvider { name: String::from(name) })
    }
}

impl<L: Lang> SpecsProvider<L> for ToyProvider {
    fn name(&self) -> &str {
        &self.name
    }

    fn retrieve_spec(
        &self,
        _query: &CallableQuery<'_, '_, L>,
        _state: &ParsingState<L>,
    ) -> Result<Option<Arc<dyn CallableSpec<L>>>, ProviderError> {
        Ok(None)
    }
}

impl<L: Lang> SerializableObject<L> for ToyProvider {
    fn serialize_object(&self, _cx: &mut SerializeContext<'_, L>) -> Result<SerialEntry, SerializeError>
    where
        L: SerializableLang,
    {
        Ok(SerialEntry { identifier: "toy.provider".into(), data: SerialValue::Str(self.name.clone()) })
    }
}

/// The reading environment: the providers the reading program already holds.
#[derive(Default)]
struct ProviderEnvironment {
    providers: Vec<Arc<ToyProvider>>,
}

impl<L: SerializableLang> DeserializableObject<L> for ToyProvider {
    type Output = Arc<dyn SpecsProvider<L>>;

    fn deserialize_object(
        value: &SerialValue,
        cx: &mut DeserializeContext<'_, L>,
    ) -> Result<Self::Output, DeserializeError> {
        let SerialValue::Str(name) = value else {
            return Err(DeserializeError::failed("a toy provider is its name"));
        };
        let env = cx
            .user_data::<ProviderEnvironment>()
            .ok_or_else(|| DeserializeError::failed("no provider environment"))?;
        let provider = env
            .providers
            .iter()
            .find(|provider| provider.name == *name)
            .cloned()
            .ok_or_else(|| DeserializeError::failed(alloc::format!("no provider named `{name}`")))?;
        Ok(provider as Arc<dyn SpecsProvider<L>>)
    }
}

/// A session with the standard tables and the toy provider reader registered, over
/// the given reading environment.
fn session_with_environment<L: SerializableLang>(
    session: SerdeSession<L>,
    providers: &[Arc<ToyProvider>],
) -> SerdeSession<L> {
    let mut session = session;
    let tables = session.standard_tables().unwrap();
    tables.providers.register_type::<ToyProvider>(&mut session, "toy.provider", |provider| provider).unwrap();
    session.set_user_data(ProviderEnvironment { providers: providers.to_vec() });
    session
}

// --- the toy digest --------------------------------------------------------------------

/// The toy hash function: the sum of the bytes, as eight big-endian bytes.
fn toy_sum(text: &str) -> Vec<u8> {
    text.bytes().map(u64::from).sum::<u64>().to_be_bytes().to_vec()
}

/// A write policy referencing every source, with or without a `toy-sum` digest.
struct ReferenceAll {
    with_digest: bool,
}

impl SourceTextPolicy<Option<String>> for ReferenceAll {
    fn text_form(&self, source: &Source<Option<String>>) -> Result<SourceTextForm, SerializeError> {
        let digest = self.with_digest.then(|| SourceDigest::new("toy-sum", toy_sum(source.content())));
        Ok(SourceTextForm::Referenced { digest })
    }
}

/// A write policy that fails.
struct RefusingPolicy;

impl SourceTextPolicy<Option<String>> for RefusingPolicy {
    fn text_form(&self, _source: &Source<Option<String>>) -> Result<SourceTextForm, SerializeError> {
        Err(SerializeError::failed("no decision today"))
    }
}

/// A supplier answering from a table of texts by origin, verifying `toy-sum`
/// digests and refusing every other algorithm.
struct TextsByOrigin {
    texts: Vec<(String, String)>,
}

impl TextsByOrigin {
    fn new(texts: &[(&str, &str)]) -> Arc<TextsByOrigin> {
        Arc::new(TextsByOrigin {
            texts: texts.iter().map(|(origin, text)| (String::from(*origin), String::from(*text))).collect(),
        })
    }
}

impl SourceTextSupplier<Option<String>> for TextsByOrigin {
    fn source_text(&self, source: &ReferencedSource<Option<String>>) -> Result<String, DeserializeError> {
        let origin = source.origin().as_deref().ok_or_else(|| DeserializeError::failed("no origin"))?;
        self.texts
            .iter()
            .find(|(candidate, _)| candidate == origin)
            .map(|(_, text)| text.clone())
            .ok_or_else(|| DeserializeError::failed(alloc::format!("no text for `{origin}`")))
    }

    fn digest_matches(&self, digest: &SourceDigest, text: &str) -> Result<bool, DeserializeError> {
        match digest.algorithm.as_str() {
            "toy-sum" => Ok(toy_sum(text) == digest.bytes),
            other => Err(DeserializeError::failed(alloc::format!("unknown digest algorithm `{other}`"))),
        }
    }
}

// --- helpers ------------------------------------------------------------------------------

fn arc_source(text: &str, origin: &str) -> Arc<Source> {
    Arc::new(Source::new(text).with_origin(Some(String::from(origin))))
}

/// The innermost error under the session's location wrappers.
fn innermost(error: &DeserializeError) -> &DeserializeError {
    match error {
        DeserializeError::InEntry { cause, .. } => innermost(cause),
        other => other,
    }
}

fn innermost_write(error: &SerializeError) -> &SerializeError {
    match error {
        SerializeError::InTable { cause, .. } => innermost_write(cause),
        other => other,
    }
}

/// The entries of the named table in a segment.
fn entries<'a>(segment: &'a Segment, table: &str) -> &'a [SerialValue] {
    segment.tables().iter().find(|t| t.name() == table).unwrap().entries()
}

/// The entries of the named table, mutably (for crafting hostile segments).
fn entry_field_mut<'a>(value: &'a mut SerialValue, path: &[&str]) -> &'a mut SerialValue {
    let mut current = value;
    for key in path {
        current = match current {
            SerialValue::Map(fields) => &mut fields.iter_mut().find(|(k, _)| k == key).unwrap().1,
            _ => panic!("not a map at `{key}`"),
        };
    }
    current
}

/// A segment with the entry at `position` of `table` replaced by `edit`'s result.
fn edited_segment(segment: &Segment, table: &str, position: usize, edit: impl FnOnce(&mut SerialValue)) -> Segment {
    let mut value = segment.to_serial_value();
    let tables = entry_field_mut(&mut value, &["tables"]);
    let SerialValue::List(tables) = tables else { panic!() };
    let index = tables
        .iter()
        .position(|part| matches!(part, SerialValue::Map(fields) if fields.iter().any(|(k, v)| k == "name" && matches!(v, SerialValue::Str(name) if name == table))))
        .unwrap();
    let part = &mut tables[index];
    let SerialValue::List(entries) = entry_field_mut(part, &["entries"]) else { panic!() };
    edit(&mut entries[position]);
    Segment::from_serial_value(&value).unwrap()
}

// --- the standard tables ------------------------------------------------------------------

#[test]
fn new_registers_the_standard_tables_in_canonical_order() {
    let mut session = SerdeSession::<ToyLang>::new();
    let tables = session.standard_tables().unwrap();
    assert_eq!(tables.sources.id().ordinal(), 0);
    assert_eq!(tables.states.id().ordinal(), 1);
    assert_eq!(tables.specs.id().ordinal(), 2);
    assert_eq!(tables.providers.id().ordinal(), 3);
    assert_eq!(tables.trees.id().ordinal(), 4);
    assert_eq!(tables.diagnostics.id().ordinal(), 5);
    assert_eq!(tables.parse_results.id().ordinal(), 6);
    let segment = session.take_segment();
    let names: Vec<String> = segment.tables().iter().map(|t| String::from(t.name())).collect();
    assert_eq!(names, ["sources", "states", "specs", "providers", "trees", "diagnostics", "parse-results"]);
    // The default is the same session.
    assert!(SerdeSession::<ToyLang>::default().standard_tables().is_some());
}

#[test]
fn an_empty_session_has_no_standard_tables_and_the_accessors_say_so() {
    let mut session = SerdeSession::<ToyLang>::empty();
    assert!(session.standard_tables().is_none());
    let state = Arc::new(ParsingState::<ToyLang>::lang_initial().unwrap());
    assert!(matches!(
        session.intern_state(&state),
        Err(SerializeError::UnknownTableName { name }) if name == "states"
    ));
    let mut reader = SerdeSession::<ToyLang>::empty();
    let handle = SerdeSession::<ToyLang>::new().standard_tables().unwrap().states;
    assert!(matches!(
        reader.state(handle.position(0)),
        Err(DeserializeError::UnknownTableName { name }) if name == "states"
    ));
    // `table_handle` checks the driver type as well as the name.
    let mut other = SerdeSession::<ToyLang>::empty();
    other.register_table(crate::serialize::DispatchingSerdeDriver::<ToyLang, dyn SpecsProvider<ToyLang>, StateIndex>::new("states")).unwrap();
    assert!(other.table_handle::<crate::serialize::StateSerdeDriver<ToyLang>>("states").is_none());
    assert!(other.standard_tables().is_none());
}

#[test]
fn the_core_readers_register_on_the_specs_and_providers_tables_alone() {
    use crate::serialize::{register_core_readers, ProviderSerdeDriver, RegistrationError, SpecSerdeDriver};
    // A session composed by hand with just those two tables (no other standard
    // table) takes the readers.
    let mut session = SerdeSession::<ToyLang>::empty();
    session.register_table(SpecSerdeDriver::<ToyLang>::new("specs")).unwrap();
    session.register_table(ProviderSerdeDriver::<ToyLang>::new("providers")).unwrap();
    register_core_readers(&mut session).unwrap();
    // A second call is the duplicate it always was.
    assert!(matches!(register_core_readers(&mut session), Err(RegistrationError::DuplicateIdentifier { .. })));
    // The error names the table that is missing: providers here, specs there.
    let mut only_specs = SerdeSession::<ToyLang>::empty();
    only_specs.register_table(SpecSerdeDriver::<ToyLang>::new("specs")).unwrap();
    assert!(matches!(
        register_core_readers(&mut only_specs),
        Err(RegistrationError::UnknownTableName { name }) if name == "providers"
    ));
    let mut none = SerdeSession::<ToyLang>::empty();
    assert!(matches!(
        register_core_readers(&mut none),
        Err(RegistrationError::UnknownTableName { name }) if name == "specs"
    ));
}

// --- sources --------------------------------------------------------------------------

#[test]
fn an_embedded_source_round_trips_with_origin_and_offsets() {
    let mut writer = SerdeSession::<ToyLang>::new();
    let source = Arc::new(
        Source::new("Hello \u{e9}!").with_origin(Some("intro.tex".into())).with_line_column_number_offsets(0, 0),
    );
    let position = writer.intern_source(&source).unwrap();
    assert_eq!(writer.intern_source(&source).unwrap(), position, "interned once");
    let segment = writer.take_segment();
    assert_eq!(entries(&segment, "sources").len(), 1);

    let mut reader = SerdeSession::<ToyLang>::new();
    reader.push_segment(segment).unwrap();
    let back = reader.source(reader.standard_tables().unwrap().sources.position(position.index())).unwrap();
    assert_eq!(back.content(), "Hello \u{e9}!");
    assert_eq!(back.origin().as_deref(), Some("intro.tex"));
    assert!(back.provenance().is_primary());
    assert_eq!((back.line_number_offset(), back.column_number_offset()), (0, 0));
    // The same position yields the same Arc.
    let again = reader.source(reader.standard_tables().unwrap().sources.position(position.index())).unwrap();
    assert!(Arc::ptr_eq(&back, &again));
}

/// The line and column offsets read from the wire are not bounded — a `usize` on the
/// wire is an `i64` read with a range check on the reader's `usize` — and the line
/// index adds them with saturating arithmetic, so the largest offset a 32-bit reader
/// can hold (`u32::MAX`, valid on both widths) round-trips and answers positions
/// without overflow.
#[test]
fn extreme_line_column_offsets_round_trip_and_saturate() {
    let extreme = u32::MAX as usize;
    let mut writer = SerdeSession::<ToyLang>::new();
    let source = Arc::new(Source::new("ab\ncd").with_line_column_number_offsets(extreme, extreme));
    let position = writer.intern_source(&source).unwrap();
    let segment = writer.take_segment();
    let mut reader = SerdeSession::<ToyLang>::new();
    reader.push_segment(segment).unwrap();
    let back = reader.source(reader.standard_tables().unwrap().sources.position(position.index())).unwrap();
    assert_eq!((back.line_number_offset(), back.column_number_offset()), (extreme, extreme));
    let mut index = back.line_index();
    assert_eq!(index.line_col(4), Some((extreme.saturating_add(1), extreme.saturating_add(1))));
    // A wire integer that does not fit the reader's `usize` (a 64-bit writer's value
    // above `u32::MAX` on a 32-bit reader; on any reader, a negative one) is a typed
    // range error, never a truncation.
    let mut writer = SerdeSession::<ToyLang>::new();
    writer.intern_source(&source).unwrap();
    let hostile = edited_segment(&writer.take_segment(), "sources", 0, |entry| {
        *entry_field_mut(entry, &["line_number_offset"]) = SerialValue::Int(-1);
    });
    let mut reader = SerdeSession::<ToyLang>::new();
    let error = reader.push_segment(hostile).unwrap_err();
    assert!(
        matches!(innermost(&error), DeserializeError::Value(SerialValueError::IntegerOutOfRange { target: "usize", .. })),
        "{error:?}"
    );
}

#[test]
fn provenance_chains_are_source_references_and_identity_survives() {
    // A (primary) <- B (resolved, triggered in A) <- C (synthesized, triggered in B);
    // D (resolved, also triggered in A) — A is shared.
    let a = arc_source("input{b} and more", "a.tex");
    let b = Arc::new(Source::resolved("bee", "b.tex", SourceSpan::new(&a, 0..8)).with_origin(Some("b.tex".into())));
    let c = Arc::new(Source::synthesized("sea", "macro expansion", SourceSpan::new(&b, 1..3)));
    let d = Arc::new(Source::resolved("dee", "d.tex", SourceSpan::new(&a, 13..17)));

    let mut writer = SerdeSession::<ToyLang>::new();
    let c_position = writer.intern_source(&c).unwrap();
    let d_position = writer.intern_source(&d).unwrap();
    // Post-order: A, B, C, D.
    assert_eq!((c_position.index(), d_position.index()), (2, 3));
    let segment = writer.take_segment();
    assert_eq!(entries(&segment, "sources").len(), 4);

    let mut reader = SerdeSession::<ToyLang>::new();
    reader.push_segment(segment).unwrap();
    let sources = reader.standard_tables().unwrap().sources;
    let c_back = reader.source(sources.position(2)).unwrap();
    let d_back = reader.source(sources.position(3)).unwrap();
    let SourceProvenance::Synthesized { description, triggered_at: c_at } = c_back.provenance() else {
        panic!("C is synthesized");
    };
    assert_eq!(description, "macro expansion");
    assert_eq!((c_at.start(), c_at.end()), (1, 3));
    assert_eq!(c_at.content(), "ee");
    let b_back = c_at.source();
    assert_eq!(b_back.origin().as_deref(), Some("b.tex"));
    let SourceProvenance::Resolved { reference, triggered_at: b_at } = b_back.provenance() else {
        panic!("B is resolved");
    };
    assert_eq!(reference, "b.tex");
    let SourceProvenance::Resolved { triggered_at: d_at, .. } = d_back.provenance() else {
        panic!("D is resolved");
    };
    // B's and D's triggering spans point into the one A: identity preserved.
    assert!(b_at.same_source(d_at));
    assert!(Arc::ptr_eq(b_at.source(), &reader.source(sources.position(0)).unwrap()));
    assert!(Arc::ptr_eq(b_back, &reader.source(sources.position(1)).unwrap()));
    // The chain iterators work over the rebuilt sources.
    assert_eq!(c_back.including_sources().count(), 3);
    assert!(c_back.provenance_chain().last().unwrap().is_primary());
}

#[test]
fn a_referenced_source_round_trips_through_a_supplier_with_a_digest() {
    let text = "Referenced \u{e9} text.";
    let source = arc_source(text, "chapter.tex");
    for with_digest in [false, true] {
        let mut writer = SerdeSession::<ToyLang>::with_source_driver(
            SourceSerdeDriver::new().with_text_policy(Arc::new(ReferenceAll { with_digest })),
        );
        let position = writer.intern_source(&source).unwrap();
        let segment = writer.take_segment();
        // The entry carries no text: the referenced form with the length (and digest).
        let entry = &entries(&segment, "sources")[0];
        let SerialValue::Map(fields) = entry else { panic!() };
        let (_, text_field) = fields.iter().find(|(k, _)| k == "text").unwrap();
        let SerialValue::Map(form) = text_field else { panic!() };
        assert_eq!(form[0].0, "referenced");
        let SerialValue::Map(reference) = &form[0].1 else { panic!() };
        assert_eq!(reference[0], (String::from("length"), SerialValue::Int(text.len() as i64)));
        assert_eq!(reference.len(), if with_digest { 2 } else { 1 });

        let mut reader = SerdeSession::<ToyLang>::with_source_driver(
            SourceSerdeDriver::new().with_text_supplier(TextsByOrigin::new(&[("chapter.tex", text)])),
        );
        reader.push_segment(segment).unwrap();
        let back = reader.source(reader.standard_tables().unwrap().sources.position(position.index())).unwrap();
        assert_eq!(back.content(), text);
        assert_eq!(back.origin().as_deref(), Some("chapter.tex"));
    }
}

#[test]
fn the_supplier_receives_everything_the_entry_records() {
    struct Recording;
    impl SourceTextSupplier<Option<String>> for Recording {
        fn source_text(&self, source: &ReferencedSource<Option<String>>) -> Result<String, DeserializeError> {
            // The parent (referenced too, interned first) is answered plainly.
            if source.origin().as_deref() == Some("parent.tex") {
                assert!(source.provenance().is_primary());
                return Ok(String::from("abc"));
            }
            assert_eq!(source.origin().as_deref(), Some("x.tex"));
            assert_eq!(source.length(), 3);
            assert_eq!(source.digest().unwrap().algorithm, "toy-sum");
            assert_eq!((source.line_number_offset(), source.column_number_offset()), (5, 6));
            let SourceProvenance::Resolved { reference, triggered_at } = source.provenance() else {
                panic!("resolved");
            };
            assert_eq!(reference, "x");
            assert_eq!(triggered_at.content(), "b");
            Ok(String::from("abc"))
        }
        fn digest_matches(&self, _digest: &SourceDigest, _text: &str) -> Result<bool, DeserializeError> {
            Ok(true)
        }
    }
    let parent = arc_source("abc", "parent.tex");
    let child = Arc::new(
        Source::resolved("abc", "x", SourceSpan::new(&parent, 1..2))
            .with_origin(Some("x.tex".into()))
            .with_line_column_number_offsets(5, 6),
    );
    let mut writer = SerdeSession::<ToyLang>::with_source_driver(
        SourceSerdeDriver::new().with_text_policy(Arc::new(ReferenceAll { with_digest: true })),
    );
    writer.intern_source(&child).unwrap();
    let mut reader = SerdeSession::<ToyLang>::with_source_driver(
        SourceSerdeDriver::new()
            .with_text_supplier(Arc::new(Recording))
    );
    reader.push_segment(writer.take_segment()).unwrap();
    let back = reader.source(reader.standard_tables().unwrap().sources.position(1)).unwrap();
    assert_eq!(back.content(), "abc");
    assert_eq!((back.line_number_offset(), back.column_number_offset()), (5, 6));
}

#[test]
fn referenced_source_failures_are_typed_and_name_the_source() {
    let source = arc_source("Hello", "h.tex");
    let segment_with = |with_digest: bool| {
        let mut writer = SerdeSession::<ToyLang>::with_source_driver(
            SourceSerdeDriver::new().with_text_policy(Arc::new(ReferenceAll { with_digest })),
        );
        writer.intern_source(&source).unwrap();
        writer.take_segment()
    };
    let reader_with = |supplier: Arc<dyn SourceTextSupplier<Option<String>>>| {
        SerdeSession::<ToyLang>::with_source_driver(SourceSerdeDriver::new().with_text_supplier(supplier))
    };

    // No supplier configured.
    let mut reader = SerdeSession::<ToyLang>::new();
    let error = reader.push_segment(segment_with(false)).unwrap_err();
    assert!(matches!(
        innermost(&error),
        DeserializeError::NoSourceTextSupplier { origin: Some(origin) } if origin == "h.tex"
    ), "{error}");
    assert!(error.to_string().contains("`h.tex`"), "{error}");
    // The failed push left the session usable and empty.
    assert!(reader.source(reader.standard_tables().unwrap().sources.position(0)).is_err());

    // Length mismatch (a changed file).
    let mut reader = reader_with(TextsByOrigin::new(&[("h.tex", "Hello!")]));
    let error = reader.push_segment(segment_with(false)).unwrap_err();
    assert!(matches!(
        innermost(&error),
        DeserializeError::SourceLengthMismatch { origin: Some(origin), expected: 5, found: 6 } if origin == "h.tex"
    ), "{error}");

    // Digest mismatch (same length, different text).
    let mut reader = reader_with(TextsByOrigin::new(&[("h.tex", "Hellp")]));
    let error = reader.push_segment(segment_with(true)).unwrap_err();
    assert!(matches!(
        innermost(&error),
        DeserializeError::SourceDigestMismatch { origin: Some(origin), algorithm } if origin == "h.tex" && algorithm == "toy-sum"
    ), "{error}");

    // The supplier cannot obtain the text.
    let mut reader = reader_with(TextsByOrigin::new(&[]));
    let error = reader.push_segment(segment_with(false)).unwrap_err();
    assert!(matches!(innermost(&error), DeserializeError::Failed { detail, .. } if detail.contains("h.tex")), "{error}");

    // The supplier refuses the algorithm.
    let refusing = segment_with(true);
    let refusing = edited_segment(&refusing, "sources", 0, |entry| {
        *entry_field_mut(entry, &["text", "referenced", "digest", "algorithm"]) = SerialValue::Str("sha-9000".into());
    });
    let mut reader = reader_with(TextsByOrigin::new(&[("h.tex", "Hello")]));
    let error = reader.push_segment(refusing).unwrap_err();
    assert!(matches!(innermost(&error), DeserializeError::Failed { detail, .. } if detail.contains("sha-9000")), "{error}");

    // The location wrapper names the table and entry.
    assert!(matches!(&error, DeserializeError::InEntry { table: "sources", index: 0, .. }));
}

#[test]
fn a_failing_write_policy_fails_the_interning() {
    let mut writer =
        SerdeSession::<ToyLang>::with_source_driver(SourceSerdeDriver::new().with_text_policy(Arc::new(RefusingPolicy)));
    let error = writer.intern_source(&arc_source("x", "x.tex")).unwrap_err();
    assert!(matches!(innermost_write(&error), SerializeError::Failed { detail, .. } if detail == "no decision today"));
    assert!(matches!(&error, SerializeError::InTable { table: "sources", .. }));
}

#[test]
fn hostile_spans_and_shapes_are_rejected() {
    let a = arc_source("ab\u{e9}cd", "a.tex"); // 'é' at bytes 2..4
    let b = Arc::new(Source::resolved("x", "b", SourceSpan::new(&a, 0..2)));
    let mut writer = SerdeSession::<ToyLang>::new();
    writer.intern_source(&b).unwrap();
    let segment = writer.take_segment();

    let push = |segment: Segment| SerdeSession::<ToyLang>::new().push_segment(segment).unwrap_err();

    // Out of bounds: end beyond the source.
    let error = push(edited_segment(&segment, "sources", 1, |entry| {
        *entry_field_mut(entry, &["provenance", "resolved", "triggered_at", "end"]) = SerialValue::Int(99);
    }));
    assert!(matches!(innermost(&error), DeserializeError::SpanOutOfBounds { start: 0, end: 99, len: 6 }), "{error}");
    // Out of bounds: start > end.
    let error = push(edited_segment(&segment, "sources", 1, |entry| {
        *entry_field_mut(entry, &["provenance", "resolved", "triggered_at", "start"]) = SerialValue::Int(3);
    }));
    assert!(matches!(innermost(&error), DeserializeError::SpanOutOfBounds { start: 3, end: 2, len: 6 }), "{error}");
    // Inside a character.
    let error = push(edited_segment(&segment, "sources", 1, |entry| {
        *entry_field_mut(entry, &["provenance", "resolved", "triggered_at", "end"]) = SerialValue::Int(3);
    }));
    assert!(matches!(innermost(&error), DeserializeError::SpanNotOnCharBoundary { start: 0, end: 3 }), "{error}");
    // A negative offset does not fit `usize`.
    let error = push(edited_segment(&segment, "sources", 1, |entry| {
        *entry_field_mut(entry, &["provenance", "resolved", "triggered_at", "start"]) = SerialValue::Int(-1);
    }));
    assert!(matches!(innermost(&error), DeserializeError::Value(SerialValueError::IntegerOutOfRange { .. })), "{error}");
    // The span's source position points beyond the table.
    let error = push(edited_segment(&segment, "sources", 1, |entry| {
        *entry_field_mut(entry, &["provenance", "resolved", "triggered_at", "source"]) =
            SerialValue::Index { table: crate::serialize::TableId::new(0), index: 7 };
    }));
    assert!(matches!(innermost(&error), DeserializeError::IndexOutOfRange { table: "sources", index: 7, .. }), "{error}");
    // …or into the wrong table.
    let error = push(edited_segment(&segment, "sources", 1, |entry| {
        *entry_field_mut(entry, &["provenance", "resolved", "triggered_at", "source"]) =
            SerialValue::Index { table: crate::serialize::TableId::new(1), index: 0 };
    }));
    assert!(matches!(innermost(&error), DeserializeError::WrongTable { expected: "sources", .. }), "{error}");
    // Bad shape: the text form is neither embedded nor referenced.
    let error = push(edited_segment(&segment, "sources", 0, |entry| {
        *entry_field_mut(entry, &["text"]) = SerialValue::Str("Hello".into());
    }));
    assert!(matches!(innermost(&error), DeserializeError::Value(SerialValueError::UnknownVariant { .. })), "{error}");
    // Bad shape: an unknown key.
    let error = push(edited_segment(&segment, "sources", 0, |entry| {
        let SerialValue::Map(fields) = entry else { panic!() };
        fields.push((String::from("mystery"), SerialValue::Null));
    }));
    assert!(matches!(innermost(&error), DeserializeError::Value(SerialValueError::UnknownField { .. })), "{error}");
    // Bad shape: not a map at all.
    let error = push(edited_segment(&segment, "sources", 0, |entry| *entry = SerialValue::Int(1)));
    assert!(matches!(innermost(&error), DeserializeError::Value(SerialValueError::TypeMismatch { .. })), "{error}");
}

#[test]
fn a_span_is_a_value_that_interns_its_source() {
    let source = arc_source("hello world", "s.tex");
    let (first, second) = (SourceSpan::new(&source, 0..5), SourceSpan::new(&source, 6..11));
    let mut writer = SerdeSession::<ToyLang>::new();
    let init = StdDescentGuardInit::default();
    let (first_value, second_value) = {
        let mut guard = StdDescentGuard::init(&init);
        let mut cx = SerializeContext::new(&mut writer, &mut guard);
        (first.serialize_value(&mut cx).unwrap(), second.serialize_value(&mut cx).unwrap())
    };
    let segment = writer.take_segment();
    assert_eq!(entries(&segment, "sources").len(), 1, "one source, two spans");

    let mut reader = SerdeSession::<ToyLang>::new();
    reader.push_segment(segment).unwrap();
    let (first_back, second_back) = {
        let mut guard = StdDescentGuard::init(&init);
        let mut cx = DeserializeContext::new(&mut reader, &mut guard, None);
        (
            SourceSpan::<Option<String>>::deserialize_value(&first_value, &mut cx).unwrap(),
            SourceSpan::<Option<String>>::deserialize_value(&second_value, &mut cx).unwrap(),
        )
    };
    assert_eq!(first_back.content(), "hello");
    assert_eq!(second_back.content(), "world");
    assert!(first_back.same_source(&second_back));
    assert!(Arc::ptr_eq(first_back.source(), &reader.source(reader.standard_tables().unwrap().sources.position(0)).unwrap()));
}

// --- states -----------------------------------------------------------------------------

fn brace_rule() -> Arc<GroupRule<ToyLang>> {
    Arc::new(GroupRule { group_type: 1, open: "{".into(), close: "}".into() })
}

fn bracket_rule() -> Arc<GroupRule<ToyLang>> {
    Arc::new(GroupRule { group_type: 2, open: "[".into(), close: "]".into() })
}

/// Rules with every section populated; the temporary rule is the expected close.
fn full_rules() -> TokenRules<ToyLang> {
    let temporary = bracket_rule();
    TokenRules {
        whitespace: WhitespaceRules { enabled: true, chars: " \t\n".into() },
        paragraphs: ParagraphRules { enabled: true },
        groups: GroupRules {
            enabled: true,
            rules: Vec::from([brace_rule(), Arc::new(GroupRule { group_type: 3, open: "$".into(), close: "$".into() })]),
            temporary: Vec::from([Arc::clone(&temporary)]),
            expecting_close: Some(temporary),
        },
        commands: CommandRules {
            enabled: true,
            rules: Vec::from([
                Arc::new(CommandRule { escape_char: '\\', name_chars: "abcdefghijklmnopqrstuvwxyz".into() }),
                Arc::new(CommandRule { escape_char: '\u{e9}', name_chars: "".into() }),
            ]),
        },
        comments: CommentRules { enabled: false, rules: Vec::from([Arc::new(CommentRule { start: "%".into() })]) },
        specials: SpecialsRules { enabled: true },
        forbidden_chars: ForbiddenCharsRules { chars: "\u{0}\u{7f}".into() },
    }
}

fn state_with(rules: TokenRules<ToyLang>, providers: &[Arc<ToyProvider>]) -> Arc<ParsingState<ToyLang>> {
    let mut scopes = ScopeStack::new();
    for provider in providers {
        scopes.push(Arc::clone(provider) as Arc<dyn SpecsProvider<ToyLang>>);
    }
    Arc::new(ParsingState::new(StateData { rules, scopes, mode: (), ext: () }))
}

#[test]
fn a_full_state_round_trips_with_caches_rebuilt_and_providers_resolved() {
    let providers = [ToyProvider::new("base"), ToyProvider::new("user")];
    let state = state_with(full_rules(), &providers);
    let mut writer = session_with_environment(SerdeSession::<ToyLang>::new(), &providers);
    let position = writer.intern_state(&state).unwrap();
    assert_eq!(writer.intern_state(&state).unwrap(), position);
    let segment = writer.take_segment();
    assert_eq!(entries(&segment, "states").len(), 1);
    assert_eq!(entries(&segment, "providers").len(), 2);
    assert!(entries(&segment, "sources").is_empty());

    let mut reader = session_with_environment(SerdeSession::<ToyLang>::new(), &providers);
    reader.push_segment(segment).unwrap();
    let states = reader.standard_tables().unwrap().states;
    let back = reader.state(states.position(position.index())).unwrap();
    // The rules are equal in every section.
    assert_eq!(*back.rules(), *state.rules());
    // The expected close is re-linked to the rebuilt temporary rule's handle.
    let expecting = back.rules().expecting_group_close().unwrap();
    assert!(Arc::ptr_eq(expecting, &back.rules().temporary_group_rules()[0]));
    // Providers are the environment's own Arcs, outermost first.
    let scopes = back.scopes().providers();
    assert_eq!(scopes.len(), 2);
    assert!(Arc::ptr_eq(&(Arc::clone(&providers[0]) as Arc<dyn SpecsProvider<ToyLang>>), &scopes[0]));
    assert!(Arc::ptr_eq(&(Arc::clone(&providers[1]) as Arc<dyn SpecsProvider<ToyLang>>), &scopes[1]));
    // The derived caches are present and rebuilt (groups and specials present).
    let table = back.prefix_table().unwrap();
    assert!(!table.entries().is_empty());
    assert!(table.match_at("[x").is_some());
    assert!(back.trigger_chars().is_some());
    // The same position yields the same state.
    assert!(Arc::ptr_eq(&back, &reader.state(states.position(position.index())).unwrap()));
    // The rebuilt state derives further, and the temporary scope rule works: descending
    // into a brace group drops the temporary rule.
    let brace = Arc::clone(&back.rules().group_rules()[0]);
    let delta = ParsingStateDelta::new().rules(TokenRulesOverrides {
        groups: GroupOverrides { expecting_close: Some(Some(brace)), ..GroupOverrides::default() },
        ..TokenRulesOverrides::default()
    });
    let derived = back.derived(&delta).unwrap();
    assert!(derived.rules().temporary_group_rules().is_empty());
}

#[test]
fn an_empty_and_a_seed_state_round_trip() {
    let seed = Arc::new(ParsingState::<ToyLang>::lang_initial().unwrap());
    let mut writer = SerdeSession::<ToyLang>::new();
    let position = writer.intern_state(&seed).unwrap();
    let mut reader = SerdeSession::<ToyLang>::new();
    reader.push_segment(writer.take_segment()).unwrap();
    let back = reader.state(reader.standard_tables().unwrap().states.position(position.index())).unwrap();
    assert_eq!(*back.rules(), TokenRules::empty());
    assert!(back.scopes().is_empty());
    assert!(back.prefix_table().unwrap().entries().is_empty());
}

#[test]
fn shared_states_and_providers_keep_their_identity() {
    let providers = [ToyProvider::new("shared")];
    let base = state_with(full_rules(), &providers);
    let first = Arc::new(base.derived(&ParsingStateDelta::new()).unwrap());
    let second = Arc::new(base.derived(&ParsingStateDelta::new()).unwrap());
    let mut writer = session_with_environment(SerdeSession::<ToyLang>::new(), &providers);
    let p_first = writer.intern_state(&first).unwrap();
    let p_second = writer.intern_state(&second).unwrap();
    let p_first_again = writer.intern_state(&first).unwrap();
    assert_eq!(p_first, p_first_again);
    assert_ne!(p_first, p_second);
    let segment = writer.take_segment();
    // Both states name the one provider entry.
    assert_eq!(entries(&segment, "providers").len(), 1);

    let mut reader = session_with_environment(SerdeSession::<ToyLang>::new(), &providers);
    reader.push_segment(segment).unwrap();
    let states = reader.standard_tables().unwrap().states;
    let first_back = reader.state(states.position(p_first.index())).unwrap();
    let second_back = reader.state(states.position(p_second.index())).unwrap();
    assert!(!Arc::ptr_eq(&first_back, &second_back));
    assert!(Arc::ptr_eq(&first_back, &reader.state(states.position(p_first.index())).unwrap()));
    assert!(Arc::ptr_eq(&first_back.scopes().providers()[0], &second_back.scopes().providers()[0]));
}

#[test]
fn a_section_for_an_absent_feature_is_refused() {
    // Written by the all-features lang, read by the no-features lang.
    let state = state_with(TokenRules { whitespace: WhitespaceRules { enabled: true, chars: " ".into() }, ..TokenRules::empty() }, &[]);
    let mut writer = SerdeSession::<ToyLang>::new();
    writer.intern_state(&state).unwrap();
    let segment = writer.take_segment();

    let mut reader = SerdeSession::<PlainLang>::new();
    let error = reader.push_segment(segment).unwrap_err();
    assert!(matches!(innermost(&error), DeserializeError::FeatureAbsent { feature: "whitespace" }), "{error}");

    // With no sections at all, the no-features lang reads the state fine (missing
    // sections are the empty rules for the features it has — none).
    let empty = state_with(TokenRules::empty(), &[]);
    let mut writer = SerdeSession::<ToyLang>::new();
    writer.intern_state(&empty).unwrap();
    let mut segment_value = writer.take_segment().to_serial_value();
    // Strip every section (the writer wrote all seven, empty).
    let states = entry_field_mut(&mut segment_value, &["tables"]);
    let SerialValue::List(parts) = states else { panic!() };
    let SerialValue::List(state_entries) = entry_field_mut(&mut parts[1], &["entries"]) else { panic!() };
    *entry_field_mut(&mut state_entries[0], &["token_rules"]) = SerialValue::Map(Vec::new());
    let mut reader = SerdeSession::<PlainLang>::new();
    reader.push_segment(Segment::from_serial_value(&segment_value).unwrap()).unwrap();
    let back = reader.state(reader.standard_tables().unwrap().states.position(0)).unwrap();
    assert!(back.prefix_table().is_none());
    assert!(back.trigger_chars().is_none());
    // And a state written by the no-features lang carries no section at all.
    let plain = Arc::new(ParsingState::<PlainLang>::lang_initial().unwrap());
    let mut writer = SerdeSession::<PlainLang>::new();
    writer.intern_state(&plain).unwrap();
    let segment = writer.take_segment();
    let SerialValue::Map(fields) = &entries(&segment, "states")[0] else { panic!() };
    assert_eq!(fields[0], (String::from("token_rules"), SerialValue::Map(Vec::new())));
}

#[test]
fn a_scope_stack_for_a_scopes_less_lang_is_refused() {
    let providers = [ToyProvider::new("p")];
    let state = state_with(TokenRules::empty(), &providers);
    let mut writer = session_with_environment(SerdeSession::<ToyLang>::new(), &providers);
    writer.intern_state(&state).unwrap();
    let mut segment_value = writer.take_segment().to_serial_value();
    // Strip the sections so that only the scope stack is at issue.
    let SerialValue::List(parts) = entry_field_mut(&mut segment_value, &["tables"]) else { panic!() };
    let SerialValue::List(state_entries) = entry_field_mut(&mut parts[1], &["entries"]) else { panic!() };
    *entry_field_mut(&mut state_entries[0], &["token_rules"]) = SerialValue::Map(Vec::new());
    let mut reader = session_with_environment(SerdeSession::<PlainLang>::new(), &providers);
    let error = reader.push_segment(Segment::from_serial_value(&segment_value).unwrap()).unwrap_err();
    assert!(matches!(innermost(&error), DeserializeError::FeatureAbsent { feature: "scopes" }), "{error}");
}

#[test]
fn a_missing_section_reads_as_the_empty_rules_of_a_present_feature() {
    // Written with every section; the reader (a lang with every feature) sees a
    // segment whose `commands` and `comments` sections are missing.
    let state = state_with(full_rules(), &[]);
    let mut writer = SerdeSession::<ToyLang>::new();
    writer.intern_state(&state).unwrap();
    let segment = edited_segment(&writer.take_segment(), "states", 0, |entry| {
        let SerialValue::Map(sections) = entry_field_mut(entry, &["token_rules"]) else { panic!() };
        sections.retain(|(k, _)| k != "commands" && k != "comments");
    });
    let mut reader = SerdeSession::<ToyLang>::new();
    reader.push_segment(segment).unwrap();
    let back = reader.state(reader.standard_tables().unwrap().states.position(0)).unwrap();
    let expected = TokenRules {
        commands: CommandRules::empty(),
        comments: CommentRules::empty(),
        ..full_rules()
    };
    assert_eq!(*back.rules(), expected);
    // The other sections came through as written.
    assert_eq!(back.rules().group_rules().len(), 2);
    assert!(back.rules().whitespace_enabled());
}

#[test]
fn an_expected_close_matching_no_rule_is_accepted_with_a_fresh_handle() {
    let state = state_with(full_rules(), &[]);
    let mut writer = SerdeSession::<ToyLang>::new();
    writer.intern_state(&state).unwrap();
    // The expected close becomes a rule that is neither in `rules` nor in `temporary`.
    let segment = edited_segment(&writer.take_segment(), "states", 0, |entry| {
        *entry_field_mut(entry, &["token_rules", "groups", "expecting_close", "open"]) = SerialValue::Str("<".into());
        *entry_field_mut(entry, &["token_rules", "groups", "expecting_close", "close"]) = SerialValue::Str(">".into());
    });
    let mut reader = SerdeSession::<ToyLang>::new();
    reader.push_segment(segment).unwrap();
    let back = reader.state(reader.standard_tables().unwrap().states.position(0)).unwrap();
    let expecting = back.rules().expecting_group_close().unwrap();
    assert_eq!((&*expecting.open, &*expecting.close), ("<", ">"));
    // A fresh handle: shared with no rule of either list.
    let shared = back
        .rules()
        .group_rules()
        .iter()
        .chain(back.rules().temporary_group_rules())
        .any(|rule| Arc::ptr_eq(rule, expecting));
    assert!(!shared);
}

#[test]
fn bad_provider_references_are_typed_errors() {
    let providers = [ToyProvider::new("p")];
    let state = state_with(TokenRules::empty(), &providers);
    let mut writer = session_with_environment(SerdeSession::<ToyLang>::new(), &providers);
    writer.intern_state(&state).unwrap();
    let segment = writer.take_segment();

    // A provider position beyond the table.
    let bad_index = edited_segment(&segment, "states", 0, |entry| {
        *entry_field_mut(entry, &["scopes"]) =
            SerialValue::List(Vec::from([SerialValue::Index { table: crate::serialize::TableId::new(3), index: 5 }]));
    });
    let mut reader = session_with_environment(SerdeSession::<ToyLang>::new(), &providers);
    let error = reader.push_segment(bad_index).unwrap_err();
    assert!(matches!(innermost(&error), DeserializeError::IndexOutOfRange { table: "providers", index: 5, len: 1 }), "{error}");

    // The provider's identifier is unknown to the reader (no reader registered).
    let mut reader = SerdeSession::<ToyLang>::new();
    let error = reader.push_segment(segment.clone()).unwrap_err();
    assert!(matches!(
        innermost(&error),
        DeserializeError::UnknownIdentifier { table: "providers", identifier } if identifier == "toy.provider"
    ), "{error}");

    // The provider is not in the reading environment.
    let mut reader = session_with_environment(SerdeSession::<ToyLang>::new(), &[]);
    let error = reader.push_segment(segment).unwrap_err();
    assert!(matches!(innermost(&error), DeserializeError::Failed { detail, .. } if detail.contains("`p`")), "{error}");
}

#[test]
fn a_hostile_state_shape_is_rejected() {
    let state = state_with(full_rules(), &[]);
    let mut writer = SerdeSession::<ToyLang>::new();
    writer.intern_state(&state).unwrap();
    let segment = writer.take_segment();
    let push = |segment: Segment| SerdeSession::<ToyLang>::new().push_segment(segment).unwrap_err();

    // A two-character escape char.
    let error = push(edited_segment(&segment, "states", 0, |entry| {
        let SerialValue::List(rules) = entry_field_mut(entry, &["token_rules", "commands", "rules"]) else { panic!() };
        *entry_field_mut(&mut rules[0], &["escape_char"]) = SerialValue::Str("ab".into());
    }));
    assert!(matches!(innermost(&error), DeserializeError::Value(SerialValueError::TypeMismatch { .. })), "{error}");
    // A missing required key.
    let error = push(edited_segment(&segment, "states", 0, |entry| {
        let SerialValue::Map(fields) = entry else { panic!() };
        fields.retain(|(k, _)| k != "scopes");
    }));
    assert!(matches!(innermost(&error), DeserializeError::Value(SerialValueError::MissingField { name: "scopes" })), "{error}");
    // A group type of the wrong kind (u32 expected).
    let error = push(edited_segment(&segment, "states", 0, |entry| {
        let SerialValue::List(rules) = entry_field_mut(entry, &["token_rules", "groups", "rules"]) else { panic!() };
        *entry_field_mut(&mut rules[0], &["group_type"]) = SerialValue::Str("math".into());
    }));
    assert!(matches!(innermost(&error), DeserializeError::Value(SerialValueError::TypeMismatch { .. })), "{error}");
    // A mode that is not null (the toy lang's mode is `()`).
    let error = push(edited_segment(&segment, "states", 0, |entry| {
        *entry_field_mut(entry, &["mode"]) = SerialValue::Int(1);
    }));
    assert!(matches!(innermost(&error), DeserializeError::Value(SerialValueError::TypeMismatch { .. })), "{error}");
}

// --- determinism ------------------------------------------------------------------------

#[test]
fn two_sessions_emit_identical_segments() {
    let providers = [ToyProvider::new("a"), ToyProvider::new("b")];
    let source = arc_source("text", "t.tex");
    let child = Arc::new(Source::resolved("child", "c", SourceSpan::new(&source, 1..3)));
    let state = state_with(full_rules(), &providers);
    let emit = || {
        let mut session = session_with_environment(SerdeSession::<ToyLang>::new(), &providers);
        session.intern_source(&child).unwrap();
        session.intern_state(&state).unwrap();
        session.take_segment()
    };
    assert_eq!(emit(), emit());
}

// --- the value traits' core impls -----------------------------------------------------

#[test]
fn core_value_impls_round_trip_and_range_check() {
    let mut session = SerdeSession::<ToyLang>::new();
    let init = StdDescentGuardInit::default();
    {
    let mut guard = StdDescentGuard::init(&init);
    let mut cx = SerializeContext::new(&mut session, &mut guard);
    assert_eq!(().serialize_value(&mut cx).unwrap(), SerialValue::Null);
    assert_eq!(true.serialize_value(&mut cx).unwrap(), SerialValue::Bool(true));
    assert_eq!(7u8.serialize_value(&mut cx).unwrap(), SerialValue::Int(7));
    assert_eq!((-3i64).serialize_value(&mut cx).unwrap(), SerialValue::Int(-3));
    assert_eq!(7usize.serialize_value(&mut cx).unwrap(), SerialValue::Int(7));
    assert!(matches!(
        u64::MAX.serialize_value(&mut cx),
        Err(SerializeError::Value(SerialValueError::IntegerOutOfRange { target: "i64", .. }))
    ));
    assert_eq!(String::from("s").serialize_value(&mut cx).unwrap(), SerialValue::Str("s".into()));
    assert_eq!(None::<String>.serialize_value(&mut cx).unwrap(), SerialValue::Null);
    assert_eq!(Some(String::from("o")).serialize_value(&mut cx).unwrap(), SerialValue::Str("o".into()));
    assert_eq!(
        Vec::from([1u32, 2]).serialize_value(&mut cx).unwrap(),
        SerialValue::List(Vec::from([SerialValue::Int(1), SerialValue::Int(2)]))
    );
    }

    let mut guard = StdDescentGuard::init(&init);
    let mut cx = DeserializeContext::new(&mut session, &mut guard, None);
    assert_eq!(<()>::deserialize_value(&SerialValue::Null, &mut cx).unwrap(), ());
    assert!(<()>::deserialize_value(&SerialValue::Int(0), &mut cx).is_err());
    assert!(bool::deserialize_value(&SerialValue::Bool(false), &mut cx).is_ok_and(|b| !b));
    assert_eq!(u8::deserialize_value(&SerialValue::Int(255), &mut cx).unwrap(), 255);
    assert!(matches!(
        u8::deserialize_value(&SerialValue::Int(256), &mut cx),
        Err(DeserializeError::Value(SerialValueError::IntegerOutOfRange { target: "u8", .. }))
    ));
    assert!(matches!(
        i8::deserialize_value(&SerialValue::Str("1".into()), &mut cx),
        Err(DeserializeError::Value(SerialValueError::TypeMismatch { found: "str", .. }))
    ));
    assert_eq!(String::deserialize_value(&SerialValue::Str("s".into()), &mut cx).unwrap(), "s");
    assert_eq!(<Option<String>>::deserialize_value(&SerialValue::Null, &mut cx).unwrap(), None);
    assert_eq!(<Option<String>>::deserialize_value(&SerialValue::Str("o".into()), &mut cx).unwrap(), Some("o".into()));
    assert_eq!(
        <Vec<u32>>::deserialize_value(&SerialValue::List(Vec::from([SerialValue::Int(1)])), &mut cx).unwrap(),
        Vec::from([1])
    );
    assert!(<Vec<u32>>::deserialize_value(&SerialValue::List(Vec::from([SerialValue::Null])), &mut cx).is_err());
}

// --- rendering ------------------------------------------------------------------------

#[cfg(feature = "serde")]
mod serde_rendering {
    use super::*;

    /// One source and one state, pinned in the canonical JSON rendering (provisional
    /// wire names).
    #[test]
    fn a_source_and_a_state_render_with_a_pinned_shape() {
        let providers = [ToyProvider::new("pkg")];
        let a = arc_source("ab", "a.tex");
        let b = Arc::new(Source::synthesized("x", "expansion", SourceSpan::new(&a, 0..1)));
        let rules = TokenRules {
            groups: GroupRules {
                enabled: true,
                rules: Vec::from([brace_rule()]),
                temporary: Vec::new(),
                expecting_close: Some(brace_rule()),
            },
            commands: CommandRules {
                enabled: true,
                rules: Vec::from([Arc::new(CommandRule { escape_char: '\\', name_chars: "ab".into() })]),
            },
            ..TokenRules::empty()
        };
        let state = state_with(rules, &providers);
        let mut writer = session_with_environment(
            SerdeSession::<ToyLang>::with_source_driver(
                SourceSerdeDriver::new().with_text_policy(Arc::new(ReferenceAll { with_digest: true })),
            ),
            &providers,
        );
        // Every source referenced, with a digest.
        writer.intern_source(&b).unwrap();
        writer.intern_state(&state).unwrap();
        let segment = writer.take_segment();
        let json = serde_json::to_string(&segment).unwrap();
        assert_eq!(
            json,
            concat!(
                r#"{"version":1,"meta":{},"tables":["#,
                r#"{"name":"sources","table":0,"start":0,"entries":["#,
                r#"{"origin":"a.tex","provenance":"primary","line_number_offset":1,"column_number_offset":1,"#,
                r#""text":{"referenced":{"length":2,"digest":{"algorithm":"toy-sum","bytes":{"$bytes":"AAAAAAAAAMM="}}}}},"#,
                r#"{"origin":null,"provenance":{"synthesized":{"description":"expansion","triggered_at":{"source":{"$index":[0,0]},"start":0,"end":1}}},"#,
                r#""line_number_offset":1,"column_number_offset":1,"#,
                r#""text":{"referenced":{"length":1,"digest":{"algorithm":"toy-sum","bytes":{"$bytes":"AAAAAAAAAHg="}}}}}"#,
                r#"]},"#,
                r#"{"name":"states","table":1,"start":0,"entries":[{"token_rules":{"#,
                r#""whitespace":{"enabled":false,"chars":""},"#,
                r#""paragraphs":{"enabled":false},"#,
                r#""groups":{"enabled":true,"rules":[{"group_type":1,"open":"{","close":"}"}],"temporary":[],"expecting_close":{"group_type":1,"open":"{","close":"}"}},"#,
                r#""commands":{"enabled":true,"rules":[{"escape_char":"\\","name_chars":"ab"}]},"#,
                r#""comments":{"enabled":false,"rules":[]},"#,
                r#""specials":{"enabled":false},"#,
                r#""forbidden_chars":{"chars":""}},"#,
                r#""mode":null,"ext":null,"scopes":[{"$index":[3,0]}]}]},"#,
                r#"{"name":"specs","table":2,"start":0,"entries":[]},"#,
                r#"{"name":"providers","table":3,"start":0,"entries":[{"identifier":"toy.provider","data":"pkg"}]},"#,
                r#"{"name":"trees","table":4,"start":0,"entries":[]},"#,
                r#"{"name":"diagnostics","table":5,"start":0,"entries":[]},"#,
                r#"{"name":"parse-results","table":6,"start":0,"entries":[]}"#,
                r#"]}"#,
            )
        );
        // The rendering reads back to the same segment.
        let back: Segment = serde_json::from_str(&json).unwrap();
        assert_eq!(back, segment);

        // Embedded, the same objects read back through JSON in a fresh session.
        let mut writer = session_with_environment(SerdeSession::<ToyLang>::new(), &providers);
        writer.intern_source(&b).unwrap();
        writer.intern_state(&state).unwrap();
        let json = serde_json::to_string(&writer.take_segment()).unwrap();
        assert!(json.contains(r#""text":{"embedded":"ab"}"#), "{json}");
        let mut reader = session_with_environment(SerdeSession::<ToyLang>::new(), &providers);
        reader.push_segment(serde_json::from_str(&json).unwrap()).unwrap();
        let tables = reader.standard_tables().unwrap();
        assert_eq!(reader.source(tables.sources.position(1)).unwrap().content(), "x");
        assert_eq!(reader.state(tables.states.position(0)).unwrap().rules().command_rules().len(), 1);
    }
}

// --- property: random states round-trip -----------------------------------------------

/// Random token rules and scope stacks (every feature-agnostic section populated at
/// random, the expected close drawn from the temporary rules, the delimiter rules, a
/// rule held by no list, or none) round-trip through a state entry equal in every
/// section, with the providers resolved to the environment's instances in order.
mod state_properties {
    use proptest::prelude::*;

    use super::*;

    /// A short string over a small alphabet (delimiters, escapes, whitespace, a
    /// multibyte character); may be empty.
    fn short_text() -> impl Strategy<Value = String> {
        proptest::collection::vec(
            prop_oneof![
                Just('{'),
                Just('}'),
                Just('['),
                Just(']'),
                Just('$'),
                Just('%'),
                Just('\\'),
                Just(' '),
                Just('\t'),
                Just('a'),
                Just('\u{e9}'),
                Just('*'),
            ],
            0..4,
        )
        .prop_map(|chars| chars.into_iter().collect())
    }

    fn group_rule() -> impl Strategy<Value = Arc<GroupRule<ToyLang>>> {
        (0..4u32, short_text(), short_text())
            .prop_map(|(group_type, open, close)| Arc::new(GroupRule { group_type, open, close }))
    }

    fn command_rule() -> impl Strategy<Value = Arc<CommandRule>> {
        (prop_oneof![Just('\\'), Just('@'), Just('\u{e9}')], short_text())
            .prop_map(|(escape_char, name_chars)| Arc::new(CommandRule { escape_char, name_chars }))
    }

    /// Random rules; `expecting` selects the expected close: an index into the
    /// temporary rules then the delimiter rules, one past them for a rule held by no
    /// list (a group type no random rule has, so it equals none), or beyond for none.
    fn token_rules() -> impl Strategy<Value = TokenRules<ToyLang>> {
        (
            (any::<bool>(), short_text()),
            any::<bool>(),
            (any::<bool>(), proptest::collection::vec(group_rule(), 0..4), proptest::collection::vec(group_rule(), 0..3)),
            0..8usize,
            (any::<bool>(), proptest::collection::vec(command_rule(), 0..3)),
            (any::<bool>(), proptest::collection::vec(short_text(), 0..3)),
            any::<bool>(),
            short_text(),
        )
            .prop_map(
                |(
                    (ws_enabled, ws_chars),
                    paragraphs,
                    (groups_enabled, rules, temporary),
                    expecting,
                    (commands_enabled, commands),
                    (comments_enabled, comments),
                    specials,
                    forbidden,
                )| {
                    let candidates: Vec<Arc<GroupRule<ToyLang>>> =
                        temporary.iter().chain(rules.iter()).cloned().collect();
                    let expecting_close = if expecting < candidates.len() {
                        Some(Arc::clone(&candidates[expecting]))
                    } else if expecting == candidates.len() {
                        Some(Arc::new(GroupRule { group_type: 9, open: "<".into(), close: ">".into() }))
                    } else {
                        None
                    };
                    TokenRules {
                        whitespace: WhitespaceRules { enabled: ws_enabled, chars: ws_chars.into() },
                        paragraphs: ParagraphRules { enabled: paragraphs },
                        groups: GroupRules { enabled: groups_enabled, rules, temporary, expecting_close },
                        commands: CommandRules { enabled: commands_enabled, rules: commands },
                        comments: CommentRules {
                            enabled: comments_enabled,
                            rules: comments.into_iter().map(|start| Arc::new(CommentRule { start })).collect(),
                        },
                        specials: SpecialsRules { enabled: specials },
                        forbidden_chars: ForbiddenCharsRules { chars: forbidden.into() },
                    }
                },
            )
    }

    /// Whether the expected close of `rules` is value-equal to a rule of its lists
    /// (temporary first, then delimiter rules) — what the reader re-links to.
    fn expecting_close_is_listed(rules: &TokenRules<ToyLang>) -> Option<bool> {
        let expecting = rules.expecting_group_close()?;
        Some(rules.temporary_group_rules().iter().chain(rules.group_rules()).any(|rule| **rule == **expecting))
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(48))]

        #[test]
        fn a_random_state_round_trips(rules in token_rules(), scope_count in 0..4usize) {
            let providers: Vec<Arc<ToyProvider>> =
                (0..scope_count).map(|i| ToyProvider::new(&alloc::format!("p{i}"))).collect();
            let state = state_with(rules, &providers);
            let mut writer = session_with_environment(SerdeSession::<ToyLang>::new(), &providers);
            let position = writer.intern_state(&state).unwrap();
            let segment = writer.take_segment();
            #[cfg(feature = "serde")]
            let segment: Segment = serde_json::from_str(&serde_json::to_string(&segment).unwrap()).unwrap();
            let mut reader = session_with_environment(SerdeSession::<ToyLang>::new(), &providers);
            reader.push_segment(segment).unwrap();
            let states = reader.standard_tables().unwrap().states;
            let back = reader.state(states.position(position.index())).unwrap();
            prop_assert_eq!(back.rules(), state.rules());
            let scopes = back.scopes().providers();
            prop_assert_eq!(scopes.len(), providers.len());
            for (provider, resolved) in providers.iter().zip(scopes) {
                prop_assert!(Arc::ptr_eq(&(Arc::clone(provider) as Arc<dyn SpecsProvider<ToyLang>>), resolved));
            }
            // An expected close value-equal to a listed rule is re-linked to the
            // rebuilt list's handle; one held by no list stays its own object.
            if let Some(listed) = expecting_close_is_listed(state.rules()) {
                let expecting = back.rules().expecting_group_close().unwrap();
                let relinked = back
                    .rules()
                    .temporary_group_rules()
                    .iter()
                    .chain(back.rules().group_rules())
                    .any(|rule| Arc::ptr_eq(rule, expecting));
                prop_assert_eq!(relinked, listed);
            }
        }
    }
}
