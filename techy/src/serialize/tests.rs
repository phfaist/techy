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
/// exists to pin the vacant-vtable behavior of the capability methods (D17). Every
/// spec/provider trait object of this lang must still be built and usable for the
/// non-serialization methods although `serialize_object` and the argument-spec pair
/// can neither be called nor reached (no context value exists for this lang). Do not
/// add a `SerializableLang` impl for it, ever: that would defeat the test.
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

// --- D17: the vacant vtable ---------------------------------------------------------------

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

// --- D21: the argument-spec index rule ----------------------------------------------------

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
