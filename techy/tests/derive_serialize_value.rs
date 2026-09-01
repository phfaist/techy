//! End-to-end exercise of `#[derive(SerializableValue)]` / `#[derive(DeserializableValue)]`
//! from a consumer crate. This file plays the third-party role: a language of its own
//! opts in with `SerializableLang`, derives the two value conversions for its own
//! types, and runs them through the public engine — a writer session interns a spec
//! carrying the values, a reader session rebuilds it from the segment — exactly the
//! surface a downstream language crate uses. Covered: the serialized form (wire names
//! in declaration order, an absent `Option` field left out), the round trip through
//! real contexts, reading `None` back from an omitted key and from `null`, the
//! strict-read failures with their typed errors, and — with the `serde` feature — the
//! identity of the shapes with the serde bridge's.

use std::sync::{Arc, Mutex};

use techy::core::specs::CallableSpec;
use techy::core::TrivialLang;
use techy::serialize::{
    register_core_readers, DeserializableObject, DeserializableValue, DeserializeContext,
    DeserializeError, KnownProviders, SerdeSession, SerialEntry, SerialIndex, SerialValue,
    SerialValueError, SerializableLang, SerializableObject, SerializableValue, SerializeContext,
    SerializeError, StandardTableInterning, StandardTableReading,
};

// --- the language ---------------------------------------------------------------------

/// A language on the crate's defaults, opted in.
#[derive(Debug, Clone, Copy)]
struct MyLang;
impl TrivialLang for MyLang {}
impl SerializableLang for MyLang {}

// --- the derived types ------------------------------------------------------------------
//
// Every wire name differs from the Rust identifier: the serialized form must come from
// the attributes, never from the field or variant names.

/// Every supported field kind.
#[derive(SerializableValue, DeserializableValue, Debug, PartialEq, Clone)]
struct Record {
    #[serial(name = "on")]
    enabled: bool,
    #[serial(name = "tiny")]
    tiny: i8,
    #[serial(name = "big")]
    big: u64,
    #[serial(name = "n")]
    count: usize,
    #[serial(name = "esc")]
    escape: char,
    #[serial(name = "label")]
    text: String,
    #[serial(name = "extra")]
    extra: Option<u32>,
    #[serial(name = "list")]
    items: Vec<i64>,
    #[serial(name = "part")]
    part: Part,
    #[serial(name = "raw")]
    verbatim: SerialValue,
    #[serial(name = "shape")]
    shape: Shape,
}

/// A nested derived struct with an optional field.
#[derive(SerializableValue, DeserializableValue, Debug, PartialEq, Clone)]
struct Part {
    #[serial(name = "id")]
    number: u32,
    #[serial(name = "note")]
    remark: Option<String>,
}

/// Every supported variant kind.
#[derive(SerializableValue, DeserializableValue, Debug, PartialEq, Clone)]
enum Shape {
    #[serial(name = "dot")]
    Dot,
    #[serial(name = "boxed")]
    Boxed(Part),
    #[serial(name = "rect")]
    Rect {
        #[serial(name = "w")]
        width: u16,
        #[serial(name = "h")]
        height: Option<u16>,
    },
}

// --- helpers ------------------------------------------------------------------------------

fn s(text: &str) -> String {
    String::from(text)
}

fn map(entries: impl IntoIterator<Item = (&'static str, SerialValue)>) -> SerialValue {
    SerialValue::Map(entries.into_iter().map(|(key, value)| (s(key), value)).collect())
}

fn sample() -> Record {
    Record {
        enabled: true,
        tiny: -3,
        big: 1 << 40,
        count: 7,
        escape: '\\',
        text: s("hello"),
        extra: None,
        items: vec![1, -2, 3],
        part: Part { number: 1, remark: Some(s("x")) },
        verbatim: map([("k", SerialValue::Bool(false))]),
        shape: Shape::Rect { width: 3, height: None },
    }
}

/// The serialized form of [`sample`].
fn sample_value() -> SerialValue {
    map([
        ("on", SerialValue::Bool(true)),
        ("tiny", SerialValue::Int(-3)),
        ("big", SerialValue::Int(1 << 40)),
        ("n", SerialValue::Int(7)),
        ("esc", SerialValue::Str(s("\\"))),
        ("label", SerialValue::Str(s("hello"))),
        // `extra: None` is left out.
        ("list", SerialValue::List(vec![SerialValue::Int(1), SerialValue::Int(-2), SerialValue::Int(3)])),
        ("part", map([("id", SerialValue::Int(1)), ("note", SerialValue::Str(s("x")))])),
        ("raw", map([("k", SerialValue::Bool(false))])),
        ("shape", map([("rect", map([("w", SerialValue::Int(3))]))])),
    ])
}

fn keys_of(value: &SerialValue) -> Vec<&str> {
    match value {
        SerialValue::Map(entries) => entries.iter().map(|(k, _)| k.as_str()).collect(),
        other => panic!("a map, not {other:?}"),
    }
}

// --- carrying a value through the public engine ------------------------------------------
//
// A value is never a table entry of its own: it travels inside an object. The carrier
// below is a spec (the object kind a consumer can define and register a reader for)
// holding one value; writing it serializes the value with a real `SerializeContext`,
// and reading it back deserializes with a real `DeserializeContext`. The read side
// hands the rebuilt value out through the registration closure, since a
// `dyn CallableSpec` offers no downcast.

const CARRIER: &str = "test.carrier";

/// The spec that carries a value of type `T` — and records what it wrote.
#[derive(Debug)]
struct Carrier<T> {
    value: T,
    written: Mutex<Option<SerialValue>>,
}

impl<T> Carrier<T> {
    fn new(value: T) -> Carrier<T> {
        Carrier { value, written: Mutex::new(None) }
    }
}

impl<T: SerializableValue<MyLang> + Send + Sync + 'static + std::fmt::Debug> SerializableObject<MyLang> for Carrier<T> {
    fn serialize_object(&self, cx: &mut SerializeContext<'_, MyLang>) -> Result<SerialEntry, SerializeError> {
        let data = self.value.serialize_value(cx)?;
        *self.written.lock().unwrap() = Some(data.clone());
        Ok(SerialEntry { identifier: CARRIER.into(), data })
    }
}

impl<T: DeserializableValue<MyLang> + Send + Sync + 'static + std::fmt::Debug> DeserializableObject<MyLang> for Carrier<T> {
    type Output = T;
    fn deserialize_object(value: &SerialValue, cx: &mut DeserializeContext<'_, MyLang>) -> Result<T, DeserializeError> {
        T::deserialize_value(value, cx)
    }
}

impl<T: SerializableValue<MyLang> + Send + Sync + 'static + std::fmt::Debug> CallableSpec<MyLang> for Carrier<T> {}

/// A spec that writes a hand-built serialized form under the carrier's identifier, so
/// that the reader path deserializes exactly that form.
#[derive(Debug)]
struct RawCarrier(SerialValue);

impl SerializableObject<MyLang> for RawCarrier {
    fn serialize_object(&self, _cx: &mut SerializeContext<'_, MyLang>) -> Result<SerialEntry, SerializeError> {
        Ok(SerialEntry { identifier: CARRIER.into(), data: self.0.clone() })
    }
}

impl CallableSpec<MyLang> for RawCarrier {}

/// The stand-in object the reader holds once the value has been handed out.
#[derive(Debug)]
struct Placeholder;
impl SerializableObject<MyLang> for Placeholder {}
impl CallableSpec<MyLang> for Placeholder {}

/// What one write-then-read pass produced.
struct Pass<T> {
    /// The value's serialized form, as the writer produced it.
    written: SerialValue,
    /// The reader's result for the carrier's entry.
    read: Result<T, DeserializeError>,
}

/// Write the carrier `spec` (under the carrier identifier) with one session, read the
/// segment with another, and return what the writer wrote and what the reader made of
/// it.
fn pass<T>(spec: Arc<dyn CallableSpec<MyLang>>, written_by: impl Fn() -> Option<SerialValue>) -> Pass<T>
where
    T: DeserializableValue<MyLang> + Send + Sync + 'static + std::fmt::Debug,
{
    let mut writer = SerdeSession::<MyLang>::new();
    let position = writer.intern_spec(&spec).expect("the carrier writes");
    let written = written_by().expect("the carrier recorded what it wrote");
    let segment = writer.take_segment();

    let received: Arc<Mutex<Option<T>>> = Arc::new(Mutex::new(None));
    let mut reader = SerdeSession::<MyLang>::new();
    reader.set_user_data(KnownProviders::<MyLang>::new());
    register_core_readers(&mut reader).expect("core readers register");
    let specs = reader.standard_tables().expect("standard tables").specs;
    let sink = Arc::clone(&received);
    specs
        .register_type::<Carrier<T>>(&mut reader, CARRIER, move |value| {
            *sink.lock().unwrap() = Some(value);
            Arc::new(Placeholder) as Arc<dyn CallableSpec<MyLang>>
        })
        .expect("the carrier reader registers");

    let read = match reader.push_segment(segment) {
        Err(error) => Err(error),
        Ok(_) => reader.spec(specs.position(position.index())).map(|_| {
            received.lock().unwrap().take().expect("the reader handed the value out")
        }),
    };
    Pass { written, read }
}

/// Write `value` and read it back through the engine.
fn round_trip<T>(value: T) -> Pass<T>
where
    T: SerializableValue<MyLang> + DeserializableValue<MyLang> + Send + Sync + 'static + std::fmt::Debug,
{
    let carrier = Arc::new(Carrier::new(value));
    let handle = Arc::clone(&carrier);
    pass(carrier as Arc<dyn CallableSpec<MyLang>>, move || handle.written.lock().unwrap().clone())
}

/// Read the hand-built serialized form `value` back as a `T` through the engine.
fn read_back<T>(value: SerialValue) -> Result<T, DeserializeError>
where
    T: DeserializableValue<MyLang> + Send + Sync + 'static + std::fmt::Debug,
{
    let written = value.clone();
    pass::<T>(Arc::new(RawCarrier(value)) as Arc<dyn CallableSpec<MyLang>>, move || Some(written.clone())).read
}

/// The `SerialValueError` behind a failed read — the engine wraps a failing entry in
/// `InEntry`, naming the table and position.
fn value_error(result: Result<impl std::fmt::Debug, DeserializeError>) -> SerialValueError {
    fn unwrap(error: DeserializeError) -> SerialValueError {
        match error {
            DeserializeError::Value(error) => error,
            DeserializeError::InEntry { cause, .. } => unwrap(*cause),
            other => panic!("expected a value error, got {other:?}"),
        }
    }
    match result {
        Err(error) => unwrap(error),
        Ok(value) => panic!("expected a read failure, got {value:?}"),
    }
}

// --- the serialized form and the round trip ---------------------------------------------

/// The keys are the wire names in declaration order, an absent `Option` field is left
/// out, and the value reads back equal through a real writer and reader session.
#[test]
fn a_struct_round_trips_through_the_engine_with_its_documented_shape() {
    let pass = round_trip(sample());
    assert_eq!(keys_of(&pass.written), ["on", "tiny", "big", "n", "esc", "label", "list", "part", "raw", "shape"]);
    assert_eq!(pass.written, sample_value());
    assert_eq!(pass.read.expect("the record reads back"), sample());
}

/// Every variant kind: its documented form, and back.
#[test]
fn an_enum_round_trips_through_its_documented_shape() {
    let shapes = [
        (Shape::Dot, SerialValue::Str(s("dot"))),
        (
            Shape::Boxed(Part { number: 2, remark: None }),
            map([("boxed", map([("id", SerialValue::Int(2))]))]),
        ),
        (Shape::Rect { width: 3, height: None }, map([("rect", map([("w", SerialValue::Int(3))]))])),
        (
            Shape::Rect { width: 3, height: Some(4) },
            map([("rect", map([("w", SerialValue::Int(3)), ("h", SerialValue::Int(4))]))]),
        ),
    ];
    for (shape, form) in shapes {
        let pass = round_trip(shape.clone());
        assert_eq!(pass.written, form);
        assert_eq!(pass.read.expect("the shape reads back"), shape);
    }
}

/// A present `Option` field is written under its wire name; `None` reads back from an
/// omitted key and from an explicit `null` alike.
#[test]
fn an_absent_option_field_is_an_omitted_key_and_reads_back_from_null_too() {
    let with = round_trip(Part { number: 5, remark: Some(s("r")) });
    assert_eq!(with.written, map([("id", SerialValue::Int(5)), ("note", SerialValue::Str(s("r")))]));

    let without = round_trip(Part { number: 5, remark: None });
    assert_eq!(without.written, map([("id", SerialValue::Int(5))]));
    assert_eq!(without.read.unwrap(), Part { number: 5, remark: None });

    let from_null = read_back::<Part>(map([("id", SerialValue::Int(5)), ("note", SerialValue::Null)]));
    assert_eq!(from_null.unwrap(), Part { number: 5, remark: None });
}

// --- strict reads -------------------------------------------------------------------------

#[test]
fn an_unknown_key_is_refused_naming_the_declared_ones() {
    let error = value_error(read_back::<Part>(map([("id", SerialValue::Int(1)), ("extra", SerialValue::Null)])));
    assert_eq!(error, SerialValueError::UnknownField { name: s("extra"), expected: &["id", "note"] });
}

#[test]
fn a_repeated_key_is_refused() {
    let error = value_error(read_back::<Part>(map([("id", SerialValue::Int(1)), ("id", SerialValue::Int(2))])));
    assert_eq!(error, SerialValueError::DuplicateField { name: s("id") });
}

#[test]
fn a_missing_required_key_is_refused_naming_the_key() {
    let error = value_error(read_back::<Part>(map([("note", SerialValue::Str(s("x")))])));
    assert_eq!(error, SerialValueError::MissingField { name: "id" });
    // Inside a struct variant as well.
    let error = value_error(read_back::<Shape>(map([("rect", map([("h", SerialValue::Int(1))]))])));
    assert_eq!(error, SerialValueError::MissingField { name: "w" });
}

#[test]
fn an_unknown_variant_is_refused() {
    let expected: &'static [&'static str] = &["dot", "boxed", "rect"];
    let error = value_error(read_back::<Shape>(SerialValue::Str(s("blob"))));
    assert_eq!(error, SerialValueError::UnknownVariant { name: s("blob"), expected });
    let error = value_error(read_back::<Shape>(map([("blob", SerialValue::Null)])));
    assert_eq!(error, SerialValueError::UnknownVariant { name: s("blob"), expected });
}

#[test]
fn a_variant_with_the_wrong_payload_shape_is_refused() {
    // A unit variant carrying data.
    let error = value_error(read_back::<Shape>(map([("dot", SerialValue::Null)])));
    assert!(matches!(error, SerialValueError::TypeMismatch { found: "map", .. }), "{error:?}");
    // A data variant written as a bare string.
    let error = value_error(read_back::<Shape>(SerialValue::Str(s("boxed"))));
    assert!(matches!(error, SerialValueError::TypeMismatch { found: "str", .. }), "{error:?}");
}

#[test]
fn a_value_of_the_wrong_kind_is_refused_by_the_field_type() {
    let error = value_error(read_back::<Part>(map([("id", SerialValue::Str(s("one")))])));
    assert!(matches!(error, SerialValueError::TypeMismatch { found: "str", .. }), "{error:?}");
    // A `char` field given a two-character string.
    let error = value_error(read_back::<Escape>(map([("c", SerialValue::Str(s("ab")))])));
    assert!(matches!(error, SerialValueError::TypeMismatch { found: "str", .. }), "{error:?}");
}

/// A one-field struct around a `char`, for the two-character-string case.
#[derive(SerializableValue, DeserializableValue, Debug, PartialEq, Clone)]
struct Escape {
    #[serial(name = "c")]
    c: char,
}

// --- the serde bridge ---------------------------------------------------------------------

/// The same Rust shape declared as a serde type and converted through the bridge gives
/// the same serialized form as the derive.
#[cfg(feature = "serde")]
#[test]
fn the_shapes_agree_with_the_serde_bridge() {
    use serde::{Deserialize, Serialize};

    #[derive(Serialize, Deserialize)]
    #[serde(deny_unknown_fields)]
    struct BridgePart {
        id: u32,
        #[serde(skip_serializing_if = "Option::is_none", default)]
        note: Option<String>,
    }
    #[derive(Serialize, Deserialize)]
    enum BridgeShape {
        #[serde(rename = "dot")]
        Dot,
        #[serde(rename = "boxed")]
        Boxed(BridgePart),
        #[serde(rename = "rect")]
        Rect {
            w: u16,
            #[serde(skip_serializing_if = "Option::is_none", default)]
            h: Option<u16>,
        },
    }

    let via_bridge = techy::serialize::to_value(&vec![
        BridgeShape::Dot,
        BridgeShape::Boxed(BridgePart { id: 2, note: None }),
        BridgeShape::Rect { w: 3, h: None },
        BridgeShape::Rect { w: 3, h: Some(4) },
    ])
    .unwrap();
    let via_derive = round_trip(vec![
        Shape::Dot,
        Shape::Boxed(Part { number: 2, remark: None }),
        Shape::Rect { width: 3, height: None },
        Shape::Rect { width: 3, height: Some(4) },
    ])
    .written;
    assert_eq!(via_bridge, via_derive);
}
