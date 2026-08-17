//! Tests of the `serde` rendering layer: the bridge (`to_value`/`from_value`) — its
//! mapping, policy rejections, index/bytes interception, and identity on
//! `SerialValue` — and the `Serialize`/`Deserialize` impls of `SerialValue` through a
//! human-readable format (serde_json: pinned canonical rendering) and a compact one
//! (postcard).

use alloc::borrow::Cow;
use alloc::collections::BTreeMap;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

use proptest::prelude::*;
use serde::{Deserialize, Serialize};

use super::bridge::{deserialize_index, serialize_index, INDEX_SENTINEL};
use super::{from_value, to_value, SerialValue, SerialValueError, TableId};

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

fn index(table: u32, index: u32) -> SerialValue {
    SerialValue::Index { table: TableId::new(table), index }
}

/// A stand-in for a typed table position: what M2's index newtypes will do in their
/// serde impls — route through the sentinel helpers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct TestIndex {
    table: TableId,
    index: u32,
}

impl Serialize for TestIndex {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serialize_index(self.table, self.index, serializer)
    }
}

impl<'de> Deserialize<'de> for TestIndex {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let (table, index) = deserialize_index(deserializer)?;
        Ok(TestIndex { table, index })
    }
}

fn round_trip<T: Serialize + for<'de> Deserialize<'de> + PartialEq + core::fmt::Debug>(
    value: &T,
    expected: SerialValue,
) {
    let serialized = to_value(value).unwrap();
    assert_eq!(serialized, expected);
    let back: T = from_value(&serialized).unwrap();
    assert_eq!(&back, value);
}

// --- the bridge: mapping and round trips ---------------------------------------------------------

#[derive(Serialize, Deserialize, Debug, PartialEq)]
struct Plain {
    name: String,
    count: u32,
    flag: bool,
    ratio: Option<i64>,
    tags: Vec<String>,
    pair: (u8, char),
}

#[derive(Serialize, Deserialize, Debug, PartialEq)]
struct Nested {
    inner: Plain,
    others: Vec<Plain>,
    lookup: BTreeMap<String, u16>,
    unit: (),
    marker: Marker,
    wrapped: Wrapper,
}

#[derive(Serialize, Deserialize, Debug, PartialEq)]
struct Marker;

#[derive(Serialize, Deserialize, Debug, PartialEq)]
struct Wrapper(i32);

#[derive(Serialize, Deserialize, Debug, PartialEq)]
enum Shape {
    Unit,
    Newtype(String),
    Tuple(u8, bool),
    Struct { width: u32, label: Option<String> },
}

fn sample_plain() -> Plain {
    Plain {
        name: "abc".into(),
        count: 7,
        flag: true,
        ratio: None,
        tags: Vec::from([String::from("x"), String::from("y")]),
        pair: (3, 'q'),
    }
}

#[test]
fn struct_maps_to_map_in_field_order() {
    round_trip(
        &sample_plain(),
        map([
            ("name", s("abc")),
            ("count", SerialValue::Int(7)),
            ("flag", SerialValue::Bool(true)),
            ("ratio", SerialValue::Null),
            ("tags", list([s("x"), s("y")])),
            ("pair", list([SerialValue::Int(3), s("q")])),
        ]),
    );
}

#[test]
fn nested_structs_maps_units_and_newtypes() {
    let value = Nested {
        inner: sample_plain(),
        others: Vec::from([Plain { ratio: Some(-5), ..sample_plain() }]),
        lookup: BTreeMap::from([(String::from("k"), 65535u16)]),
        unit: (),
        marker: Marker,
        wrapped: Wrapper(-1),
    };
    let expected_plain = to_value(&sample_plain()).unwrap();
    let mut other = to_value(&sample_plain()).unwrap();
    if let SerialValue::Map(entries) = &mut other {
        entries[3].1 = SerialValue::Int(-5);
    }
    round_trip(
        &value,
        map([
            ("inner", expected_plain),
            ("others", list([other])),
            ("lookup", map([("k", SerialValue::Int(65535))])),
            ("unit", SerialValue::Null),
            ("marker", SerialValue::Null),
            ("wrapped", SerialValue::Int(-1)),
        ]),
    );
}

#[test]
fn enums_take_the_externally_tagged_form() {
    round_trip(&Shape::Unit, s("Unit"));
    round_trip(&Shape::Newtype("n".into()), map([("Newtype", s("n"))]));
    round_trip(
        &Shape::Tuple(2, false),
        map([("Tuple", list([SerialValue::Int(2), SerialValue::Bool(false)]))]),
    );
    round_trip(
        &Shape::Struct { width: 9, label: Some("w".into()) },
        map([("Struct", map([("width", SerialValue::Int(9)), ("label", s("w"))]))]),
    );
    // A list of every variant kind.
    let all = Vec::from([
        Shape::Unit,
        Shape::Newtype("a".into()),
        Shape::Tuple(0, true),
        Shape::Struct { width: 1, label: None },
    ]);
    let value = to_value(&all).unwrap();
    assert_eq!(from_value::<Vec<Shape>>(&value).unwrap(), all);
}

#[test]
fn options_and_integers_of_every_width() {
    round_trip(&Some(3u8), SerialValue::Int(3));
    round_trip(&None::<u8>, SerialValue::Null);
    round_trip(&Some(Some(1i8)), SerialValue::Int(1));
    round_trip(&i8::MIN, SerialValue::Int(-128));
    round_trip(&i16::MAX, SerialValue::Int(32767));
    round_trip(&i32::MIN, SerialValue::Int(i64::from(i32::MIN)));
    round_trip(&i64::MAX, SerialValue::Int(i64::MAX));
    round_trip(&u8::MAX, SerialValue::Int(255));
    round_trip(&u16::MAX, SerialValue::Int(65535));
    round_trip(&u32::MAX, SerialValue::Int(i64::from(u32::MAX)));
    round_trip(&(i64::MAX as u64), SerialValue::Int(i64::MAX));
    round_trip(&(-7i128), SerialValue::Int(-7));
    round_trip(&(9u128), SerialValue::Int(9));
    round_trip(&'é', s("é"));
    round_trip(&String::from("text"), s("text"));
    round_trip(&(), SerialValue::Null);
    round_trip(&true, SerialValue::Bool(true));
}

#[test]
fn maps_with_string_like_keys() {
    #[derive(Serialize, Deserialize, Debug, PartialEq, PartialOrd, Ord, Eq)]
    enum Key {
        Alpha,
        Beta,
    }
    #[derive(Serialize, Deserialize, Debug, PartialEq, PartialOrd, Ord, Eq)]
    struct Name(String);

    round_trip(
        &BTreeMap::from([('a', 1u8), ('b', 2u8)]),
        map([("a", SerialValue::Int(1)), ("b", SerialValue::Int(2))]),
    );
    round_trip(
        &BTreeMap::from([(Key::Alpha, 1u8), (Key::Beta, 2u8)]),
        map([("Alpha", SerialValue::Int(1)), ("Beta", SerialValue::Int(2))]),
    );
    round_trip(
        &BTreeMap::from([(Name("n".into()), true)]),
        map([("n", SerialValue::Bool(true))]),
    );
}

/// A `with` module reading a borrowed byte string, to show the bridge lends bytes.
mod serial_bytes_borrowed {
    pub fn deserialize<'de, D: serde::Deserializer<'de>>(d: D) -> Result<&'de [u8], D::Error> {
        struct V;
        impl<'de> serde::de::Visitor<'de> for V {
            type Value = &'de [u8];
            fn expecting(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
                f.write_str("borrowed bytes")
            }
            fn visit_borrowed_bytes<E: serde::de::Error>(self, v: &'de [u8]) -> Result<&'de [u8], E> {
                Ok(v)
            }
        }
        d.deserialize_bytes(V)
    }
}

#[test]
fn borrowed_strings_and_bytes_are_borrowed_from_the_value() {
    #[derive(Deserialize, Debug, PartialEq)]
    struct Borrowed<'a> {
        name: &'a str,
        #[serde(deserialize_with = "serial_bytes_borrowed::deserialize", borrow)]
        data: &'a [u8],
    }
    let value = map([("name", s("borrowed")), ("data", SerialValue::Bytes(Vec::from([1, 2])))]);
    let read: Borrowed<'_> = from_value(&value).unwrap();
    assert_eq!(read, Borrowed { name: "borrowed", data: &[1, 2] });
}

// --- the bridge: policy rejections ------------------------------------------------------------------

#[test]
fn floats_are_rejected_both_ways() {
    assert_eq!(to_value(&1.5f64), Err(SerialValueError::FloatRejected));
    assert_eq!(to_value(&1.5f32), Err(SerialValueError::FloatRejected));
    #[derive(Serialize, Deserialize, Debug)]
    struct WithFloat {
        x: f64,
    }
    assert_eq!(to_value(&WithFloat { x: 0.0 }), Err(SerialValueError::FloatRejected));
    assert_eq!(
        from_value::<WithFloat>(&map([("x", SerialValue::Int(0))])).map(|w| w.x.to_bits()),
        Err(SerialValueError::FloatRejected)
    );
    assert_eq!(from_value::<f32>(&SerialValue::Int(1)).map(|f| f.to_bits()), Err(SerialValueError::FloatRejected));
}

#[test]
fn integers_out_of_i64_are_rejected() {
    assert_eq!(
        to_value(&u64::MAX),
        Err(SerialValueError::IntegerOutOfRange { value: u64::MAX.to_string(), target: "i64" })
    );
    assert_eq!(
        to_value(&(i64::MAX as u64 + 1)),
        Err(SerialValueError::IntegerOutOfRange {
            value: (i64::MAX as u64 + 1).to_string(),
            target: "i64"
        })
    );
    assert_eq!(
        to_value(&(i128::from(i64::MIN) - 1)),
        Err(SerialValueError::IntegerOutOfRange {
            value: (i128::from(i64::MIN) - 1).to_string(),
            target: "i64"
        })
    );
    assert_eq!(
        to_value(&u128::MAX),
        Err(SerialValueError::IntegerOutOfRange { value: u128::MAX.to_string(), target: "i64" })
    );
    // On reading, the bridge range-checks against the target width itself.
    assert_eq!(
        from_value::<u8>(&SerialValue::Int(300)),
        Err(SerialValueError::IntegerOutOfRange { value: "300".into(), target: "u8" })
    );
    assert_eq!(
        from_value::<u32>(&SerialValue::Int(-1)),
        Err(SerialValueError::IntegerOutOfRange { value: "-1".into(), target: "u32" })
    );
    assert_eq!(
        from_value::<i16>(&SerialValue::Int(-40000)),
        Err(SerialValueError::IntegerOutOfRange { value: "-40000".into(), target: "i16" })
    );
    #[derive(Deserialize, Debug, PartialEq)]
    struct Narrow {
        n: i8,
    }
    assert_eq!(
        from_value::<Narrow>(&map([("n", SerialValue::Int(128))])),
        Err(SerialValueError::IntegerOutOfRange { value: "128".into(), target: "i8" })
    );
    assert_eq!(from_value::<Narrow>(&map([("n", SerialValue::Int(-128))])), Ok(Narrow { n: -128 }));
    // A non-integer where an integer is read is still a kind mismatch.
    assert!(from_value::<u8>(&s("7")).is_err());
}

#[test]
fn structs_read_from_maps_only() {
    // A list is not read positionally into a struct.
    assert!(matches!(
        from_value::<Plain>(&list([
            s("abc"),
            SerialValue::Int(7),
            SerialValue::Bool(true),
            SerialValue::Null,
            list([]),
            list([SerialValue::Int(3), s("q")]),
        ]))
        .unwrap_err(),
        SerialValueError::TypeMismatch { found: "list", .. }
    ));
    assert!(matches!(
        from_value::<Plain>(&SerialValue::Null).unwrap_err(),
        SerialValueError::TypeMismatch { found: "null", .. }
    ));
    // Tuples and tuple structs still read from lists.
    #[derive(Deserialize, Debug, PartialEq)]
    struct Pair(u8, u8);
    assert_eq!(from_value::<Pair>(&list([SerialValue::Int(1), SerialValue::Int(2)])), Ok(Pair(1, 2)));
}

#[test]
fn non_string_map_keys_are_rejected() {
    assert_eq!(
        to_value(&BTreeMap::from([(1u32, "a")])),
        Err(SerialValueError::NonStringMapKey)
    );
    assert_eq!(
        to_value(&BTreeMap::from([(true, "a")])),
        Err(SerialValueError::NonStringMapKey)
    );
    assert_eq!(
        to_value(&BTreeMap::from([((1u8, 2u8), "a")])),
        Err(SerialValueError::NonStringMapKey)
    );
    assert_eq!(
        to_value(&BTreeMap::from([(Some("k"), "a")])),
        Err(SerialValueError::NonStringMapKey)
    );
    // Reading an integer-keyed map from a string-keyed value fails symmetrically.
    assert!(from_value::<BTreeMap<u32, u8>>(&map([("1", SerialValue::Int(1))])).is_err());
}

#[test]
fn shape_mismatches_are_reported() {
    assert_eq!(
        from_value::<Plain>(&s("not a map")).unwrap_err(),
        SerialValueError::TypeMismatch { expected: Cow::Borrowed("a map (struct Plain)"), found: "str" }
    );
    assert_eq!(
        from_value::<Option<u8>>(&s("x")).unwrap_err(),
        SerialValueError::Custom("invalid type: string \"x\", expected u8".into())
    );
    assert!(matches!(
        from_value::<Shape>(&SerialValue::Int(1)).unwrap_err(),
        SerialValueError::TypeMismatch { found: "int", .. }
    ));
    assert!(matches!(
        from_value::<Shape>(&map([("Unit", SerialValue::Null), ("Extra", SerialValue::Null)])).unwrap_err(),
        SerialValueError::TypeMismatch { found: "map", .. }
    ));
    assert_eq!(
        from_value::<Shape>(&s("Nope")).unwrap_err(),
        SerialValueError::UnknownVariant {
            name: "Nope".into(),
            expected: &["Unit", "Newtype", "Tuple", "Struct"]
        }
    );
    // Data for a unit variant, and no data for a newtype variant.
    assert!(matches!(
        from_value::<Shape>(&map([("Unit", SerialValue::Null)])).unwrap_err(),
        SerialValueError::TypeMismatch { found: "null", .. }
    ));
    assert!(matches!(
        from_value::<Shape>(&s("Newtype")).unwrap_err(),
        SerialValueError::TypeMismatch { found: "str", .. }
    ));
    // serde's field errors surface as the dedicated variants.
    assert_eq!(
        from_value::<Plain>(&map([("name", s("a"))])).unwrap_err(),
        SerialValueError::MissingField { name: "count" }
    );
    #[derive(Deserialize, Debug)]
    #[serde(deny_unknown_fields)]
    struct Strict {
        #[allow(dead_code)]
        a: u8,
    }
    assert_eq!(
        from_value::<Strict>(&map([("a", SerialValue::Int(1)), ("b", SerialValue::Int(2))])).unwrap_err(),
        SerialValueError::UnknownField { name: "b".into(), expected: &["a"] }
    );
    assert_eq!(
        from_value::<Strict>(&map([("a", SerialValue::Int(1)), ("a", SerialValue::Int(2))])).unwrap_err(),
        SerialValueError::DuplicateField { name: "a".into() }
    );
    // Trailing elements are not silently dropped.
    assert!(from_value::<(u8, u8)>(&list([SerialValue::Int(1), SerialValue::Int(2), SerialValue::Int(3)])).is_err());
}

// --- the bridge: index and bytes interception --------------------------------------------------------

#[test]
fn index_sentinel_maps_to_index() {
    let position = TestIndex { table: TableId::new(2), index: 40 };
    round_trip(&position, index(2, 40));

    #[derive(Serialize, Deserialize, Debug, PartialEq)]
    struct Refers {
        spec: TestIndex,
        others: Vec<TestIndex>,
        maybe: Option<TestIndex>,
    }
    round_trip(
        &Refers {
            spec: position,
            others: Vec::from([TestIndex { table: TableId::new(0), index: 0 }]),
            maybe: None,
        },
        map([("spec", index(2, 40)), ("others", list([index(0, 0)])), ("maybe", SerialValue::Null)]),
    );

    // Any other newtype struct is transparent — including one wrapping a pair.
    #[derive(Serialize, Deserialize, Debug, PartialEq)]
    struct NotAnIndex((u32, u32));
    round_trip(&NotAnIndex((1, 2)), list([SerialValue::Int(1), SerialValue::Int(2)]));

    // Reading an index into anything but a typed position is an error.
    assert!(matches!(
        from_value::<(u32, u32)>(&index(1, 2)).unwrap_err(),
        SerialValueError::TypeMismatch { found: "index", .. }
    ));
    assert!(matches!(
        from_value::<TestIndex>(&list([SerialValue::Int(1), SerialValue::Int(2)])).unwrap_err(),
        SerialValueError::TypeMismatch { found: "list", .. }
    ));
    assert_eq!(INDEX_SENTINEL, "techy::serialize::Index");
}

#[test]
fn bytes_travel_through_the_bytes_channel() {
    #[derive(Serialize, Deserialize, Debug, PartialEq)]
    struct Digest {
        #[serde(with = "super::serial_bytes")]
        bytes: Vec<u8>,
        plain: Vec<u8>,
    }
    round_trip(
        &Digest { bytes: Vec::from([0, 255]), plain: Vec::from([0, 255]) },
        map([
            ("bytes", SerialValue::Bytes(Vec::from([0, 255]))),
            ("plain", list([SerialValue::Int(0), SerialValue::Int(255)])),
        ]),
    );
    // Reading is strict in both directions: a list is not a byte string and a byte
    // string is not a list.
    assert!(matches!(
        from_value::<Digest>(&map([
            ("bytes", list([SerialValue::Int(0)])),
            ("plain", list([])),
        ]))
        .unwrap_err(),
        SerialValueError::TypeMismatch { found: "list", .. }
    ));
    assert!(from_value::<Digest>(&map([
        ("bytes", SerialValue::Bytes(Vec::new())),
        ("plain", SerialValue::Bytes(Vec::new())),
    ]))
    .is_err());
}

// --- the bridge: SerialValue converts to itself ---------------------------------------------------

fn arbitrary_value() -> impl Strategy<Value = SerialValue> {
    let leaf = prop_oneof![
        Just(SerialValue::Null),
        any::<bool>().prop_map(SerialValue::Bool),
        any::<i64>().prop_map(SerialValue::Int),
        ".{0,12}".prop_map(SerialValue::Str),
        proptest::collection::vec(any::<u8>(), 0..8).prop_map(SerialValue::Bytes),
        (any::<u32>(), any::<u32>()).prop_map(|(t, i)| index(t, i)),
    ];
    leaf.prop_recursive(4, 32, 6, |inner| {
        prop_oneof![
            proptest::collection::vec(inner.clone(), 0..6).prop_map(SerialValue::List),
            // Keys never begin with `$` (the reserved prefix), but may contain it.
            proptest::collection::vec(("[a-z][$a-z]{0,3}|", inner), 0..6)
                .prop_map(SerialValue::Map),
        ]
    })
}

proptest! {
    /// Identity through the bridge, and identity through both renderings.
    #[test]
    fn serial_value_round_trips(value in arbitrary_value()) {
        let through_bridge = to_value(&value).unwrap();
        prop_assert_eq!(&through_bridge, &value);
        let back: SerialValue = from_value(&through_bridge).unwrap();
        prop_assert_eq!(&back, &value);

        let json = serde_json::to_string(&value).unwrap();
        let from_json: SerialValue = serde_json::from_str(&json).unwrap();
        prop_assert_eq!(&from_json, &value);

        let bytes = postcard::to_allocvec(&value).unwrap();
        let from_postcard: SerialValue = postcard::from_bytes(&bytes).unwrap();
        prop_assert_eq!(&from_postcard, &value);
    }
}

#[test]
fn serial_value_inside_a_payload_is_verbatim() {
    #[derive(Serialize, Deserialize, Debug, PartialEq)]
    struct Payload {
        ext: SerialValue,
        many: Vec<SerialValue>,
    }
    let payload = Payload {
        ext: map([("k$", SerialValue::Bytes(Vec::from([1])))]),
        many: Vec::from([SerialValue::Null, index(3, 4), s("$$")]),
    };
    round_trip(
        &payload,
        map([
            ("ext", map([("k$", SerialValue::Bytes(Vec::from([1])))])),
            ("many", list([SerialValue::Null, index(3, 4), s("$$")])),
        ]),
    );
}

#[test]
fn a_map_key_beginning_with_dollar_is_rejected_everywhere() {
    // The value model reserves the `$` prefix; there is no escaping. A `Map` holding
    // such a key is refused by every rendering and by the bridge — on writing and on
    // reading alike — while `$` anywhere else in a key is ordinary.
    let offending = map([("a", SerialValue::Null), ("$k", SerialValue::Int(1))]);
    let expected = SerialValueError::ReservedMapKey { key: "$k".into() };
    assert_eq!(to_value(&offending).unwrap_err(), expected);
    assert!(serde_json::to_string(&offending).unwrap_err().to_string().contains("reserved"));
    // (postcard keeps no message; the failure itself is what matters.)
    assert!(postcard::to_allocvec(&offending).is_err());
    // Nested inside a payload: the same.
    #[derive(Serialize)]
    struct Payload {
        ext: SerialValue,
    }
    assert_eq!(to_value(&Payload { ext: offending.clone() }).unwrap_err(), expected);
    // Struct fields, map keys, and variant names of serde types are map keys too.
    #[derive(Serialize)]
    struct Renamed {
        #[serde(rename = "$field")]
        field: u8,
    }
    assert_eq!(to_value(&Renamed { field: 1 }).unwrap_err(), SerialValueError::ReservedMapKey { key: "$field".into() });
    #[derive(Serialize)]
    enum Tagged {
        #[serde(rename = "$v")]
        V(u8),
    }
    assert_eq!(to_value(&Tagged::V(1)).unwrap_err(), SerialValueError::ReservedMapKey { key: "$v".into() });
    let keyed: std::collections::BTreeMap<&str, u8> = [("$m", 1)].into_iter().collect();
    assert_eq!(to_value(&keyed).unwrap_err(), SerialValueError::ReservedMapKey { key: "$m".into() });
    // Reading the compact form: a `$` key inside a map payload is refused too. The
    // encoding of `{"k$": 1}` reads fine; swapping the key's bytes to `$k` (the same
    // length, so only those two bytes change) makes it hostile.
    let mut bytes = postcard::to_allocvec(&map([("k$", SerialValue::Int(1))])).unwrap();
    assert!(postcard::from_bytes::<SerialValue>(&bytes).is_ok());
    let position = bytes.windows(2).position(|w| w == b"k$").unwrap();
    bytes[position] = b'$';
    bytes[position + 1] = b'k';
    assert!(postcard::from_bytes::<SerialValue>(&bytes).is_err());
}

#[test]
fn map_equality_and_rendering_are_order_sensitive() {
    // A `Map` is an ordered list of entries: the same entries in another order are a
    // different value with a different rendering.
    let ab = map([("a", SerialValue::Int(1)), ("b", SerialValue::Int(2))]);
    let ba = map([("b", SerialValue::Int(2)), ("a", SerialValue::Int(1))]);
    assert_ne!(ab, ba);
    assert_eq!(json(&ab), r#"{"a":1,"b":2}"#);
    assert_eq!(json(&ba), r#"{"b":2,"a":1}"#);
    assert_ne!(json(&ab), json(&ba));
    // And each reads back as itself, order included.
    assert_eq!(serde_json::from_str::<SerialValue>(&json(&ba)).unwrap(), ba);
    assert_ne!(postcard::to_allocvec(&ab).unwrap(), postcard::to_allocvec(&ba).unwrap());
}

// --- the canonical JSON rendering (provisional until the wire vocabulary is finalized) -----------

fn json(value: &SerialValue) -> String {
    serde_json::to_string(value).unwrap()
}

#[test]
fn json_snapshots() {
    assert_eq!(json(&SerialValue::Null), "null");
    assert_eq!(json(&SerialValue::Bool(false)), "false");
    assert_eq!(json(&SerialValue::Int(-42)), "-42");
    assert_eq!(json(&SerialValue::Int(i64::MAX)), "9223372036854775807");
    assert_eq!(json(&s("a\"b")), r#""a\"b""#);
    assert_eq!(json(&list([])), "[]");
    assert_eq!(json(&map([])), "{}");
    assert_eq!(json(&SerialValue::Bytes(Vec::new())), r#"{"$bytes":""}"#);
    assert_eq!(json(&SerialValue::Bytes(b"foobar".to_vec())), r#"{"$bytes":"Zm9vYmFy"}"#);
    assert_eq!(json(&SerialValue::Bytes(Vec::from([0xff, 0xfe]))), r#"{"$bytes":"//4="}"#);
    assert_eq!(json(&index(0, 7)), r#"{"$index":[0,7]}"#);
    assert_eq!(json(&index(u32::MAX, u32::MAX)), r#"{"$index":[4294967295,4294967295]}"#);
    // No escaping: `$` inside a key is ordinary, a leading `$` is an error.
    assert_eq!(json(&map([("a$", SerialValue::Null), ("b$$x", SerialValue::Null)])), r#"{"a$":null,"b$$x":null}"#);
    assert!(serde_json::to_string(&map([("$bytes", s("not bytes"))])).is_err());
    // Nested, order-preserving.
    let nested = map([
        ("z", list([SerialValue::Int(1), map([("k$", index(1, 2))]), SerialValue::Bytes(Vec::from([0]))])),
        ("a", map([])),
    ]);
    assert_eq!(
        json(&nested),
        r#"{"z":[1,{"k$":{"$index":[1,2]}},{"$bytes":"AA=="}],"a":{}}"#
    );
    // And every snapshot reads back.
    for value in [
        SerialValue::Null,
        SerialValue::Bytes(b"foobar".to_vec()),
        index(u32::MAX, 3),
        map([("a$", SerialValue::Null), ("b$$x", SerialValue::Null)]),
        nested,
    ] {
        let back: SerialValue = serde_json::from_str(&json(&value)).unwrap();
        assert_eq!(back, value);
    }
}

#[test]
fn json_reader_accepts_only_the_canonical_forms() {
    fn read(text: &str) -> Result<SerialValue, String> {
        serde_json::from_str::<SerialValue>(text).map_err(|e| e.to_string())
    }
    // Plain JSON maps onto the model.
    assert_eq!(read(r#"{"a":[1,true,null,"s"]}"#).unwrap(), map([("a", list([SerialValue::Int(1), SerialValue::Bool(true), SerialValue::Null, s("s")]))]));
    // Bad base64.
    assert!(read(r#"{"$bytes":"Zg"}"#).unwrap_err().contains("multiple of four"));
    assert!(read(r#"{"$bytes":"Zh=="}"#).unwrap_err().contains("canonical"));
    assert!(read(r#"{"$bytes":"Zm9v-g=="}"#).unwrap_err().contains("invalid base64 character"));
    assert!(read(r#"{"$bytes":5}"#).is_err());
    assert!(read(r#"{"$bytes":"","x":1}"#).unwrap_err().contains("one entry only"));
    // Malformed `$index`.
    assert!(read(r#"{"$index":[1]}"#).is_err());
    assert!(read(r#"{"$index":[1,2,3]}"#).is_err());
    assert!(read(r#"{"$index":[-1,2]}"#).is_err());
    assert!(read(r#"{"$index":[1,4294967296]}"#).is_err());
    assert!(read(r#"{"$index":"1,2"}"#).is_err());
    assert!(read(r#"{"$index":[1,2],"more":0}"#).unwrap_err().contains("one entry only"));
    // A reserved key that is not first, or any other `$` key: fail closed (there is no
    // escaping, so `$$…` is an error too).
    assert!(read(r#"{"a":1,"$bytes":""}"#).unwrap_err().contains("reserved"));
    assert!(read(r#"{"$other":1}"#).unwrap_err().contains("reserved"));
    assert!(read(r#"{"$":1}"#).unwrap_err().contains("reserved"));
    assert!(read(r#"{"$$k":1}"#).unwrap_err().contains("reserved"));
    assert!(read(r#"{"a":{"$$":1}}"#).unwrap_err().contains("reserved"));
    // Numbers outside the model.
    assert!(read("1.5").unwrap_err().contains("floating-point"));
    assert!(read("1e3").unwrap_err().contains("floating-point"));
    assert!(read("9223372036854775808").unwrap_err().contains("does not fit i64"));
    assert!(read("[1.0]").unwrap_err().contains("floating-point"));
}

// --- the compact rendering (postcard) ------------------------------------------------------------------

#[test]
fn compact_rendering_round_trips_every_variant() {
    let values = [
        SerialValue::Null,
        SerialValue::Bool(true),
        SerialValue::Int(i64::MIN),
        s("héllo"),
        SerialValue::Bytes(Vec::from([0, 1, 2, 255])),
        list([SerialValue::Null, index(1, 2)]),
        map([("k$", SerialValue::Bytes(Vec::new())), ("", list([]))]),
        index(u32::MAX, 0),
    ];
    for value in values {
        let bytes = postcard::to_allocvec(&value).unwrap();
        let back: SerialValue = postcard::from_bytes(&bytes).unwrap();
        assert_eq!(back, value);
    }
    // Compactness sanity: bytes are not expanded to a list of integers.
    let bytes = postcard::to_allocvec(&SerialValue::Bytes(Vec::from([7; 100]))).unwrap();
    assert!(bytes.len() < 110, "{}", bytes.len());
    // Truncated input is a clean error.
    assert!(postcard::from_bytes::<SerialValue>(&bytes[..bytes.len() / 2]).is_err());
    // A payload struct holding values, through the compact format.
    #[derive(Serialize, Deserialize, Debug, PartialEq)]
    struct Holder {
        a: SerialValue,
        b: Vec<SerialValue>,
    }
    let holder = Holder { a: index(1, 1), b: Vec::from([SerialValue::Bytes(Vec::from([9])), s("x")]) };
    let bytes = postcard::to_allocvec(&holder).unwrap();
    assert_eq!(postcard::from_bytes::<Holder>(&bytes).unwrap(), holder);
}

// --- SerialValueError as a serde error -------------------------------------------------------------

#[test]
fn custom_errors_from_impls_are_kept() {
    struct Failing;
    impl Serialize for Failing {
        fn serialize<S: serde::Serializer>(&self, _: S) -> Result<S::Ok, S::Error> {
            Err(serde::ser::Error::custom("refused"))
        }
    }
    assert_eq!(to_value(&Failing), Err(SerialValueError::Custom("refused".into())));
    let display = SerialValueError::UnknownField { name: "b".into(), expected: &["a", "c"] }.to_string();
    assert_eq!(display, "unknown key `b`; expected one of `a`, `c`");
    let display = SerialValueError::TypeMismatch { expected: Cow::Borrowed("a byte string"), found: "list" }.to_string();
    assert_eq!(display, "expected a byte string, found list");
}
