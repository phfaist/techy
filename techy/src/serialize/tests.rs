//! Unit tests of the serialization foundation: the language gate on the capability
//! traits (vacant vtable for a non-serializable lang; callable defaults for an
//! opted-in one), and the argument-spec index rule's default bodies.

use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec::Vec;

use crate::constructs::{ConstructParserResult, ParseContext};
use crate::engine::{DescentGuard, StdDescentGuard, StdDescentGuardInit};
use crate::scopes::{Package, SpecsProvider};
use crate::spec::{
    ArgumentParser, ArgumentSpec, CallableSpec, ParsedArgumentNodes, StdCallableSpec,
};
use crate::state::{Lang, TrivialLang};

use super::{
    DeserializeContext, DeserializeError, SerdeSession, SerialEntry, SerialValue,
    SerializableLang, SerializableObject, SerializeContext, SerializeError, TableId,
};

// --- the two test langs ---------------------------------------------------------------

/// A language that NEVER implements `SerializableLang` — permanently, by design: it
/// exists to pin the vacant-vtable behavior of the capability methods. Every
/// spec/provider trait object of this lang must still be built and usable for the
/// non-serialization methods although `serialize_object` and the argument-spec pair
/// can neither be called nor reached (no context value exists for this lang). Do not
/// add a `SerializableLang` impl for it, ever: that would defeat the test.
// Capability-trait design: cf. [§dd-dr:serialize-capability-traits].
#[derive(Debug, Clone, Copy)]
struct NeverSerializableLang;
impl TrivialLang for NeverSerializableLang {}

/// A language that opts in: the capability methods are callable for it.
#[derive(Debug, Clone, Copy)]
struct OptedInLang;
impl TrivialLang for OptedInLang {}
impl SerializableLang for OptedInLang {}

// --- stubs ------------------------------------------------------------------------------

/// A spec that does not participate in serialization: the one-line empty impl.
#[derive(Debug)]
struct StubSpec;
impl<L: Lang> SerializableObject<L> for StubSpec {}
impl<L: Lang> CallableSpec<L> for StubSpec {
    fn requires_content(&self) -> bool {
        true
    }
}

/// A spec that participates: overrides `serialize_object`.
#[derive(Debug)]
struct ParticipatingSpec;
impl<L: Lang> SerializableObject<L> for ParticipatingSpec {
    fn serialize_object(
        &self,
        _cx: &mut SerializeContext<'_, L>,
    ) -> Result<SerialEntry, SerializeError>
    where
        L: SerializableLang,
    {
        Ok(SerialEntry {
            identifier: "test.participating".into(),
            data: SerialValue::Map(Vec::from([(String::from("k"), SerialValue::Int(7))])),
        })
    }
}
impl<L: Lang> CallableSpec<L> for ParticipatingSpec {}

/// Stand-in argument parser — never invoked; the tests only need `ArgumentSpec`
/// values with distinct `Arc` identities.
#[derive(Debug)]
struct StubParser;
impl<L: Lang> ArgumentParser<L> for StubParser {
    fn parse_argument(
        &self,
        _cx: &mut ParseContext<'_, '_, L>,
        _spec: &ArgumentSpec<L>,
    ) -> ConstructParserResult<L, Option<ParsedArgumentNodes<L>>> {
        Ok(None)
    }
}

fn two_argument_spec<L: Lang>() -> StdCallableSpec<L> {
    StdCallableSpec::new([
        ArgumentSpec::new(StubParser, "first"),
        ArgumentSpec::new(StubParser, "second"),
    ])
}

// --- the vacant vtable ([§dd-dr:serialize-capability-traits]) ---------------------------------------------------------------

/// `dyn CallableSpec<NeverSerializableLang>` / `dyn SpecsProvider<NeverSerializableLang>`
/// are built and dispatch their non-gated methods although the gated capability
/// methods (`serialize_object`, the argument-spec pair) are uncallable for this lang:
/// the `where L: SerializableLang` clauses leave those vtable slots vacant instead of
/// making the trait objects impossible.
#[test]
fn vacant_vtable_for_a_never_serializable_lang() {
    // A user-side stub spec, through both trait-object shapes.
    let shared: Arc<dyn CallableSpec<NeverSerializableLang>> = Arc::new(StubSpec);
    assert!(shared.arguments().is_empty());
    assert!(shared.requires_content());

    let borrowed: &dyn CallableSpec<NeverSerializableLang> = &StubSpec;
    assert!(borrowed.requires_content());

    // The crate's own spec and provider types for the same lang.
    let std_spec: Arc<dyn CallableSpec<NeverSerializableLang>> =
        Arc::new(two_argument_spec::<NeverSerializableLang>());
    assert_eq!(std_spec.arguments().len(), 2);
    // Both stub arguments can match empty (the parser default), so no content is required.
    assert!(!std_spec.requires_content());

    let package: Arc<dyn SpecsProvider<NeverSerializableLang>> =
        Arc::new(Package::<NeverSerializableLang>::new("pkg"));
    assert_eq!(package.name(), "pkg");
}

// --- the opted-in lang: defaults through the vtable ---------------------------------------

/// A serialization context over an empty session, for calling the capability methods
/// directly (the engine's own tests exercise the session; see `engine/tests.rs`).
fn with_serialize_context<R>(f: impl FnOnce(&mut SerializeContext<'_, OptedInLang>) -> R) -> R {
    let mut session = SerdeSession::<OptedInLang>::empty();
    let mut guard = StdDescentGuard::init(&StdDescentGuardInit::default());
    let mut cx = SerializeContext::new(&mut session, &mut guard);
    f(&mut cx)
}

/// The deserialization counterpart of [`with_serialize_context`].
fn with_deserialize_context<R>(f: impl FnOnce(&mut DeserializeContext<'_, OptedInLang>) -> R) -> R {
    let mut session = SerdeSession::<OptedInLang>::empty();
    let mut guard = StdDescentGuard::init(&StdDescentGuardInit::default());
    let mut cx = DeserializeContext::new(&mut session, &mut guard, None);
    f(&mut cx)
}

/// Through `dyn CallableSpec<L>` for an opted-in lang, the default `serialize_object`
/// reports `Unsupported`, and an override is dispatched.
#[test]
fn serialize_object_defaults_to_unsupported_and_dispatches_overrides() {
    with_serialize_context(|cx| {
        let stub: Arc<dyn CallableSpec<OptedInLang>> = Arc::new(StubSpec);
        assert!(matches!(stub.serialize_object(cx), Err(SerializeError::Unsupported)));
        assert!(matches!(SerializeError::unsupported(), SerializeError::Unsupported));

        let participating: Arc<dyn CallableSpec<OptedInLang>> = Arc::new(ParticipatingSpec);
        let entry = participating.serialize_object(cx).unwrap();
        assert_eq!(entry.identifier, "test.participating");
        assert_eq!(
            entry.data,
            SerialValue::Map(Vec::from([("k".into(), SerialValue::Int(7))]))
        );

        // A provider's serialization, through its own trait object: a package writes
        // its identity (its name) — no default in play, the impl is the crate's.
        let package: Arc<dyn SpecsProvider<OptedInLang>> = Arc::new(Package::<OptedInLang>::new("pkg"));
        let entry = package.serialize_object(cx).unwrap();
        assert_eq!(entry.identifier, "core.package");
        assert_eq!(entry.data, SerialValue::Map(Vec::from([("name".into(), SerialValue::Str("pkg".into()))])));
    });
}

// --- the argument-spec index rule (`CallableSpec::serialize_argument_spec`) ----------------------------------------------------

/// The write default: `Ok(None)` exactly when the parsed argument's `Arc` is the
/// declared one at that index (pointer identity); out-of-band otherwise, including
/// an out-of-range index and an equal-but-distinct allocation.
#[test]
fn serialize_argument_spec_default_is_the_index_rule() {
    with_serialize_context(|cx| {
        let spec: Arc<dyn CallableSpec<OptedInLang>> = Arc::new(two_argument_spec::<OptedInLang>());
        let declared = spec.arguments().to_vec();

        assert!(matches!(spec.serialize_argument_spec(0, &declared[0], cx), Ok(None)));
        assert!(matches!(spec.serialize_argument_spec(1, &declared[1], cx), Ok(None)));

        // Same content, other allocation: not the declared Arc.
        let lookalike = Arc::new(ArgumentSpec::<OptedInLang>::new(StubParser, "first"));
        assert!(matches!(
            spec.serialize_argument_spec(0, &lookalike, cx),
            Err(SerializeError::ArgumentSpecOutOfBand { index: 0, count: 2 })
        ));
        // The declared Arc, but at the wrong position.
        assert!(matches!(
            spec.serialize_argument_spec(0, &declared[1], cx),
            Err(SerializeError::ArgumentSpecOutOfBand { index: 0, count: 2 })
        ));
        // Out of range.
        assert!(matches!(
            spec.serialize_argument_spec(2, &declared[0], cx),
            Err(SerializeError::ArgumentSpecOutOfBand { index: 2, count: 2 })
        ));
    });
}

/// The read default: for `None`, the declared `Arc` at that index (the same
/// allocation); a bounds error beyond the declared count; and a `Some(_)` payload —
/// written by an overriding spec type — is a fail-closed error, never ignored.
#[test]
fn deserialize_argument_spec_default_is_the_index_rule() {
    with_deserialize_context(|cx| {
        let spec: Arc<dyn CallableSpec<OptedInLang>> = Arc::new(two_argument_spec::<OptedInLang>());
        let declared = spec.arguments().to_vec();

        let rebuilt = spec.deserialize_argument_spec(1, None, cx).unwrap();
        assert!(Arc::ptr_eq(&rebuilt, &declared[1]));

        assert!(matches!(
            spec.deserialize_argument_spec(2, None, cx).map(|_| ()),
            Err(DeserializeError::ArgumentIndexOutOfRange { index: 2, count: 2 })
        ));

        // A payload reaching the default: the writer's spec type overrode the pair, the
        // reader's did not — even at an in-range index.
        let payload = SerialValue::Str("custom".into());
        assert!(matches!(
            spec.deserialize_argument_spec(0, Some(&payload), cx).map(|_| ()),
            Err(DeserializeError::UnexpectedArgumentSpecPayload { index: 0 })
        ));
        // The payload check comes first: an out-of-range index with a payload reports
        // the payload.
        assert!(matches!(
            spec.deserialize_argument_spec(5, Some(&payload), cx).map(|_| ()),
            Err(DeserializeError::UnexpectedArgumentSpecPayload { index: 5 })
        ));
    });
}

// --- the value model ------------------------------------------------------------------------

/// `SerialValue` equality is structural and order-sensitive for maps; `TableId`s
/// compare by ordinal.
#[test]
fn serial_value_equality_is_structural() {
    let a = SerialValue::Map(Vec::from([
        ("x".into(), SerialValue::Int(1)),
        (
            "y".into(),
            SerialValue::List(Vec::from([SerialValue::Null, SerialValue::Bool(true)])),
        ),
    ]));
    let b = a.clone();
    assert_eq!(a, b);

    let reordered = SerialValue::Map(Vec::from([
        (
            "y".into(),
            SerialValue::List(Vec::from([SerialValue::Null, SerialValue::Bool(true)])),
        ),
        ("x".into(), SerialValue::Int(1)),
    ]));
    assert_ne!(a, reordered);

    let first = SerialValue::Index {
        table: TableId::new(0),
        index: 3,
    };
    let second = SerialValue::Index {
        table: TableId::new(1),
        index: 3,
    };
    assert_ne!(first, second);
    assert_eq!(
        first,
        SerialValue::Index {
            table: TableId::new(0),
            index: 3
        }
    );
    assert!(TableId::new(0) < TableId::new(1));
}

/// The error types render and implement `Error`.
#[test]
fn errors_display() {
    use alloc::string::ToString;
    let unsupported: &dyn core::error::Error = &SerializeError::unsupported();
    assert_eq!(
        unsupported.to_string(),
        "serialization is unsupported by this type"
    );
    let out_of_band = SerializeError::ArgumentSpecOutOfBand { index: 1, count: 1 }.to_string();
    assert!(out_of_band.contains("argument #2"), "{out_of_band}");
    let out_of_range: &dyn core::error::Error =
        &DeserializeError::ArgumentIndexOutOfRange { index: 3, count: 2 };
    assert!(out_of_range.to_string().contains("argument #4"));
    let unexpected = DeserializeError::UnexpectedArgumentSpecPayload { index: usize::MAX }.to_string();
    assert!(unexpected.contains("does not override deserialize_argument_spec"), "{unexpected}");

    // The node location wrappers render the node (and callable) and expose the cause.
    let in_node: &dyn core::error::Error = &SerializeError::InNode {
        node: 3,
        callable: Some(String::from("frac")),
        cause: Box::new(SerializeError::unsupported()),
    };
    assert_eq!(
        in_node.to_string(),
        "while serializing node #3 (callable `frac`) of the tree: serialization is unsupported by this type"
    );
    assert!(in_node.source().is_some());
    let in_node: &dyn core::error::Error = &DeserializeError::InNode {
        node: 0,
        callable: None,
        cause: Box::new(DeserializeError::failed("x")),
    };
    assert_eq!(in_node.to_string(), "while rebuilding node #0 of the tree: deserialization failed: x");
    assert!(in_node.source().is_some());
}

// --- the nesting bound ------------------------------------------------------------------

/// `depth` lists nested one inside the other around `innermost`, built without
/// recursion (`0` returns `innermost` itself). Shared by the nesting-bound tests of
/// the foundation, the engine, and the rendering layer.
pub(crate) fn nested_lists(depth: usize, innermost: SerialValue) -> SerialValue {
    let mut value = innermost;
    for _ in 0..depth {
        value = SerialValue::List(Vec::from([value]));
    }
    value
}

/// `depth` one-entry maps (key `k`) nested one inside the other around `innermost`.
pub(crate) fn nested_maps(depth: usize, innermost: SerialValue) -> SerialValue {
    let mut value = innermost;
    for _ in 0..depth {
        value = SerialValue::Map(Vec::from([(String::from("k"), value)]));
    }
    value
}

#[test]
fn nesting_depth_counts_the_enclosing_lists_and_maps() {
    assert_eq!(SerialValue::Null.nesting_depth(), 0);
    assert_eq!(SerialValue::Int(3).nesting_depth(), 0);
    assert_eq!(SerialValue::Bytes(Vec::from([1u8])).nesting_depth(), 0);
    assert_eq!(SerialValue::Index { table: TableId::new(0), index: 0 }.nesting_depth(), 0);
    assert_eq!(SerialValue::List(Vec::new()).nesting_depth(), 1);
    assert_eq!(SerialValue::Map(Vec::new()).nesting_depth(), 1);
    assert_eq!(nested_lists(2, SerialValue::Int(1)).nesting_depth(), 2);
    assert_eq!(nested_maps(3, SerialValue::Null).nesting_depth(), 3);
    // The deepest branch counts, wherever it sits.
    let mixed = SerialValue::Map(Vec::from([
        ("a".into(), SerialValue::Int(1)),
        ("b".into(), nested_lists(4, SerialValue::Null)),
        ("c".into(), nested_maps(2, SerialValue::Null)),
    ]));
    assert_eq!(mixed.nesting_depth(), 5);
    // Measured without recursion: a value far deeper than the bound is measured
    // fine (dropped recursively afterwards, which this depth still permits — a
    // value's drop is the cost of whoever built it).
    assert_eq!(nested_lists(2_000, SerialValue::Null).nesting_depth(), 2_000);
    assert_eq!(SerialValue::MAX_NESTING_DEPTH, 64);
}

/// A segment's serialized form is checked against the bound before it is walked:
/// an entry at the greatest depth a segment allows round-trips; one level more is
/// refused by `from_serial_value` (the whole value, the segment's own four levels
/// counted) — for a hostile depth far beyond the bound as much as for one level over.
#[test]
fn a_segment_value_nesting_too_deep_is_refused_before_it_is_walked() {
    use super::engine::MAX_ENTRY_NESTING_DEPTH;
    use super::{Segment, SerialValueError};

    fn segment_value(entry: SerialValue) -> SerialValue {
        SerialValue::Map(Vec::from([
            ("version".into(), SerialValue::Int(1)),
            ("meta".into(), SerialValue::Map(Vec::new())),
            (
                "tables".into(),
                SerialValue::List(Vec::from([SerialValue::Map(Vec::from([
                    ("name".into(), SerialValue::Str("leaves".into())),
                    ("table".into(), SerialValue::Int(0)),
                    ("start".into(), SerialValue::Int(0)),
                    ("entries".into(), SerialValue::List(Vec::from([entry]))),
                ]))])),
            ),
        ]))
    }

    assert_eq!(MAX_ENTRY_NESTING_DEPTH, SerialValue::MAX_NESTING_DEPTH - 4);
    let at_the_bound = segment_value(nested_lists(MAX_ENTRY_NESTING_DEPTH, SerialValue::Null));
    assert_eq!(at_the_bound.nesting_depth(), SerialValue::MAX_NESTING_DEPTH);
    let segment = Segment::from_serial_value(&at_the_bound).expect("a segment at the bound reads");
    assert_eq!(segment.to_serial_value(), at_the_bound);

    for depth in [MAX_ENTRY_NESTING_DEPTH + 1, 500, 2_000] {
        let too_deep = segment_value(nested_maps(depth, SerialValue::Null));
        assert_eq!(
            Segment::from_serial_value(&too_deep).unwrap_err(),
            SerialValueError::NestingTooDeep { limit: SerialValue::MAX_NESTING_DEPTH },
            "depth {depth}"
        );
    }
    let message = alloc::string::ToString::to_string(&SerialValueError::NestingTooDeep { limit: 64 });
    assert_eq!(message, "the value nests deeper than 64 levels of lists and maps");
}

// --- the field hooks of the value traits ----------------------------------------------
//
// `is_absent_field` / `deserialize_field`: what the `SerializableValue` /
// `DeserializableValue` derives rely on for a field of a derived structure — an
// `Option` that is `None` is an omitted key, read back from a missing key or a `null`;
// every other type always writes its key and requires it when reading.

/// `Option` is absent as a field when `None`, and only then; as a bare value it is
/// still `null`.
#[test]
fn option_is_absent_as_a_field_and_null_as_a_value() {
    use super::wire::FieldWriter;
    use super::{DeserializableValue, SerializableValue};

    with_serialize_context(|cx| {
        let none: Option<u32> = None;
        assert!(SerializableValue::<OptedInLang>::is_absent_field(&none));
        assert!(!SerializableValue::<OptedInLang>::is_absent_field(&Some(1u32)));
        assert!(!SerializableValue::<OptedInLang>::is_absent_field(&7u32));
        assert!(!SerializableValue::<OptedInLang>::is_absent_field(&String::from("x")));
        assert!(!SerializableValue::<OptedInLang>::is_absent_field(&SerialValue::Null));
        assert_eq!(none.serialize_value(cx).unwrap(), SerialValue::Null);

        // Through the field writer: the absent field's key is left out, the others
        // follow in call order.
        let mut writer = FieldWriter::with_capacity(4);
        writer.value_field("a", &none, cx).unwrap();
        writer.value_field("b", &Some(2u32), cx).unwrap();
        writer.value_field("c", &3u32, cx).unwrap();
        writer.value_field("d", &SerialValue::Null, cx).unwrap();
        assert_eq!(
            writer.finish(),
            SerialValue::Map(Vec::from([
                (String::from("b"), SerialValue::Int(2)),
                (String::from("c"), SerialValue::Int(3)),
                (String::from("d"), SerialValue::Null),
            ]))
        );
    });

    // `char` and `SerialValue` (added for derived structures): their forms.
    with_serialize_context(|cx| {
        assert_eq!('\\'.serialize_value(cx).unwrap(), SerialValue::Str(String::from("\\")));
        let verbatim = SerialValue::List(Vec::from([SerialValue::Int(1)]));
        assert_eq!(verbatim.serialize_value(cx).unwrap(), verbatim);
    });
    with_deserialize_context(|cx| {
        assert_eq!(char::deserialize_value(&SerialValue::Str(String::from("é")), cx).unwrap(), 'é');
        assert!(char::deserialize_value(&SerialValue::Str(String::from("ab")), cx).is_err());
        assert!(char::deserialize_value(&SerialValue::Str(String::new()), cx).is_err());
        assert!(char::deserialize_value(&SerialValue::Int(97), cx).is_err());
        let verbatim = SerialValue::Map(Vec::from([(String::from("k"), SerialValue::Bool(true))]));
        assert_eq!(SerialValue::deserialize_value(&verbatim, cx).unwrap(), verbatim);
    });
}

/// Reading a field by name: a missing key is `None` for an `Option` and a
/// `MissingField` error naming the key for anything else; a `null` reads as `None`.
#[test]
fn field_reads_handle_a_missing_key_by_type() {
    use super::wire::FieldReader;
    use super::SerialValueError;

    with_deserialize_context(|cx| {
        let map = SerialValue::Map(Vec::from([
            (String::from("b"), SerialValue::Int(2)),
            (String::from("n"), SerialValue::Null),
        ]));
        let reader = FieldReader::new(&map, "Sample", &["a", "b", "c", "n"]).unwrap();

        assert_eq!(reader.value_field::<OptedInLang, Option<u32>>("a", cx).unwrap(), None);
        assert_eq!(reader.value_field::<OptedInLang, Option<u32>>("n", cx).unwrap(), None);
        assert_eq!(reader.value_field::<OptedInLang, Option<u32>>("b", cx).unwrap(), Some(2));
        assert_eq!(reader.value_field::<OptedInLang, u32>("b", cx).unwrap(), 2);
        assert_eq!(reader.value_field::<OptedInLang, SerialValue>("n", cx).unwrap(), SerialValue::Null);

        match reader.value_field::<OptedInLang, u32>("c", cx) {
            Err(DeserializeError::Value(SerialValueError::MissingField { name })) => assert_eq!(name, "c"),
            other => panic!("expected MissingField, got {other:?}"),
        }
        match reader.value_field::<OptedInLang, SerialValue>("c", cx) {
            Err(DeserializeError::Value(SerialValueError::MissingField { name })) => assert_eq!(name, "c"),
            other => panic!("expected MissingField for a verbatim value too, got {other:?}"),
        }
        // A present key with a value of the wrong kind is that type's own error, not a
        // missing-field error.
        assert!(matches!(
            reader.value_field::<OptedInLang, u32>("n", cx),
            Err(DeserializeError::Value(SerialValueError::TypeMismatch { .. }))
        ));
    });
}

/// Two levels of `Option`: an `Option<Option<T>>` field is omitted when `None` and
/// written as `null` when `Some(None)` — reading back as `None`, the documented loss;
/// an `Option<SerialValue>` holding a `Null` is not absent. And an error inside a
/// field's conversion propagates out of `value_field`, leaving no entry behind.
#[test]
fn field_hooks_at_two_levels_and_error_propagation() {
    use super::wire::{FieldReader, FieldWriter};
    use super::{SerialValueError, SerializableValue};

    with_serialize_context(|cx| {
        let outer_none: Option<Option<u32>> = None;
        let some_none: Option<Option<u32>> = Some(None);
        let some_some: Option<Option<u32>> = Some(Some(5));
        let some_null: Option<SerialValue> = Some(SerialValue::Null);
        assert!(SerializableValue::<OptedInLang>::is_absent_field(&outer_none));
        assert!(!SerializableValue::<OptedInLang>::is_absent_field(&some_none));
        assert!(!SerializableValue::<OptedInLang>::is_absent_field(&some_null));

        let mut writer = FieldWriter::with_capacity(4);
        writer.value_field("a", &outer_none, cx).unwrap();
        writer.value_field("b", &some_none, cx).unwrap();
        writer.value_field("c", &some_some, cx).unwrap();
        writer.value_field("d", &some_null, cx).unwrap();
        assert_eq!(
            writer.finish(),
            SerialValue::Map(Vec::from([
                (String::from("b"), SerialValue::Null),
                (String::from("c"), SerialValue::Int(5)),
                (String::from("d"), SerialValue::Null),
            ]))
        );

        let mut writer = FieldWriter::with_capacity(2);
        writer.value_field("ok", &1u32, cx).unwrap();
        assert!(matches!(
            writer.value_field("big", &u64::MAX, cx),
            Err(SerializeError::Value(SerialValueError::IntegerOutOfRange { .. }))
        ));
        assert_eq!(writer.finish(), SerialValue::Map(Vec::from([(String::from("ok"), SerialValue::Int(1))])));
    });

    with_deserialize_context(|cx| {
        let map = SerialValue::Map(Vec::from([
            (String::from("b"), SerialValue::Null),
            (String::from("c"), SerialValue::Int(5)),
        ]));
        let reader = FieldReader::new(&map, "Sample", &["a", "b", "c"]).unwrap();
        assert_eq!(reader.value_field::<OptedInLang, Option<Option<u32>>>("a", cx).unwrap(), None);
        assert_eq!(reader.value_field::<OptedInLang, Option<Option<u32>>>("b", cx).unwrap(), None);
        assert_eq!(reader.value_field::<OptedInLang, Option<Option<u32>>>("c", cx).unwrap(), Some(Some(5)));
        assert_eq!(reader.value_field::<OptedInLang, Option<SerialValue>>("b", cx).unwrap(), None);
    });
}
