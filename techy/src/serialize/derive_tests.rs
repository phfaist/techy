//! Tests of the public `#[derive(SerializableValue)]` / `#[derive(DeserializableValue)]`
//! pair, expanded inside the crate itself (the generated code names `::techy::…`,
//! which `extern crate self as techy` resolves here): round trips of a struct with
//! every supported field kind and of an enum with every variant kind, the omission of
//! absent fields, the strict-read failures, the context reaching the fields (spans in a
//! type that names its language with `#[serial(lang = …)]` intern their source), and —
//! with the `serde` feature — the identity of the shapes with the serde bridge's. The
//! rejections of unsupported shapes are tested in techy-derive; a consumer crate's use
//! is the integration test `tests/derive_serialize_value.rs`.

use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec::Vec;

use crate::engine::{DescentGuard, StdDescentGuard, StdDescentGuardInit};
use crate::source::{Source, SourceSpan};
use crate::state::TrivialLang;

use super::{
    register_core_readers, DeserializableValue, DeserializeContext, DeserializeError,
    SerdeSession, SerialValue, SerialValueError, SerializableLang, SerializableValue,
    SerializeContext,
};

// --- a lang and its contexts ------------------------------------------------------------

/// A language on the crate's defaults, opted in.
#[derive(Debug, Clone, Copy)]
struct DerivingLang;
impl TrivialLang for DerivingLang {}
impl SerializableLang for DerivingLang {}

fn with_serialize_context<R>(f: impl FnOnce(&mut SerializeContext<'_, DerivingLang>) -> R) -> R {
    let mut session = SerdeSession::<DerivingLang>::empty();
    let mut guard = StdDescentGuard::init(&StdDescentGuardInit::default());
    let mut cx = SerializeContext::new(&mut session, &mut guard);
    f(&mut cx)
}

fn with_deserialize_context<R>(f: impl FnOnce(&mut DeserializeContext<'_, DerivingLang>) -> R) -> R {
    let mut session = SerdeSession::<DerivingLang>::empty();
    let mut guard = StdDescentGuard::init(&StdDescentGuardInit::default());
    let mut cx = DeserializeContext::new(&mut session, &mut guard, None);
    f(&mut cx)
}

fn serialize<T: SerializableValue<DerivingLang>>(value: &T) -> SerialValue {
    with_serialize_context(|cx| value.serialize_value(cx)).expect("serializes")
}

fn deserialize<T: DeserializableValue<DerivingLang>>(value: &SerialValue) -> Result<T, DeserializeError> {
    with_deserialize_context(|cx| T::deserialize_value(value, cx))
}

fn read_error<T: DeserializableValue<DerivingLang> + core::fmt::Debug>(value: &SerialValue) -> SerialValueError {
    match deserialize::<T>(value) {
        Err(DeserializeError::Value(error)) => error,
        other => panic!("expected a value error, got {other:?}"),
    }
}

fn s(text: &str) -> String {
    String::from(text)
}

fn map(entries: impl IntoIterator<Item = (&'static str, SerialValue)>) -> SerialValue {
    SerialValue::Map(entries.into_iter().map(|(key, value)| (s(key), value)).collect())
}

// --- the derived types ------------------------------------------------------------------

/// Every supported field kind.
#[derive(SerializableValue, DeserializableValue, Debug, PartialEq, Clone)]
struct All {
    #[serial(name = "flag")]
    flag: bool,
    #[serial(name = "small")]
    small: i8,
    #[serial(name = "wide")]
    wide: u64,
    #[serial(name = "count")]
    count: usize,
    #[serial(name = "escape")]
    escape: char,
    #[serial(name = "text")]
    text: String,
    #[serial(name = "maybe")]
    maybe: Option<u32>,
    #[serial(name = "items")]
    items: Vec<i64>,
    #[serial(name = "ext")]
    ext: SerialValue,
    #[serial(name = "inner")]
    inner: Inner,
    #[serial(name = "kinds")]
    kinds: Vec<Kind>,
    #[serial(name = "nested-option")]
    nested_option: Option<Option<bool>>,
}

/// A nested derived struct with an optional field.
#[derive(SerializableValue, DeserializableValue, Debug, PartialEq, Clone)]
struct Inner {
    #[serial(name = "id")]
    id: u32,
    #[serial(name = "note")]
    note: Option<String>,
}

/// Every supported variant kind.
#[derive(SerializableValue, DeserializableValue, Debug, PartialEq, Clone)]
enum Kind {
    #[serial(name = "plain")]
    Plain,
    #[serial(name = "named")]
    Named(String),
    #[serial(name = "wrapped")]
    Wrapped(Inner),
    #[serial(name = "sized")]
    Sized {
        #[serial(name = "w")]
        w: u16,
        #[serial(name = "h")]
        h: Option<u16>,
    },
}

fn sample() -> All {
    All {
        flag: true,
        small: -3,
        wide: 1 << 40,
        count: 7,
        escape: '\\',
        text: s("hello"),
        maybe: None,
        items: Vec::from([1, -2, 3]),
        ext: map([("k", SerialValue::Bool(false))]),
        inner: Inner { id: 1, note: Some(s("x")) },
        kinds: Vec::from([
            Kind::Plain,
            Kind::Named(s("n")),
            Kind::Wrapped(Inner { id: 2, note: None }),
            Kind::Sized { w: 3, h: None },
            Kind::Sized { w: 3, h: Some(4) },
        ]),
        nested_option: Some(Some(false)),
    }
}

fn sample_kinds_value() -> SerialValue {
    SerialValue::List(Vec::from([
        SerialValue::Str(s("plain")),
        map([("named", SerialValue::Str(s("n")))]),
        map([("wrapped", map([("id", SerialValue::Int(2))]))]),
        map([("sized", map([("w", SerialValue::Int(3))]))]),
        map([("sized", map([("w", SerialValue::Int(3)), ("h", SerialValue::Int(4))]))]),
    ]))
}

fn sample_value() -> SerialValue {
    map([
        ("flag", SerialValue::Bool(true)),
        ("small", SerialValue::Int(-3)),
        ("wide", SerialValue::Int(1 << 40)),
        ("count", SerialValue::Int(7)),
        ("escape", SerialValue::Str(s("\\"))),
        ("text", SerialValue::Str(s("hello"))),
        // `maybe: None` is left out.
        ("items", SerialValue::List(Vec::from([SerialValue::Int(1), SerialValue::Int(-2), SerialValue::Int(3)]))),
        ("ext", map([("k", SerialValue::Bool(false))])),
        ("inner", map([("id", SerialValue::Int(1)), ("note", SerialValue::Str(s("x")))])),
        ("kinds", sample_kinds_value()),
        ("nested-option", SerialValue::Bool(false)),
    ])
}

// --- round trips and the shape ----------------------------------------------------------

/// The serialized form is the documented one — fields in declaration order under their
/// wire names, an absent `Option` left out — and reads back to the same value.
#[test]
fn a_struct_round_trips_through_its_documented_shape() {
    let value = serialize(&sample());
    assert_eq!(value, sample_value());
    assert_eq!(deserialize::<All>(&value).unwrap(), sample());
}

/// Every variant kind, alone and read back.
#[test]
fn an_enum_round_trips_through_its_documented_shape() {
    let kinds = Vec::from([
        Kind::Plain,
        Kind::Named(s("n")),
        Kind::Wrapped(Inner { id: 2, note: None }),
        Kind::Sized { w: 3, h: None },
        Kind::Sized { w: 3, h: Some(4) },
    ]);
    assert_eq!(serialize(&kinds), sample_kinds_value());
    for kind in kinds {
        let value = serialize(&kind);
        assert_eq!(deserialize::<Kind>(&value).unwrap(), kind);
    }
}

/// An absent `Option` field is an omitted key; reading accepts the missing key and a
/// `null` alike, as `None`. A present `Some(None)` writes `null` and reads as `None`.
#[test]
fn an_absent_option_field_is_an_omitted_key() {
    let without = Inner { id: 5, note: None };
    assert_eq!(serialize(&without), map([("id", SerialValue::Int(5))]));
    assert_eq!(deserialize::<Inner>(&map([("id", SerialValue::Int(5))])).unwrap(), without);
    assert_eq!(
        deserialize::<Inner>(&map([("id", SerialValue::Int(5)), ("note", SerialValue::Null)])).unwrap(),
        without
    );

    let mut nested = sample();
    nested.nested_option = Some(None);
    let value = serialize(&nested);
    match &value {
        SerialValue::Map(entries) => {
            assert!(entries.iter().any(|(key, value)| key == "nested-option" && *value == SerialValue::Null));
        }
        other => panic!("a map, not {other:?}"),
    }
    let back = deserialize::<All>(&value).unwrap();
    assert_eq!(back.nested_option, None);
}

// --- strict reads -----------------------------------------------------------------------

#[test]
fn an_unknown_key_is_refused() {
    let value = map([("id", SerialValue::Int(1)), ("extra", SerialValue::Null)]);
    assert_eq!(
        read_error::<Inner>(&value),
        SerialValueError::UnknownField { name: s("extra"), expected: &["id", "note"] }
    );
}

#[test]
fn a_repeated_key_is_refused() {
    let value = map([("id", SerialValue::Int(1)), ("id", SerialValue::Int(2))]);
    assert_eq!(read_error::<Inner>(&value), SerialValueError::DuplicateField { name: s("id") });
}

#[test]
fn a_missing_required_key_is_refused_naming_the_key() {
    let value = map([("note", SerialValue::Str(s("x")))]);
    assert_eq!(read_error::<Inner>(&value), SerialValueError::MissingField { name: "id" });
    // Inside a struct variant too.
    let value = map([("sized", map([("h", SerialValue::Int(1))]))]);
    assert_eq!(read_error::<Kind>(&value), SerialValueError::MissingField { name: "w" });
}

#[test]
fn an_unknown_variant_is_refused() {
    let expected: &'static [&'static str] = &["plain", "named", "wrapped", "sized"];
    assert_eq!(
        read_error::<Kind>(&SerialValue::Str(s("other"))),
        SerialValueError::UnknownVariant { name: s("other"), expected }
    );
    assert_eq!(
        read_error::<Kind>(&map([("other", SerialValue::Null)])),
        SerialValueError::UnknownVariant { name: s("other"), expected }
    );
}

#[test]
fn a_variant_with_the_wrong_payload_shape_is_refused() {
    // A unit variant carrying data.
    assert!(matches!(
        read_error::<Kind>(&map([("plain", SerialValue::Null)])),
        SerialValueError::TypeMismatch { found: "map", .. }
    ));
    // A data variant written as a bare string.
    assert!(matches!(
        read_error::<Kind>(&SerialValue::Str(s("named"))),
        SerialValueError::TypeMismatch { found: "str", .. }
    ));
    // A struct variant whose payload is not a map.
    assert!(matches!(
        read_error::<Kind>(&map([("sized", SerialValue::Int(3))])),
        SerialValueError::TypeMismatch { found: "int", .. }
    ));
    // A newtype payload of the wrong kind: the payload type's own error.
    assert!(matches!(
        read_error::<Kind>(&map([("named", SerialValue::Int(1))])),
        SerialValueError::TypeMismatch { found: "int", .. }
    ));
    // Neither a string nor a one-entry map.
    assert!(matches!(
        read_error::<Kind>(&SerialValue::List(Vec::new())),
        SerialValueError::TypeMismatch { found: "list", .. }
    ));
    assert!(matches!(
        read_error::<Kind>(&map([("plain", SerialValue::Null), ("named", SerialValue::Null)])),
        SerialValueError::TypeMismatch { found: "map", .. }
    ));
}

#[test]
fn a_value_of_the_wrong_kind_is_refused_by_the_field_type() {
    let value = map([("id", SerialValue::Str(s("one")))]);
    assert!(matches!(read_error::<Inner>(&value), SerialValueError::TypeMismatch { found: "str", .. }));
    // A struct given a non-map.
    assert!(matches!(read_error::<Inner>(&SerialValue::Int(1)), SerialValueError::TypeMismatch { found: "int", .. }));
    // An out-of-range integer is the field type's own range error.
    let value = map([("id", SerialValue::Int(-1))]);
    assert!(matches!(read_error::<Inner>(&value), SerialValueError::IntegerOutOfRange { target: "u32", .. }));
}

// --- the context, observed ----------------------------------------------------------------
//
// A span's conversion exists for the languages whose `SourceOrigin` is the span's
// origin type, so a type holding one names its language with `#[serial(lang = …)]`;
// its spans then intern their source through the context — the observable use of the
// context a derived impl passes along, as a struct field, a newtype payload, and a
// struct-variant field.

/// Spans as a struct field and, through `Place`, as a newtype payload and a
/// struct-variant field.
#[derive(SerializableValue, DeserializableValue, Debug, Clone)]
#[serial(lang = DerivingLang)]
struct Located {
    #[serial(name = "at")]
    at: SourceSpan<Option<String>>,
    #[serial(name = "place")]
    place: Place,
}

#[derive(SerializableValue, DeserializableValue, Debug, Clone)]
#[serial(lang = DerivingLang)]
enum Place {
    #[serial(name = "point")]
    Point(SourceSpan<Option<String>>),
    #[serial(name = "range")]
    Range {
        #[serial(name = "from")]
        from: SourceSpan<Option<String>>,
        #[serial(name = "to")]
        to: Option<SourceSpan<Option<String>>>,
    },
}

/// Serialize `value` with a session holding the standard tables, then read it back in
/// a second session that absorbed the emitted segment: the serialized form, the number
/// of entries the sources table received, and the rebuilt value.
fn through_sessions<T>(value: &T) -> (SerialValue, usize, T)
where
    T: SerializableValue<DerivingLang> + DeserializableValue<DerivingLang>,
{
    let mut writer = SerdeSession::<DerivingLang>::new();
    let sources = writer.standard_tables().expect("standard tables").sources.id();
    let written = {
        let mut guard = StdDescentGuard::init(&StdDescentGuardInit::default());
        let mut cx = SerializeContext::new(&mut writer, &mut guard);
        value.serialize_value(&mut cx).expect("serializes")
    };
    let segment = writer.take_segment();
    let source_entries = segment
        .tables()
        .iter()
        .find(|table| table.id() == sources)
        .map_or(0, |table| table.entries().len());

    let mut reader = SerdeSession::<DerivingLang>::new();
    register_core_readers(&mut reader).expect("core readers register");
    reader.push_segment(segment).expect("the segment reads");
    let back = {
        let mut guard = StdDescentGuard::init(&StdDescentGuardInit::default());
        let mut cx = DeserializeContext::new(&mut reader, &mut guard, None);
        T::deserialize_value(&written, &mut cx).expect("reads back")
    };
    (written, source_entries, back)
}

/// The spans of a language-tied type intern their source through the context: one
/// source entry for every span into it, and one shared rebuilt source on the read side.
#[test]
fn a_language_tied_type_passes_the_context_to_its_spans() {
    let source = Arc::new(Source::new("hello world"));
    let hello = SourceSpan::new(&source, 0..5);
    let world = SourceSpan::new(&source, 6..11);

    let value = Located { at: hello.clone(), place: Place::Range { from: world.clone(), to: Some(hello.clone()) } };
    let (written, source_entries, back) = through_sessions(&value);
    assert_eq!(source_entries, 1, "one source, interned once for three spans");
    match &written {
        SerialValue::Map(entries) => {
            assert_eq!(entries[0].0, "at");
            assert!(matches!(entries[0].1, SerialValue::Map(_)), "a span is a map: {:?}", entries[0].1);
            assert_eq!(entries[1].0, "place");
        }
        other => panic!("a map, not {other:?}"),
    }
    assert_eq!(back.at.range(), 0..5);
    assert_eq!(back.at.source().content(), "hello world");
    assert!(!Arc::ptr_eq(back.at.source(), &source), "a rebuilt source, not the writer's");
    match &back.place {
        Place::Range { from, to } => {
            let to = to.as_ref().expect("the present span");
            assert_eq!(from.range(), 6..11);
            assert_eq!(to.range(), 0..5);
            assert!(Arc::ptr_eq(from.source(), back.at.source()), "one rebuilt source, shared");
            assert!(Arc::ptr_eq(to.source(), back.at.source()));
        }
        other => panic!("the range, not {other:?}"),
    }

    // The newtype payload, and an absent optional span.
    let point = Located { at: hello.clone(), place: Place::Point(world.clone()) };
    let (_, source_entries, back) = through_sessions(&point);
    assert_eq!(source_entries, 1);
    match &back.place {
        Place::Point(span) => {
            assert_eq!(span.range(), 6..11);
            assert!(Arc::ptr_eq(span.source(), back.at.source()));
        }
        other => panic!("the point, not {other:?}"),
    }
    let open = Located { at: hello, place: Place::Range { from: world, to: None } };
    let (written, _, back) = through_sessions(&open);
    match &written {
        SerialValue::Map(entries) => match &entries[1].1 {
            SerialValue::Map(range) => match &range[0].1 {
                SerialValue::Map(fields) => assert_eq!(fields.len(), 1, "`to` is left out: {fields:?}"),
                other => panic!("the range's fields, not {other:?}"),
            },
            other => panic!("the range, not {other:?}"),
        },
        other => panic!("a map, not {other:?}"),
    }
    assert!(matches!(back.place, Place::Range { to: None, .. }));
}

// --- the serde bridge ---------------------------------------------------------------------

/// The shapes are the ones the serde bridge produces for the corresponding serde
/// shapes (identical rendering across mechanisms — the canonical-form discipline,
/// [§dd-dr:serial-value-model]): the enum alone, and the whole struct, including a
/// `Some(None)` written as `null` and a verbatim `SerialValue` field.
#[cfg(feature = "serde")]
#[test]
fn shapes_agree_with_the_bridge() {
    use serde::{Deserialize, Serialize};

    #[derive(Serialize, Deserialize)]
    #[serde(deny_unknown_fields)]
    struct BridgeInner {
        id: u32,
        #[serde(skip_serializing_if = "Option::is_none", default)]
        note: Option<String>,
    }
    #[derive(Serialize, Deserialize)]
    enum BridgeKind {
        #[serde(rename = "plain")]
        Plain,
        #[serde(rename = "named")]
        Named(String),
        #[serde(rename = "wrapped")]
        Wrapped(BridgeInner),
        #[serde(rename = "sized")]
        Sized {
            w: u16,
            #[serde(skip_serializing_if = "Option::is_none", default)]
            h: Option<u16>,
        },
    }
    #[derive(Serialize, Deserialize)]
    #[serde(deny_unknown_fields)]
    struct BridgeAll {
        flag: bool,
        small: i8,
        wide: u64,
        count: usize,
        escape: char,
        text: String,
        #[serde(skip_serializing_if = "Option::is_none", default)]
        maybe: Option<u32>,
        items: Vec<i64>,
        ext: SerialValue,
        inner: BridgeInner,
        kinds: Vec<BridgeKind>,
        #[serde(rename = "nested-option", skip_serializing_if = "Option::is_none", default)]
        nested_option: Option<Option<bool>>,
    }

    fn bridge_kinds() -> Vec<BridgeKind> {
        Vec::from([
            BridgeKind::Plain,
            BridgeKind::Named(s("n")),
            BridgeKind::Wrapped(BridgeInner { id: 2, note: None }),
            BridgeKind::Sized { w: 3, h: None },
            BridgeKind::Sized { w: 3, h: Some(4) },
        ])
    }
    fn bridge_all(nested_option: Option<Option<bool>>) -> BridgeAll {
        BridgeAll {
            flag: true,
            small: -3,
            wide: 1 << 40,
            count: 7,
            escape: '\\',
            text: s("hello"),
            maybe: None,
            items: Vec::from([1, -2, 3]),
            ext: map([("k", SerialValue::Bool(false))]),
            inner: BridgeInner { id: 1, note: Some(s("x")) },
            kinds: bridge_kinds(),
            nested_option,
        }
    }

    let via_bridge = crate::serialize::to_value(&bridge_kinds()).unwrap();
    assert_eq!(via_bridge, sample_kinds_value());
    assert_eq!(via_bridge, serialize(&sample().kinds));

    let via_bridge = crate::serialize::to_value(&bridge_all(Some(Some(false)))).unwrap();
    assert_eq!(via_bridge, sample_value());
    assert_eq!(via_bridge, serialize(&sample()));

    let mut some_none = sample();
    some_none.nested_option = Some(None);
    let via_bridge = crate::serialize::to_value(&bridge_all(Some(None))).unwrap();
    assert_eq!(via_bridge, serialize(&some_none));
}
