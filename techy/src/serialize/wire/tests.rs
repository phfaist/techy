//! Tests of the wire conversion traits and their derives: round trips of test-only
//! wire structs covering every supported field kind and every enum variant kind, the
//! omission of absent fields, and the strict-read failures.

use alloc::borrow::Cow;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

use super::{FieldReader, FromSerialValue, SerialBytes, ToSerialValue};
use crate::serialize::{SerialValue, SerialValueError, TableId};

// --- test wire structs -----------------------------------------------------------------------

/// Every supported field kind.
#[derive(ToSerialValue, FromSerialValue, Debug, PartialEq, Clone)]
struct WireAll {
    #[serial(name = "flag")]
    flag: bool,
    #[serial(name = "small")]
    small: i8,
    #[serial(name = "wide")]
    wide: u64,
    #[serial(name = "count")]
    count: usize,
    #[serial(name = "huge")]
    huge: i128,
    #[serial(name = "text")]
    text: String,
    #[serial(name = "label")]
    label: Cow<'static, str>,
    #[serial(name = "maybe")]
    maybe: Option<u32>,
    #[serial(name = "items")]
    items: Vec<i64>,
    #[serial(name = "raw")]
    raw: Vec<u8>,
    #[serial(name = "digest")]
    digest: SerialBytes,
    #[serial(name = "ext")]
    ext: SerialValue,
    #[serial(name = "inner")]
    inner: WireInner,
    #[serial(name = "kinds")]
    kinds: Vec<WireKind>,
    #[serial(name = "nested-option")]
    nested_option: Option<Option<bool>>,
}

/// A nested wire struct.
#[derive(ToSerialValue, FromSerialValue, Debug, PartialEq, Clone)]
struct WireInner {
    #[serial(name = "id")]
    id: u32,
    #[serial(name = "note")]
    note: Option<String>,
}

/// Every enum variant kind.
#[derive(ToSerialValue, FromSerialValue, Debug, PartialEq, Clone)]
enum WireKind {
    #[serial(name = "plain")]
    Plain,
    #[serial(name = "named")]
    Named(String),
    #[serial(name = "wrapped")]
    Wrapped(WireInner),
    #[serial(name = "sized")]
    Sized {
        #[serial(name = "w")]
        width: u16,
        #[serial(name = "h")]
        height: Option<u16>,
    },
}

/// A hand-written implementer of the traits (what a typed table position will do).
#[derive(Debug, PartialEq, Clone, Copy)]
struct TestPosition {
    table: TableId,
    index: u32,
}

impl ToSerialValue for TestPosition {
    fn to_serial_value(&self) -> Result<SerialValue, SerialValueError> {
        Ok(SerialValue::Index { table: self.table, index: self.index })
    }
}

impl FromSerialValue for TestPosition {
    fn from_serial_value(value: &SerialValue) -> Result<Self, SerialValueError> {
        match value {
            SerialValue::Index { table, index } => Ok(TestPosition { table: *table, index: *index }),
            other => Err(SerialValueError::TypeMismatch {
                expected: Cow::Borrowed("a table position"),
                found: other.kind_name(),
            }),
        }
    }
}

#[derive(ToSerialValue, FromSerialValue, Debug, PartialEq)]
struct WireRef {
    #[serial(name = "at")]
    at: TestPosition,
    #[serial(name = "all")]
    all: Vec<TestPosition>,
}

// --- helpers ------------------------------------------------------------------------------------

fn s(text: &str) -> SerialValue {
    SerialValue::Str(String::from(text))
}

fn map<const N: usize>(entries: [(&str, SerialValue); N]) -> SerialValue {
    SerialValue::Map(entries.into_iter().map(|(k, v)| (String::from(k), v)).collect())
}

fn list<const N: usize>(items: [SerialValue; N]) -> SerialValue {
    SerialValue::List(Vec::from(items))
}

fn round_trip<T: ToSerialValue + FromSerialValue + PartialEq + core::fmt::Debug>(value: &T, expected: SerialValue) {
    let serialized = value.to_serial_value().unwrap();
    assert_eq!(serialized, expected);
    let back = T::from_serial_value(&serialized).unwrap();
    assert_eq!(&back, value);
}

fn sample() -> WireAll {
    WireAll {
        flag: true,
        small: -3,
        wide: 1 << 40,
        count: 12,
        huge: -1,
        text: "t".into(),
        label: Cow::Borrowed("static"),
        maybe: Some(5),
        items: Vec::from([1, -2]),
        raw: Vec::from([7, 8]),
        digest: SerialBytes(Vec::from([0xde, 0xad])),
        ext: map([("$weird", SerialValue::Bytes(Vec::new()))]),
        inner: WireInner { id: 9, note: None },
        kinds: Vec::from([
            WireKind::Plain,
            WireKind::Named("n".into()),
            WireKind::Wrapped(WireInner { id: 1, note: Some("x".into()) }),
            WireKind::Sized { width: 3, height: None },
            WireKind::Sized { width: 3, height: Some(4) },
        ]),
        nested_option: Some(None),
    }
}

fn sample_value() -> SerialValue {
    map([
        ("flag", SerialValue::Bool(true)),
        ("small", SerialValue::Int(-3)),
        ("wide", SerialValue::Int(1 << 40)),
        ("count", SerialValue::Int(12)),
        ("huge", SerialValue::Int(-1)),
        ("text", s("t")),
        ("label", s("static")),
        ("maybe", SerialValue::Int(5)),
        ("items", list([SerialValue::Int(1), SerialValue::Int(-2)])),
        ("raw", list([SerialValue::Int(7), SerialValue::Int(8)])),
        ("digest", SerialValue::Bytes(Vec::from([0xde, 0xad]))),
        ("ext", map([("$weird", SerialValue::Bytes(Vec::new()))])),
        ("inner", map([("id", SerialValue::Int(9))])),
        (
            "kinds",
            list([
                s("plain"),
                map([("named", s("n"))]),
                map([("wrapped", map([("id", SerialValue::Int(1)), ("note", s("x"))]))]),
                map([("sized", map([("w", SerialValue::Int(3))]))]),
                map([("sized", map([("w", SerialValue::Int(3)), ("h", SerialValue::Int(4))]))]),
            ]),
        ),
        // `Some(None)` is present with a `Null` value; it reads back as `Some(None)`
        // only through the value model's `Null` → inner `None`.
        ("nested-option", SerialValue::Null),
    ])
}

// --- round trips ------------------------------------------------------------------------------

#[test]
fn every_field_kind_round_trips() {
    let value = sample();
    let serialized = value.to_serial_value().unwrap();
    assert_eq!(serialized, sample_value());
    let back = WireAll::from_serial_value(&serialized).unwrap();
    // `Some(None)` cannot be told from `None` once written as `Null`: it reads back as
    // `None`. Everything else is identical.
    assert_eq!(back, WireAll { nested_option: None, ..value });
}

#[test]
fn absent_option_fields_are_omitted_and_read_back() {
    let value = WireInner { id: 1, note: None };
    round_trip(&value, map([("id", SerialValue::Int(1))]));
    // A present `Null` also reads as `None`.
    assert_eq!(
        WireInner::from_serial_value(&map([("id", SerialValue::Int(1)), ("note", SerialValue::Null)])).unwrap(),
        value
    );
    round_trip(
        &WireInner { id: 1, note: Some("n".into()) },
        map([("id", SerialValue::Int(1)), ("note", s("n"))]),
    );
}

#[test]
fn enum_variants_take_the_bridge_shapes() {
    round_trip(&WireKind::Plain, s("plain"));
    round_trip(&WireKind::Named("a".into()), map([("named", s("a"))]));
    round_trip(
        &WireKind::Wrapped(WireInner { id: 2, note: None }),
        map([("wrapped", map([("id", SerialValue::Int(2))]))]),
    );
    round_trip(
        &WireKind::Sized { width: 1, height: Some(2) },
        map([("sized", map([("w", SerialValue::Int(1)), ("h", SerialValue::Int(2))]))]),
    );
}

#[test]
fn hand_written_implementers_nest() {
    let at = TestPosition { table: TableId::new(1), index: 2 };
    round_trip(
        &WireRef { at, all: Vec::from([at, TestPosition { table: TableId::new(0), index: 0 }]) },
        map([
            ("at", SerialValue::Index { table: TableId::new(1), index: 2 }),
            (
                "all",
                list([
                    SerialValue::Index { table: TableId::new(1), index: 2 },
                    SerialValue::Index { table: TableId::new(0), index: 0 },
                ]),
            ),
        ]),
    );
}

#[test]
fn integers_are_range_checked_both_ways() {
    assert_eq!(
        u64::MAX.to_serial_value(),
        Err(SerialValueError::IntegerOutOfRange { value: u64::MAX.to_string(), target: "i64" })
    );
    assert_eq!(
        (i128::from(i64::MAX) + 1).to_serial_value(),
        Err(SerialValueError::IntegerOutOfRange { value: (i128::from(i64::MAX) + 1).to_string(), target: "i64" })
    );
    assert_eq!(u128::MAX.to_serial_value().unwrap_err(), SerialValueError::IntegerOutOfRange { value: u128::MAX.to_string(), target: "i64" });
    assert_eq!(i64::MIN.to_serial_value(), Ok(SerialValue::Int(i64::MIN)));
    assert_eq!(
        u8::from_serial_value(&SerialValue::Int(256)),
        Err(SerialValueError::IntegerOutOfRange { value: "256".into(), target: "u8" })
    );
    assert_eq!(
        u64::from_serial_value(&SerialValue::Int(-1)),
        Err(SerialValueError::IntegerOutOfRange { value: "-1".into(), target: "u64" })
    );
    assert_eq!(i128::from_serial_value(&SerialValue::Int(i64::MIN)), Ok(i128::from(i64::MIN)));
    // A struct with an unrepresentable field fails as a whole.
    let mut value = sample();
    value.wide = u64::MAX;
    assert!(matches!(value.to_serial_value(), Err(SerialValueError::IntegerOutOfRange { .. })));
}

// --- strict reads ---------------------------------------------------------------------------------

#[test]
fn strict_struct_reads() {
    // Wrong kind for the struct.
    assert_eq!(
        WireInner::from_serial_value(&s("x")).unwrap_err(),
        SerialValueError::TypeMismatch { expected: Cow::Borrowed("a map (WireInner)"), found: "str" }
    );
    // Unknown key.
    assert_eq!(
        WireInner::from_serial_value(&map([("id", SerialValue::Int(1)), ("bogus", SerialValue::Null)])).unwrap_err(),
        SerialValueError::UnknownField { name: "bogus".into(), expected: &["id", "note"] }
    );
    // Missing required key (an absent Option is fine, an absent required key is not).
    assert_eq!(
        WireInner::from_serial_value(&map([("note", s("n"))])).unwrap_err(),
        SerialValueError::MissingField { name: "id" }
    );
    // Repeated key.
    assert_eq!(
        WireInner::from_serial_value(&map([("id", SerialValue::Int(1)), ("id", SerialValue::Int(2))])).unwrap_err(),
        SerialValueError::DuplicateField { name: "id".into() }
    );
    // Wrong kind for a field.
    assert_eq!(
        WireInner::from_serial_value(&map([("id", s("1"))])).unwrap_err(),
        SerialValueError::TypeMismatch { expected: Cow::Borrowed("an integer"), found: "str" }
    );
    // Wrong kind for a nested field, a list, a byte string, an option.
    let mut broken = sample_value();
    if let SerialValue::Map(entries) = &mut broken {
        entries.iter_mut().find(|(k, _)| k == "digest").unwrap().1 = list([SerialValue::Int(1)]);
    }
    assert_eq!(
        WireAll::from_serial_value(&broken).unwrap_err(),
        SerialValueError::TypeMismatch { expected: Cow::Borrowed("a byte string"), found: "list" }
    );
    let mut broken = sample_value();
    if let SerialValue::Map(entries) = &mut broken {
        entries.iter_mut().find(|(k, _)| k == "items").unwrap().1 = SerialValue::Bytes(Vec::new());
    }
    assert_eq!(
        WireAll::from_serial_value(&broken).unwrap_err(),
        SerialValueError::TypeMismatch { expected: Cow::Borrowed("a list"), found: "bytes" }
    );
    let mut broken = sample_value();
    if let SerialValue::Map(entries) = &mut broken {
        entries.iter_mut().find(|(k, _)| k == "maybe").unwrap().1 = SerialValue::Bool(true);
    }
    assert_eq!(
        WireAll::from_serial_value(&broken).unwrap_err(),
        SerialValueError::TypeMismatch { expected: Cow::Borrowed("an integer"), found: "bool" }
    );
    // The `FieldReader` scan is bounded on hostile input: a long map of one repeated
    // known key fails at the first repeat.
    let hostile = SerialValue::Map((0..10_000).map(|_| (String::from("id"), SerialValue::Int(1))).collect());
    assert_eq!(
        FieldReader::new(&hostile, "WireInner", &["id", "note"]).err(),
        Some(SerialValueError::DuplicateField { name: "id".into() })
    );
}

#[test]
fn strict_enum_reads() {
    const VARIANTS: &[&str] = &["plain", "named", "wrapped", "sized"];
    // Unknown variant, as a string and as a map key.
    assert_eq!(
        WireKind::from_serial_value(&s("nope")).unwrap_err(),
        SerialValueError::UnknownVariant { name: "nope".into(), expected: VARIANTS }
    );
    assert_eq!(
        WireKind::from_serial_value(&map([("nope", SerialValue::Null)])).unwrap_err(),
        SerialValueError::UnknownVariant { name: "nope".into(), expected: VARIANTS }
    );
    // Wrong shapes: not a string or map; a map with two entries.
    assert!(matches!(
        WireKind::from_serial_value(&SerialValue::Int(1)).unwrap_err(),
        SerialValueError::TypeMismatch { found: "int", .. }
    ));
    assert!(matches!(
        WireKind::from_serial_value(&map([("plain", SerialValue::Null), ("named", s("x"))])).unwrap_err(),
        SerialValueError::TypeMismatch { found: "map", .. }
    ));
    // Data for a unit variant; no data for a data variant.
    assert!(matches!(
        WireKind::from_serial_value(&map([("plain", SerialValue::Null)])).unwrap_err(),
        SerialValueError::TypeMismatch { found: "map", .. }
    ));
    assert!(matches!(
        WireKind::from_serial_value(&s("named")).unwrap_err(),
        SerialValueError::TypeMismatch { found: "str", .. }
    ));
    // Wrong payload kinds.
    assert_eq!(
        WireKind::from_serial_value(&map([("named", SerialValue::Int(1))])).unwrap_err(),
        SerialValueError::TypeMismatch { expected: Cow::Borrowed("a string"), found: "int" }
    );
    assert_eq!(
        WireKind::from_serial_value(&map([("sized", map([("w", SerialValue::Int(1)), ("d", SerialValue::Int(1))]))])).unwrap_err(),
        SerialValueError::UnknownField { name: "d".into(), expected: &["w", "h"] }
    );
    assert_eq!(
        WireKind::from_serial_value(&map([("sized", s("x"))])).unwrap_err(),
        SerialValueError::TypeMismatch { expected: Cow::Borrowed("a map (WireKind::Sized)"), found: "str" }
    );
}

/// The wire shapes are the ones the serde bridge produces for the corresponding serde
/// shapes (P7: identical rendering across mechanisms).
#[cfg(feature = "serde")]
#[test]
fn shapes_agree_with_the_bridge() {
    use serde::{Deserialize, Serialize};

    #[derive(Serialize, Deserialize)]
    struct BridgeInner {
        id: u32,
        #[serde(skip_serializing_if = "Option::is_none")]
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
            #[serde(skip_serializing_if = "Option::is_none")]
            h: Option<u16>,
        },
    }
    let via_bridge = crate::serialize::to_value(&Vec::from([
        BridgeKind::Plain,
        BridgeKind::Named("n".into()),
        BridgeKind::Wrapped(BridgeInner { id: 1, note: Some("x".into()) }),
        BridgeKind::Sized { w: 3, h: None },
        BridgeKind::Sized { w: 3, h: Some(4) },
    ]))
    .unwrap();
    let via_derive = sample().kinds.to_serial_value().unwrap();
    assert_eq!(via_bridge, via_derive);
}
