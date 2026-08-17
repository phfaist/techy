# AI guide: serialization

Condensed reference for writing code that uses techy's serialization
([`techy::serialize`](crate::serialize)) from a third-party project: the exact
API sequences for writing and reading, the registration obligations, the
extension traits to import, the errors, the invariants, and how a language or
framework of your own opts in. Every rule here is documented in full on the
linked item; the [`serialize`](crate::serialize) module page defines the
vocabulary and links every public item; the narrative introduction is
[Serializing parses](crate::guide::serialize).

**Terms.** A *session* ([`SerdeSession`](crate::serialize::SerdeSession)) holds
one *table* per kind of object; an *object* (source, parsing state, callable
spec, provider, node tree, diagnostic, parse result) is written into its table
once (*interned*, by `Arc` identity) and referred to by its *position*; a
*value* (a mode, an ext, a span) is embedded inline. A *segment*
([`Segment`](crate::serialize::Segment)) is what one emission contains: per
table, the entries new since the previous emission; the segments of one session
form a *stream*, positions are numbered across the stream. Every entry carries
an *identifier* string naming its kind (`core.state`, `core.tree`,
`latexlike.begin`), never a Rust type name. The *reading environment* is the
set of live objects (packages) the reader already holds; specs and packages
serialize *by identity* (a reference resolved there) or in a *self-contained*
form. Everything is unconditional plain Rust (`no_std` + `alloc`); the optional
cargo feature `serde` adds only rendering (`Serialize`/`Deserialize` impls for
`Segment` and [`SerialValue`](crate::serialize::SerialValue), the bridge
`to_value`/`from_value`).

## Write path

```rust
use std::sync::Arc;
use techy::core::{Language, ParsingState};
use techy::error::Recovery;
use techy::latexlike::minidefs::minilatex_package;
use techy::latexlike::{Latexlike, LatexlikeDriver};
use techy::serialize::{ParseResultSerialization, SerdeSession, Segment};

let language: Language<Latexlike> = Language::new(
    LatexlikeDriver::new(Recovery::Tolerant),
    ParsingState::lang_initial_with_packages([minilatex_package()])?,
);
let result = Arc::new(language.parse(r"a \emph{b}")?);

let mut writer = SerdeSession::<Latexlike>::new();      // the 7 standard tables
writer.set_profile("myapp 1.0 / techy");               // optional; see "Profile"
let position = writer.serialize_parse_result(&result)?; // interns everything reachable
let segment: Segment = writer.take_segment_with_main(position)?;
// or: writer.serialize_tree(&tree)? (TreeSerialization), writer.take_segment()
# assert_eq!(segment.tables().len(), 7);
# Ok::<(), Box<dyn std::error::Error>>(())
```

Extension traits to import for the by-kind convenience methods:
[`ParseResultSerialization`](crate::serialize::ParseResultSerialization)
(`serialize_parse_result` / `parse_result`),
[`TreeSerialization`](crate::serialize::TreeSerialization) (`serialize_tree` /
`tree::<A>`), [`DiagnosticSerialization`](crate::serialize::DiagnosticSerialization)
(`serialize_diagnostic` / `diagnostic`),
[`StandardTableInterning`](crate::serialize::StandardTableInterning)
(`intern_source`, `intern_state`, `intern_spec`, `intern_provider`),
[`StandardTableReading`](crate::serialize::StandardTableReading) (`source`,
`state`, `spec`, `provider`). Sources and states are interned (written once,
shared); trees and diagnostics are values (a new entry per call); a parse
result is interned by its `Arc`.

Rendering (feature `serde`): `serde_json::to_string(&segment)` — one segment
per line is the canonical stream form (JSON Lines); any serde format works
(`postcard::to_allocvec(&segment)`); no format crate is a techy dependency.

## Read path

```rust
# use std::sync::Arc;
# use techy::core::{Language, ParsingState};
# use techy::error::Recovery;
# use techy::latexlike::{Latexlike, LatexlikeDriver};
# use techy::serialize::{ParseResultSerialization, SerdeSession};
use techy::latexlike::minidefs::{self, minilatex_package};
use techy::latexlike::serialize::{register, register_package_recipes};
use techy::serialize::KnownProviders;
# let language: Language<Latexlike> = Language::new(
#     LatexlikeDriver::new(Recovery::Tolerant),
#     ParsingState::lang_initial_with_packages([minilatex_package()])?,
# );
# let result = Arc::new(language.parse(r"a \emph{b}")?);
# let mut writer = SerdeSession::<Latexlike>::new();
# writer.set_profile("myapp 1.0 / techy");
# let position = writer.serialize_parse_result(&result)?;
# let segment = writer.take_segment_with_main(position)?;

// 1. Reading environment: the packages by identity (+ recipes for the ones not held).
let mut known = KnownProviders::<Latexlike>::new();
for provider in language.initial_state().scopes().providers() {
    known.insert(Arc::clone(provider));                 // the SAME instances the parse used
}
register_package_recipes(&mut known);                   // `_builtin` recipe
minidefs::register_package_recipes(&mut known);         // `minilatex`, `minilatex.item`

// 2. Session: same profile, user data = environment, readers registered once.
let mut reader = SerdeSession::<Latexlike>::new();
reader.set_profile("myapp 1.0 / techy");
reader.set_user_data(known);
register(&mut reader)?;                                 // core + preset readers

// 3. Absorb the stream's segments in order; read by position.
let main = reader.push_segment(segment)?;               // Option<(TableId, u32)>, reader numbering
let (table, index) = main.expect("segment named a main entry");
let parse_results = reader.standard_tables().expect("standard tables").parse_results;
assert_eq!(table, parse_results.id());
let back = reader.parse_result(parse_results.position(index))?;
assert_eq!(back.tree.root().child(1).unwrap().macro_name(), Some("emph"));
# Ok::<(), Box<dyn std::error::Error>>(())
```

Read-then-append: after absorbing, `reader.serialize_parse_result(&next)?` and
`reader.take_segment_with_main(pos)?` emit the stream's next segment (held
objects are referred to, not rewritten). A stream from JSON Lines:
`for line in text.lines() { reader.push_segment(serde_json::from_str::<Segment>(line)?)?; }`.

## Registration obligations (reading only; writing needs none)

| Obligation | How | Failure if missing |
|---|---|---|
| Readers of the specs/providers tables | [`latexlike::serialize::register(&mut session)`](crate::latexlike::serialize::register) once (calls [`register_core_readers`](crate::serialize::register_core_readers)); a framework's own helper for its identifiers | [`DeserializeError::UnknownIdentifier`](crate::serialize::DeserializeError::UnknownIdentifier) |
| Reading environment for packages | `session.set_user_data(KnownProviders)` — [`insert`](crate::serialize::KnownProviders::insert) held packages (identity), [`register_recipe`](crate::serialize::KnownProviders::register_recipe) builders for others ([`ProviderRecipe`](crate::serialize::ProviderRecipe)); the preset's [`register_package_recipes`](crate::latexlike::serialize::register_package_recipes) and [`minidefs::register_package_recipes`](crate::latexlike::minidefs::register_package_recipes) | [`DeserializeError::MissingProvider`](crate::serialize::DeserializeError::MissingProvider) / [`MissingDefinition`](crate::serialize::DeserializeError::MissingDefinition) |
| Non-unit tree annotation type `A` | [`trees.register_annotation::<A>(&mut session, "myapp.annot")`](crate::serialize::TableHandle::register_annotation) on BOTH sides (`A: SerializableValue + DeserializableValue`); plain-data `A` under feature `serde`: `register_serde_annotation` | `SerializeError` on write; `UnknownIdentifier` on read |
| Referenced (non-embedded) source text | writer: [`SourceSerdeDriver::with_text_policy`](crate::serialize::SourceSerdeDriver::with_text_policy) ([`SourceTextPolicy`](crate::serialize::SourceTextPolicy)); reader: [`with_text_supplier`](crate::serialize::SourceSerdeDriver::with_text_supplier) ([`SourceTextSupplier`](crate::serialize::SourceTextSupplier), verifies [`SourceDigest`](crate::serialize::SourceDigest)); install with [`SerdeSession::with_source_driver`](crate::serialize::SerdeSession::with_source_driver) | [`NoSourceTextSupplier`](crate::serialize::DeserializeError::NoSourceTextSupplier), [`SourceLengthMismatch`](crate::serialize::DeserializeError::SourceLengthMismatch), [`SourceDigestMismatch`](crate::serialize::DeserializeError::SourceDigestMismatch) |
| Profile | [`set_profile`](crate::serialize::SerdeSession::set_profile) on writer and reader with the same string (names the configuration that reads the stream fully); a reader without one accepts any | [`ProfileMismatch`](crate::serialize::DeserializeError::ProfileMismatch) |
| Custom tables | `session.register_table(MyDriver)` ([`ObjectSerdeDriver`](crate::serialize::ObjectSerdeDriver), position type via [`serial_index!`](crate::serialize::serial_index)) in the SAME set on both sides (order may differ; matched by name) | [`UnknownTableName`](crate::serialize::DeserializeError::UnknownTableName) |

## Errors

Write: [`SerializeError`](crate::serialize::SerializeError) — `Unsupported`
(the type has the empty `SerializableObject` impl), `MissingProvenance` (a
parser-holding spec built outside `Package::new_shared`), `ProviderDropped`
(stamp's package no longer alive), `ArgumentSpecOutOfBand` (an argument's spec
is not one of the callable's declared ones and the spec type has no override),
`ReferenceCycle`, `Value(NestingTooDeep)`, all wrapped as `InTable {table,
cause}` / `InNode {index, cause}` naming where. Read:
[`DeserializeError`](crate::serialize::DeserializeError) — `UnsupportedVersion`,
`ProfileMismatch`, `SegmentOutOfOrder` (skipped/repeated segment or wrong
stream), `UnemittedEntries` (push while entries pending emission),
`UnknownIdentifier`, `MissingProvider`/`MissingDefinition` (environment lacks
it), `IndexOutOfRange`/`WrongTable` (bad reference), `SpanOutOfBounds`,
`FeatureAbsent` (state uses a token feature the reading language lacks),
`Value(_)` (shape), wrapped in `InEntry {table, index, cause}` / `InNode`. A
failed `push_segment` leaves the session unchanged. Setup:
[`RegistrationError`](crate::serialize::RegistrationError) (`DuplicateIdentifier`
= `register` called twice). Bridge and shape:
[`SerialValueError`](crate::serialize::SerialValueError) (`FloatRejected`,
`ReservedMapKey`, `IntegerOutOfRange`, `TypeMismatch`, …). Nothing panics on
wire input.

## Invariants

- **Positions are session-scoped.** A typed position (`TreeIndex`,
  `SpecIndex`, … — [`SerialIndex`](crate::serialize::SerialIndex)) carries its
  session's [`TableId`](crate::serialize::TableId); never pass one to another
  session — exchange `(table name, u32)` and rebuild with
  [`TableHandle::position`](crate::serialize::TableHandle::position);
  `push_segment` returns the main entry already translated.
- **One stream per session, in order, each segment once**; the session checks
  contiguity (`start == table length`) but cannot detect a foreign stream whose
  positions line up. **Absorb all, then append**: no `push_segment` while
  entries are pending emission.
- **Sharing follows `Arc` identity, not equality**: a fresh but equal state or
  source is a new entry; a package inserted in `KnownProviders` is the instance
  read data resolves to — insert the very instances your parses use.
- **No `$`-prefixed map keys** anywhere (reserved for `$bytes`/`$index`;
  `SerialValueError::ReservedMapKey`); maps are ordered (order-sensitive
  equality); no floats; integers are `i64`.
- **Nesting bound** [`SerialValue::MAX_NESTING_DEPTH`](crate::serialize::SerialValue::MAX_NESTING_DEPTH)
  = 64, segment wrapping counted (an entry may nest ≤ 60).
- **Owned text in language payloads**: invocation syntax, ext values, and
  annotations must not carry spans relative to a node's source
  ([`TextContent`](crate::source::TextContent) values are owned on the wire; a
  [`SourceSpan`](crate::source::SourceSpan), which names its source, is fine).
- **What does not survive**: `NodeId`s and tree tags (fresh on read; keep
  durable node identity in annotations), the concrete condition types of
  diagnostics ([`DeserializedCondition`](crate::serialize::DeserializedCondition)
  instead — match on `identifier()`).
- **Vocabulary not yet frozen**: `Segment::VERSION` = 1; layout may still change
  incompatibly before it is declared final.

## Opting in a language or framework of your own

Owed: (a) an empty `impl SerializableLang for MyLang {}` — its bounds require
[`SerializableValue`](crate::serialize::SerializableValue) +
[`DeserializableValue`](crate::serialize::DeserializableValue) on every type the
language supplies (`ModeId`, `CallableTypeId`, `GroupTypeId`, `Event`,
`StateExt`, `SessionExt`, `SourceOrigin`, node/argument/slot exts,
`InvocationSyntax`); the crate implements both for `()`, `bool`, integers,
`String`, `Option<T>`, `Vec<T>`, `SourceSpan` (a language on the defaults, like
[`TrivialLang`](crate::core::TrivialLang), needs nothing more); (b)
[`SerializableObject`](crate::serialize::SerializableObject) impls for your
spec/provider types (`impl<L> SerializableObject<L> for MySpec {}` for
non-participants; a parser-holding spec delegates to its provenance stamp); (c)
[`DeserializableObject`](crate::serialize::DeserializableObject) impls for
self-contained forms; (d) a `register(&mut session)` helper that calls
`register_core_readers` and registers your identifiers
([`TableHandle::register_type`](crate::serialize::TableHandle::register_type) on
`standard_tables().specs` / `.providers`; an
[`IdentifierResolver`](crate::serialize::IdentifierResolver) for an open set);
(e) packages built with [`Package::new_shared`](crate::core::specs::Package::new_shared)
so specs get their [`SpecProvenance`](crate::core::specs::SpecProvenance) stamp
([`provenance_for`](crate::core::specs::Package::provenance_for), a
`with_provenance`-style setter on your spec types); (f) under feature `serde`,
`#[derive(Serialize, Deserialize)]` + explicit `#[serde(rename = …)]` on your
vocabulary types (for the bridge; `#[serde(skip_serializing_if = "Option::is_none")]`
+ `#[serde(default)]` on `Option` fields so an absent value is an omitted key).
Wire identifiers you mint (`myfw.<kind>`) are yours to keep stable.

```rust
use std::sync::Arc;
use techy::core::specs::{CallableSpec, Package, SpecProvenance};
use techy::core::TrivialLang;
use techy::serialize::{
    register_core_readers, DeserializableObject, DeserializableValue, DeserializeContext,
    DeserializeError, KnownProviders, RegistrationError, SerdeSession, SerialEntry, SerialIndex,
    SerialValue, SerializableLang, SerializableObject, SerializableValue, SerializeContext,
    SerializeError, StandardTableInterning, StandardTableReading,
};

#[derive(Debug, Clone, Copy)]
struct MyLang;
impl TrivialLang for MyLang {}
impl SerializableLang for MyLang {}                       // (a): the defaults suffice

// A value type of your own (an ext, say): its two conversions, for every language.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Flavor { Plain, Fancy }
impl<L: techy::core::Lang> SerializableValue<L> for Flavor {
    fn serialize_value(&self, _cx: &mut SerializeContext<'_, L>) -> Result<SerialValue, SerializeError>
    where L: SerializableLang {
        Ok(SerialValue::Str(match self { Flavor::Plain => "plain", Flavor::Fancy => "fancy" }.into()))
    }
}
impl<L: techy::core::Lang> DeserializableValue<L> for Flavor {
    fn deserialize_value(value: &SerialValue, _cx: &mut DeserializeContext<'_, L>) -> Result<Self, DeserializeError>
    where L: SerializableLang {
        match value {
            SerialValue::Str(s) if s == "plain" => Ok(Flavor::Plain),
            SerialValue::Str(s) if s == "fancy" => Ok(Flavor::Fancy),
            _ => Err(DeserializeError::failed("a flavor is \"plain\" or \"fancy\"")),
        }
    }
}

// (b)+(e): a parser-holding spec — identity through its stamp; unstamped = write error.
#[derive(Debug)]
struct MySpec { provenance: Option<SpecProvenance<MyLang>> }
impl SerializableObject<MyLang> for MySpec {
    fn serialize_object(&self, cx: &mut SerializeContext<'_, MyLang>) -> Result<SerialEntry, SerializeError> {
        match &self.provenance {
            Some(stamp) => stamp.serialize_object(cx),    // `core.provider-spec-identity`
            None => Err(SerializeError::MissingProvenance { spec: "MySpec" }),
        }
    }
}
impl CallableSpec<MyLang> for MySpec {}

// (b)+(c): a self-contained spec under an identifier of your own.
#[derive(Debug)]
struct MyBreak;
const MY_BREAK: &str = "myfw.break";
impl SerializableObject<MyLang> for MyBreak {
    fn serialize_object(&self, _cx: &mut SerializeContext<'_, MyLang>) -> Result<SerialEntry, SerializeError> {
        Ok(SerialEntry { identifier: MY_BREAK.into(), data: SerialValue::Map(Vec::new()) })
    }
}
impl DeserializableObject<MyLang> for MyBreak {
    type Output = MyBreak;
    fn deserialize_object(_value: &SerialValue, _cx: &mut DeserializeContext<'_, MyLang>) -> Result<MyBreak, DeserializeError> {
        Ok(MyBreak)
    }
}
impl CallableSpec<MyLang> for MyBreak {}

// (d): the helper a reading session calls once.
fn register(session: &mut SerdeSession<MyLang>) -> Result<(), RegistrationError> {
    register_core_readers(session)?;
    let specs = session.standard_tables().expect("standard tables").specs;
    specs.register_type::<MyBreak>(session, MY_BREAK, |spec| Arc::new(spec) as Arc<dyn CallableSpec<MyLang>>)
}

// (e): the package stamps its specs.
const MACRO: u32 = 0;
let defs = Package::<MyLang>::new_shared("mydefs", |package| {
    let stamp = package.provenance_for(MACRO, "emph").expect("a shared package stamps");
    package.insert(MACRO, "emph", MySpec { provenance: Some(stamp) });
});
let emph = Arc::clone(defs.get(MACRO, "emph").unwrap());

// Round trip: identity for `emph`, a rebuilt value for the self-contained spec.
let mut writer = SerdeSession::<MyLang>::new();
let emph_pos = writer.intern_spec(&emph)?;
let break_pos = writer.intern_spec(&(Arc::new(MyBreak) as Arc<dyn CallableSpec<MyLang>>))?;
let segment = writer.take_segment();

let mut known = KnownProviders::<MyLang>::new();
known.insert(Arc::clone(&defs));
let mut reader = SerdeSession::<MyLang>::new();
reader.set_user_data(known);
register(&mut reader)?;
reader.push_segment(segment)?;
let specs = reader.standard_tables().unwrap().specs;
assert!(Arc::ptr_eq(&reader.spec(specs.position(emph_pos.index()))?, &emph));   // same instance
assert!(reader.spec(specs.position(break_pos.index())).is_ok());
# Ok::<(), Box<dyn std::error::Error>>(())
```

(The reader registered its tables in the writer's order, so the writer's `u32`
indices are valid positions there once rebuilt with `position`.)

## Checklist

1. Writer: `SerdeSession::<L>::new()` (+ `set_profile`, + `with_source_driver`
   for referenced sources); `serialize_parse_result` / `serialize_tree`;
   `take_segment[_with_main]`; render per segment (JSON Lines).
2. Reader: same profile; `KnownProviders` with the SAME package instances (+
   recipes) as user data; `register` (preset or your own) exactly once;
   `push_segment` in stream order; read via `parse_result` / `tree` /
   `standard_tables()` positions; translate `main` with `handle.position`.
3. Own types: value traits on every lang type; `SerializableObject` on every
   spec/provider (empty impl to opt out); `DeserializableObject` + a `register`
   helper for self-contained forms; `Package::new_shared` for stamped specs;
   annotation types registered on both sides.
4. Never: `$` map keys, floats, spans in ext/annotation values, positions
   across sessions, pushing while entries are pending, mixing streams.
