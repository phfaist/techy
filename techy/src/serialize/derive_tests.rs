//! Tests of the public `#[derive(SerializableValue)]` / `#[derive(DeserializableValue)]`
//! pair, expanded inside the crate itself (the generated code names `::techy::…`,
//! which `extern crate self as techy` resolves here): round trips of a struct with
//! every supported field kind and of an enum with every variant kind, the omission of
//! absent fields, the strict-read failures, and — with the `serde` feature — the
//! identity of the shapes with the serde bridge's. The rejections of unsupported
//! shapes are tested in techy-derive; a consumer crate's use is the integration test
//! `tests/derive_serialize_value.rs`.

use alloc::string::String;
use alloc::vec::Vec;

use crate::engine::{DescentGuard, StdDescentGuard, StdDescentGuardInit};
use crate::state::TrivialLang;

use super::{
    DeserializableValue, DeserializeContext, DeserializeError, SerdeSession, SerialValue,
    SerialValueError, SerializableLang, SerializableValue, SerializeContext,
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

// --- the serde bridge ---------------------------------------------------------------------

/// The shapes are the ones the serde bridge produces for the corresponding serde
/// shapes (identical rendering across mechanisms — the canonical-form discipline,
/// [§dd-dr:serial-value-model]).
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
    let via_bridge = crate::serialize::to_value(&Vec::from([
        BridgeKind::Plain,
        BridgeKind::Named(s("n")),
        BridgeKind::Wrapped(BridgeInner { id: 2, note: None }),
        BridgeKind::Sized { w: 3, h: None },
        BridgeKind::Sized { w: 3, h: Some(4) },
    ]))
    .unwrap();
    assert_eq!(via_bridge, sample_kinds_value());
    assert_eq!(via_bridge, serialize(&sample().kinds));
}
