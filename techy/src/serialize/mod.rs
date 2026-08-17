//! Serialization: converting the objects techy consumers handle — parsed node trees,
//! parsing states, sources, callable specs and providers, diagnostics, and whole parse
//! results — to and from a format-independent, in-memory value model, so that they
//! can be stored (a cache of parses, a golden file of a test suite), transmitted (to
//! another process or program), inspected, and rebuilt on the other side with their
//! sharing and identity intact.
//!
//! This page is the reference entry point of the module (the introductory guide
//! chapter is [Serializing parses](crate::guide::serialize)). It defines the
//! vocabulary, walks the write path and the read path with one example each, and
//! describes the streams, the capability traits, the engine, the standard tables, the
//! optional `serde` rendering, the errors, and the stability of the serialized form;
//! the index at the end links every public item. The preset's own support — the impls that make
//! [`Latexlike`](crate::latexlike::Latexlike) serializable and the helper that prepares
//! a reading session for latexlike data — is [`latexlike::serialize`](crate::latexlike::serialize).
//!
//! # Vocabulary
//!
//! The terms below are used with exactly these meanings throughout the module and its
//! items.
//!
//! - **Serialization** converts a live object into a [`SerialValue`] — a plain tree of
//!   values (null, booleans, integers, strings, byte strings, lists, string-keyed maps,
//!   and table references) that depends on no encoding format; **deserialization** is
//!   the reverse. The module uses only this verb pair.
//! - The **serialized form** of an object is the `SerialValue` that describes it — the
//!   data as it is written out and read back, as opposed to the live object. The
//!   crate calls that form the **wire** for short (a "wire name" is a key name or an
//!   enum string of the serialized form; the "wire structures" are the crate's own
//!   layouts of that form).
//! - An **object** is a thing kept in a *table* and referred to, from wherever it is
//!   used, by its position in that table: sources, parsing states, callable specs,
//!   providers, node trees, diagnostics, and parse results are objects. A **value** is
//!   data embedded inline in an object's entry and converted in place — a state's mode,
//!   a source's origin, a span, a language's ext values — never kept in a table of its
//!   own.
//! - A **table** holds the objects of one kind. A [`SerdeSession`] keeps a set of
//!   tables, numbered in the order they were registered ([`TableId`]); a **position**
//!   is a `u32` index into a table. A **table handle** ([`TableHandle`]) is a session's
//!   typed handle on one of its tables; a **typed position** (a type defined with
//!   [`serial_index!`] and satisfying [`SerialIndex`]) is a position together with its
//!   table's id, as Rust code holds it — [`SourceIndex`], [`StateIndex`], and so on.
//! - A table is **homogeneous** when it holds objects of one kind only (every entry
//!   carries the same identifier, which the table then does not write out — sources,
//!   states, diagnostics, parse results) and **heterogeneous** when it holds trait
//!   objects of several concrete types, each entry carrying its own identifier
//!   (specs, providers, and — by annotation type — trees).
//! - Every serialized object carries an **identifier**: a deliberately chosen, stable
//!   string naming what kind of object the value describes (`core.state`,
//!   `latexlike.begin`) — never a Rust type name. A serialization call returns the
//!   identifier together with the data as a [`SerialEntry`].
//! - **Interning** an object writes it into its table once and returns its position;
//!   interning the same object again (the same `Arc`, by pointer identity) returns the
//!   existing position without writing it again. This is what makes sharing survive: a
//!   parsing state referenced by many nodes, or a source referenced by many spans, is
//!   written once and read back as one shared object.
//! - A **driver** ([`ObjectSerdeDriver`]) is what a table is registered with: how the
//!   objects of that table are serialized into entries and rebuilt from them.
//! - A **segment** ([`Segment`]) is the unit a session emits and absorbs: for every
//!   table, the entries new since the previous emission, together with the position
//!   they start at, plus a **table directory** — every table of the emitting session
//!   by name and writer-side id — a version, and a **main entry** the segment may name
//!   (the one entry the segment is about, say the parse result of that segment). A
//!   **stream** is the sequence of segments one session emits. Positions are scoped
//!   to the stream: later segments refer to earlier ones' entries by position, so a
//!   reading session absorbs the segments of one stream only, in order.
//! - A **profile** is a caller-chosen string naming the configuration that reads a
//!   stream fully (the packages, spec types, and readers that resolve everything in
//!   it); a session that declares one writes it into every segment and refuses
//!   segments carrying a different one ([`SerdeSession::set_profile`]).
//! - The **reading environment** is the set of live objects — providers above all —
//!   that the deserializing program already holds and that serialized data refers to
//!   by identity rather than describes in full; it is handed to a reading session as
//!   its *user data* ([`SerdeSession::set_user_data`]). For the crate's own
//!   providers the reading environment is a [`KnownProviders`] directory: the
//!   providers the program holds, by name, plus **recipes** ([`ProviderRecipe`]) that
//!   build the ones it does not hold. A reference to something the reading environment
//!   lacks is a deserialization error.
//! - A spec or provider is serialized either by **identity** — a reference the reading
//!   side resolves against its reading environment — or in a **self-contained** form
//!   the reading side rebuilds an equivalent object from. Identity of a spec goes
//!   through its **provenance stamp** ([`SpecProvenance`](crate::core::specs::SpecProvenance)):
//!   a record of the provider that defined it and the key it was defined under, which
//!   a package built with [`Package::new_shared`](crate::core::specs::Package::new_shared)
//!   hands out.
//! - Everything read is **untrusted input**: a malformed segment, a reference out of
//!   range or into the wrong table, a reference cycle, an unknown identifier, or a
//!   value nesting deeper than the bound is an error naming what failed — never a
//!   panic — and a failed absorption leaves the session as it was.
//!
//! # Writing
//!
//! A [`SerdeSession`] with the standard tables ([`SerdeSession::new`]) interns
//! whatever it is given — a parse result here, through the
//! [`ParseResultSerialization`] extension trait — and emits the new entries as a
//! segment. Writing needs no registration and no reading environment: every object
//! serializes itself. Nothing here needs a cargo feature.
//!
//! ```
//! use std::sync::Arc;
//! use techy::core::{Language, ParsingState};
//! use techy::error::Recovery;
//! use techy::latexlike::minidefs::minilatex_package;
//! use techy::latexlike::{Latexlike, LatexlikeDriver};
//! use techy::serialize::{ParseResultSerialization, SerdeSession, Segment};
//!
//! let language: Language<Latexlike> = Language::new(
//!     LatexlikeDriver::new(Recovery::Tolerant),
//!     ParsingState::lang_initial_with_packages([minilatex_package()])?,
//! );
//! let result = Arc::new(language.parse(r"Hello \emph{world}")?);
//!
//! let mut writer = SerdeSession::<Latexlike>::new();
//! let position = writer.serialize_parse_result(&result)?;   // interns tree, states, sources, specs, …
//! let segment: Segment = writer.take_segment_with_main(position)?;
//! assert_eq!(segment.version(), Segment::VERSION);
//! assert_eq!(segment.tables().len(), 7);                    // the standard tables' directory
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```
//!
//! With the `serde` cargo feature the segment encodes through any serde format; in
//! JSON, one segment per line is the canonical stream rendering:
//!
//! ```
//! # #[cfg(feature = "serde")]
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! # use techy::core::{Language, ParsingState};
//! # use techy::error::Recovery;
//! # use techy::latexlike::{Latexlike, LatexlikeDriver};
//! # use techy::serialize::{SerdeSession, TreeSerialization};
//! # let language: Language<Latexlike> = Language::new(
//! #     LatexlikeDriver::new(Recovery::Tolerant), ParsingState::lang_initial()?);
//! # let result = language.parse("Hello {world}")?;
//! # let mut writer = SerdeSession::<Latexlike>::new();
//! # writer.serialize_tree(&result.tree)?;
//! # let segment = writer.take_segment();
//! let line = serde_json::to_string(&segment)?;             // one segment, one line
//! assert!(line.starts_with(r#"{"version":1,"meta":{},"tables":["#));
//! let back: techy::serialize::Segment = serde_json::from_str(&line)?;
//! assert_eq!(back, segment);
//! # Ok(()) }
//! # #[cfg(not(feature = "serde"))]
//! # fn main() {}
//! ```
//!
//! # Reading
//!
//! A reading session needs three things a writing session does not: the **readers**
//! of the heterogeneous tables (which concrete type rebuilds an entry of a given
//! identifier — registered once per session; the preset's
//! [`latexlike::serialize::register`](crate::latexlike::serialize::register) registers
//! the crate's own and the preset's, calling [`register_core_readers`]), a **reading
//! environment** for the objects serialized by identity (a [`KnownProviders`] holding
//! the packages the parse used — the very instances, so that read data shares them —
//! and recipes for the ones it does not hold), and the **segments of one stream, in
//! order**. [`SerdeSession::push_segment`] validates a segment, appends its entries,
//! rebuilds every object, and hands back the segment's main entry translated into the
//! reading session's numbering; the objects are then read by position.
//!
//! ```
//! # use std::sync::Arc;
//! # use techy::core::{Language, ParsingState};
//! # use techy::error::Recovery;
//! # use techy::latexlike::minidefs::{self, minilatex_package};
//! # use techy::latexlike::{Latexlike, LatexlikeDriver};
//! # use techy::serialize::{ParseResultSerialization, SerdeSession};
//! # let language: Language<Latexlike> = Language::new(
//! #     LatexlikeDriver::new(Recovery::Tolerant),
//! #     ParsingState::lang_initial_with_packages([minilatex_package()])?,
//! # );
//! # let result = Arc::new(language.parse(r"Hello \emph{world}")?);
//! # let mut writer = SerdeSession::<Latexlike>::new();
//! # let position = writer.serialize_parse_result(&result)?;
//! # let segment = writer.take_segment_with_main(position)?;
//! use techy::latexlike::serialize::{register, register_package_recipes};
//! use techy::serialize::KnownProviders;
//!
//! // The reading environment: the packages the parse used, held by identity, plus
//! // recipes for the preset's packages a program might not hold (`_builtin`, the
//! // minilatex packages).
//! let mut known = KnownProviders::<Latexlike>::new();
//! for provider in language.initial_state().scopes().providers() {
//!     known.insert(Arc::clone(provider));
//! }
//! register_package_recipes(&mut known);
//! minidefs::register_package_recipes(&mut known);
//!
//! let mut reader = SerdeSession::<Latexlike>::new();
//! reader.set_user_data(known);
//! register(&mut reader)?;                                   // the readers, once
//!
//! // Absorb the segment (from the writing example) and read its main entry back.
//! let (table, index) = reader.push_segment(segment)?.expect("the segment names its main entry");
//! let parse_results = reader.standard_tables().expect("standard tables").parse_results;
//! assert_eq!(table, parse_results.id());
//! let back = reader.parse_result(parse_results.position(index))?;
//! assert_eq!(back.tree.root().child_count(), 2);
//! assert_eq!(back.tree.root().child(1).unwrap().macro_name(), Some("emph"));
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```
//!
//! What survives a round trip: the tree's structure, every span (into the same
//! sources, shared as before), each node's parsing state (shared as before), the
//! callable specs by identity (the `\emph` node's spec is the instance the reading
//! side's `minilatex` package holds), and the diagnostics' identifiers, projections,
//! messages, and spans. What does not: node ids and tree tags (a rebuilt tree is a
//! new tree, with fresh ones — durable node identity travels in annotations), and the
//! concrete condition types of diagnostics (a diagnostic read back carries a
//! [`DeserializedCondition`]; consumers match on the identifier).
//!
//! # Streams
//!
//! A session that has absorbed a stream may intern further objects and emit segments
//! of its own, which continue the same stream — reading then appending. The objects
//! its reading environment holds by identity (the providers the absorbed entries
//! resolved to) are then written once for the whole stream, while a live object the
//! program creates anew (the parsing states of a fresh parse) is a new entry even when
//! an equal object was absorbed earlier: sharing follows object identity, not
//! equality. Two rules bind: the segments pushed into one session must all come from
//! one stream, in order (the session checks that each segment continues its tables,
//! but cannot recognize a foreign stream whose positions happen to line up), and a
//! session absorbs before it appends — pushing a segment while entries are pending
//! emission is an error ([`DeserializeError::UnemittedEntries`]).
//!
//! **JSON Lines.** With the `serde` feature the canonical stream rendering is one
//! segment per line: each segment encoded with `serde_json::to_string(&segment)` (its
//! rendering contains no raw line break) and appended to the stream; a reader decodes
//! each line with `serde_json::from_str::<Segment>(line)` and pushes the segments in
//! order. Every line is an independently valid segment (each carries the version and
//! the full table directory), so a stream can be appended to by appending lines,
//! split into per-file or per-message pieces, or truncated with only its last,
//! incomplete line lost; there is no end-of-stream marker — the stream ends where the
//! input ends. The same conventions hold for any other serde format that frames its
//! values (one segment per framed value, in order); the crate itself calls no
//! encoder — the engine emits and absorbs `Segment` values only. Two segment-level
//! conveniences serve streams: the **main entry** ([`SerdeSession::take_segment_with_main`];
//! `push_segment` returns it in the reader's numbering, so a reader finds each
//! segment's payload without knowing the tables' layout) and the **profile**
//! ([`SerdeSession::set_profile`], carried in every [`SegmentMeta`]), so that a stream
//! written for one configuration is refused up front by a reader configured for
//! another instead of failing on some unresolvable entry.
//!
//! # The capability traits
//!
//! Two pairs of traits express the capability. For objects: [`SerializableObject`] —
//! the write side, which every [`CallableSpec`](crate::core::specs::CallableSpec) and
//! [`SpecsProvider`](crate::core::specs::SpecsProvider) carries as a supertrait, so
//! that the method is callable through their trait objects; it is defaulted to
//! "unsupported", so a type that does not participate writes a one-line empty impl —
//! and [`DeserializableObject`], the opt-in read side implemented by concrete types
//! only (its [`Output`](DeserializableObject::Output) is the type itself for a
//! self-contained form, or an `Arc<dyn …>` for an object resolved in the reading
//! environment). For values: [`SerializableValue`] and [`DeserializableValue`],
//! implemented by the owner of each value type — the crate covers `()`, `bool`, the
//! integers, `String`, `Option<T>`, `Vec<T>`, and spans; a language covers its own
//! vocabulary and ext types. All four are usable only for a language that declares
//! itself serializable by implementing [`SerializableLang`] — a trait with no items,
//! whose bounds require the two value traits of every type the language supplies to
//! the parse — since the methods receive a [`SerializeContext`] or a
//! [`DeserializeContext`], which exist only for such languages. A callable spec may
//! also take part in the serialization of the arguments parsed with it, through the
//! defaulted pair [`CallableSpec::serialize_argument_spec`](crate::core::specs::CallableSpec::serialize_argument_spec)
//! and [`deserialize_argument_spec`](crate::core::specs::CallableSpec::deserialize_argument_spec)
//! (the default: an argument's spec is one of the callable's declared ones, and only
//! its index is written).
//!
//! # The engine
//!
//! A [`SerdeSession`] holds the tables, each registered with its driver
//! ([`SerdeSession::register_table`]) and addressed through its [`TableHandle`];
//! interns objects ([`SerdeSession::intern`], and [`SerializeContext::intern`] from
//! inside a serialization call) and reads objects back from positions
//! ([`SerdeSession::object`], [`DeserializeContext::object`]); emits and absorbs
//! segments ([`SerdeSession::take_segment`], [`SerdeSession::take_segment_with_main`],
//! [`SerdeSession::push_segment`]); carries the caller's user data
//! ([`SerdeSession::set_user_data`], one value per type); and bounds nested calls with
//! the crate's descent guard ([`SerdeSession::with_descent_guard_init`]). Both
//! directions share one session type: [`SerdeSession::new`] registers the standard
//! tables, [`SerdeSession::empty`] starts with none, for a session composed of other
//! tables. A heterogeneous table uses the [`DispatchingSerdeDriver`]: writing calls
//! each object's own `serialize_object`; reading dispatches on the entry's identifier
//! through the [`ObjectReader`]s registered on the table's handle
//! ([`TableHandle::register_type`], [`TableHandle::register_reader`]) and, for
//! identifiers no reader covers, the [`IdentifierResolver`]s registered for
//! identifier prefixes ([`TableHandle::register_resolver`]) — a framework with an open
//! set of types supplies readers on demand that way, under its own trust policy; an
//! identifier nothing recognizes is an error ([`DeserializeError::UnknownIdentifier`]),
//! never a guess.
//!
//! # The standard tables
//!
//! [`SerdeSession::new`] registers the drivers of the crate's own object kinds, in
//! this order: `sources` ([`SourceSerdeDriver`], positions [`SourceIndex`]), `states`
//! ([`StateSerdeDriver`], [`StateIndex`]), `specs` ([`SpecSerdeDriver`],
//! [`SpecIndex`]), `providers` ([`ProviderSerdeDriver`], [`ProviderIndex`]), `trees`
//! ([`TreeSerdeDriver`], [`TreeIndex`]), `diagnostics` ([`DiagnosticSerdeDriver`],
//! [`DiagnosticIndex`]), and `parse-results` ([`ParseResultSerdeDriver`],
//! [`ParseResultIndex`]). [`SerdeSession::standard_tables`] returns their handles as
//! a [`StandardTables`] bundle, and five extension traits intern and read by kind:
//! [`StandardTableInterning`] (`intern_source`, `intern_state`, `intern_spec`,
//! `intern_provider`) and [`StandardTableReading`] (`source`, `state`, `spec`,
//! `provider`), both on the session and on the contexts; [`TreeSerialization`]
//! (`serialize_tree` / `tree`), [`DiagnosticSerialization`] (`serialize_diagnostic` /
//! `diagnostic`), and [`ParseResultSerialization`] (`serialize_parse_result` /
//! `parse_result`) on the session. Each driver's page states its table's entry
//! layout in full; in brief:
//!
//! - **Sources.** A source's text is either *embedded* in its entry (the default) or
//!   *referenced* — kept outside the serialized form and described by its length and
//!   an optional *digest*, a fixed-size fingerprint of the text computed by a hash
//!   function the writer chooses, stored as the function's name and output
//!   ([`SourceDigest`]). The choice is a caller-supplied [`SourceTextPolicy`]
//!   ([`SourceTextForm`]); on reading, a caller-supplied [`SourceTextSupplier`]
//!   supplies the text of a referenced source ([`ReferencedSource`]) and verifies its
//!   digest — the crate implements no hash function. Both are configured on the driver
//!   ([`SerdeSession::with_source_driver`]). Every source is written once however
//!   often it is referred to, and read back as one shared `Arc<Source>`.
//! - **States.** A state's entry carries its token rules, mode, ext, and scope stack
//!   (as provider positions); the derived caches are rebuilt on reading. States are
//!   interned: written once, read back shared.
//! - **Specs and providers.** Heterogeneous tables whose readers a language or
//!   framework registers; see the next section.
//! - **Trees.** A tree's entry carries its nodes in storage order — spans, states,
//!   specs, and exts referring to the other tables — and, for a non-unit annotation
//!   type, one annotation value per node (annotation types are registered on the
//!   table's handle with [`TableHandle::register_annotation`]; the unit annotation is
//!   pre-registered under `core.tree`). The reader rebuilds the tree through the node
//!   builder, minting a fresh layout tag and re-establishing every structural
//!   invariant. A tree is a value: every `serialize_tree` call writes a new entry.
//! - **Diagnostics.** A diagnostic's entry carries its severity, its condition's
//!   identifier and serialization projection, its rendered message, its span, and its
//!   traceback frames; the reader rebuilds it with a [`DeserializedCondition`] as its
//!   condition — the written identifier, projection, and message as values — so that
//!   consumers on the far side match on [`Diagnostic::identifier`](crate::error::Diagnostic::identifier)
//!   rather than downcast to the original condition type. A diagnostic is a value:
//!   written in full on every call.
//! - **Parse results.** A parse result's entry ties a parse together: its tree's
//!   position, its diagnostics' positions with the collection's retention cap and
//!   counts, and its session extension. A parse result is interned by identity (its
//!   `Arc`) and read back as the shared `Arc` the session holds.
//!
//! # Specs and providers: identity or a self-contained form
//!
//! A [`Package`](crate::core::specs::Package) is part of the reading program's own
//! configuration and goes by identity: its name, resolved through the
//! [`KnownProviders`] directory the reading program sets as the session's user data —
//! the providers it holds, by name, plus [`ProviderRecipe`]s to build the ones it does
//! not hold (the preset's helpers register recipes for its own packages:
//! [`latexlike::serialize::register_package_recipes`](crate::latexlike::serialize::register_package_recipes),
//! [`minidefs::register_package_recipes`](crate::latexlike::minidefs::register_package_recipes)).
//! A spec that holds parsers (a [`StdCallableSpec`](crate::core::specs::StdCallableSpec),
//! the latexlike macro, environment, and specials specs) goes by identity too, through
//! the [`SpecProvenance`](crate::core::specs::SpecProvenance) stamp a package built
//! with [`Package::new_shared`](crate::core::specs::Package::new_shared) hands out
//! ([`Package::provenance_for`](crate::core::specs::Package::provenance_for)) — a
//! reference to the package's entry plus the definition key, resolved by looking the
//! key up in the reading side's package of that name: the very instance that package
//! holds, never a lookup re-run. A spec of such a type built outside a shared package
//! cannot be serialized ([`SerializeError::MissingProvenance`]). Scopes and fallback
//! providers are written in full (their definitions as spec positions), the error spec
//! and the preset's simple specs (`\begin`, `\end`, the paragraph break, `\input`) in
//! self-contained forms (a stamped `\begin` or `\input` spec goes by identity too).
//! The readers of the crate's own forms are registered with
//! [`register_core_readers`]; a language's own helper calls it and adds its own (the
//! preset's [`latexlike::serialize::register`](crate::latexlike::serialize::register)).
//!
//! # Absent values in the serialized form
//!
//! Two spellings of "nothing" occur, and both read back the same: a *field the crate's
//! own wire structures leave out* — an optional part that is absent (a source's
//! digest, a slot's name, an argument's region) is an omitted key, never a `null` — and
//! a *language-owned value that is null* — the slots a language fills with its own
//! values (a state's mode or ext, a node's ext, a source's origin, a parse result's
//! session extension) render whatever the language's value conversion produced, and
//! the crate's conversions of `()` and of an `Option` that is `None` produce `null`.
//! So a reader of the rendering sees `"origin": null` beside an omitted `"digest"`; the
//! difference is principled (an omitted key is the structure's, a `null` value is the
//! language's), and reading accepts a missing key and a `null` alike wherever a value
//! may be absent.
//!
//! # The `serde` cargo feature: rendering
//!
//! Everything above is unconditional plain Rust with no external dependency
//! (`no_std` + `alloc`): sessions produce and absorb in-memory segments without any
//! feature. The optional `serde` cargo feature adds the rendering layer:
//! `Serialize`/`Deserialize` impls for [`SerialValue`] and [`Segment`], which encode
//! through any serde format — the canonical rendering, stated for JSON on
//! [`SerialValue`]'s page (`Bytes` as `{"$bytes": "<base64>"}`, `Index` as
//! `{"$index": [table, position]}`, keys beginning with `$` reserved), and a compact
//! rendering for non-human-readable formats — plus the *bridge*, `to_value` /
//! `from_value`, which converts any type implementing serde's traits to and from a
//! `SerialValue`, enforcing the value model's rules ([`SerialValueError`]: no
//! floating-point numbers, no integers outside `i64`, string map keys, no `$`-keys),
//! and the `serial_bytes` helper module, which marks a byte-string field of a serde
//! type for it. Positions defined with [`serial_index!`] gain serde impls under the
//! feature. Trees with a plain-data annotation type can be registered through the
//! bridge (`TableHandle::register_serde_annotation`, available with the feature). The
//! feature adds no obligation to any implementer of the traits here: enabling it
//! changes no trait surface.
//!
//! # Errors and panics
//!
//! Four error types: [`SerializeError`] (the write side — a type's own failure,
//! wrapped with the table and node it happened in; a reference cycle; a spec without a
//! provenance stamp; an argument spec the default rule cannot serialize; a value
//! nesting too deep), [`DeserializeError`] (the read side — every validation failure,
//! naming what failed: a malformed value, an index out of range or into the wrong
//! table, an unknown identifier, a version or profile mismatch, a segment out of order,
//! a missing provider or definition, a span outside its source, a digest mismatch, …),
//! [`RegistrationError`] (setting a session up: duplicate table names or identifiers,
//! a handle of another session), and [`SerialValueError`] (converting plain data to or
//! from a `SerialValue`: the bridge's policy errors and shape mismatches, the nesting
//! bound). Every value read is bounded in nesting depth
//! ([`SerialValue::MAX_NESTING_DEPTH`], checked before any value is walked) and every
//! nested call in descent, so that no input — malformed or malicious — can exhaust the
//! stack. No public item of this module panics on any wire input or on any object the
//! parser or the node builder produced; the one panic reachable through it is the
//! crate-wide [`TextContent::resolve`](crate::source::TextContent::resolve) invariant
//! panic, reached by the tree writer on a consumer-built tree whose invocation-syntax
//! text spans outside its node's source (see [`TreeSerdeDriver`]).
//!
//! # Stability of the serialized form
//!
//! The serialized form — the layouts, table names, identifiers, key names, and enum
//! strings this module writes, and its canonical JSON rendering — is **not yet
//! frozen**. Every segment carries the layout version ([`Segment::VERSION`], currently
//! 1), and a reader accepts exactly its own; until the layout is declared final, it
//! may still change incompatibly, and streams written before such a change are not
//! preserved. Once frozen, the abstract structure and the canonical JSON rendering
//! become a public contract under the crate's usual stability rules: names hard-stable,
//! payload keys growing additively only, a layout change bumping the version. Not part
//! of the contract in any case: message wording (a diagnostic's message, a frame's
//! title), Rust type names (never on the wire), table ordinals (the directory maps
//! them per segment), and the compact rendering used by non-human-readable formats (a
//! same-version pairing between writer and reader).
//!
//! # Index of items
//!
//! - **The value model:** [`SerialValue`], [`SerialEntry`], [`TableId`],
//!   [`SerialIndex`], [`serial_index!`].
//! - **The capability traits:** [`SerializableObject`], [`DeserializableObject`],
//!   [`SerializableValue`], [`DeserializableValue`], [`SerializableLang`].
//! - **The engine:** [`SerdeSession`], [`SerializeContext`], [`DeserializeContext`],
//!   [`ObjectSerdeDriver`], [`TableHandle`], [`Segment`], [`SegmentMeta`],
//!   [`SegmentTable`], [`DispatchingSerdeDriver`], [`ObjectReader`],
//!   [`IdentifierResolver`].
//! - **The standard tables:** [`StandardTables`], [`StandardTableInterning`],
//!   [`StandardTableReading`]; [`SourceSerdeDriver`], [`SourceIndex`],
//!   [`SourceTextPolicy`], [`SourceTextForm`], [`SourceTextSupplier`],
//!   [`ReferencedSource`], [`SourceDigest`]; [`StateSerdeDriver`], [`StateIndex`];
//!   [`SpecSerdeDriver`], [`SpecIndex`], [`ProviderSerdeDriver`], [`ProviderIndex`],
//!   [`KnownProviders`], [`ProviderRecipe`], [`register_core_readers`];
//!   [`TreeSerdeDriver`], [`TreeIndex`], [`TreeSerialization`];
//!   [`DiagnosticSerdeDriver`], [`DiagnosticIndex`], [`DiagnosticSerialization`],
//!   [`DeserializedCondition`]; [`ParseResultSerdeDriver`], [`ParseResultIndex`],
//!   [`ParseResultSerialization`].
//! - **Errors:** [`SerializeError`], [`DeserializeError`], [`RegistrationError`],
//!   [`SerialValueError`].
//! - **With the `serde` feature:** `to_value`, `from_value`, `serial_bytes` (the
//!   bridge), and the `Serialize`/`Deserialize` impls of [`SerialValue`], [`Segment`],
//!   [`DiagnosticValue`](crate::error::DiagnosticValue), and every [`serial_index!`]
//!   type.
//! - **Elsewhere in the crate:** [`SpecProvenance`](crate::core::specs::SpecProvenance),
//!   [`DefinitionKey`](crate::core::specs::DefinitionKey),
//!   [`Package::new_shared`](crate::core::specs::Package::new_shared),
//!   [`Package::provenance_for`](crate::core::specs::Package::provenance_for),
//!   [`Package::provenance_for_specials`](crate::core::specs::Package::provenance_for_specials),
//!   [`Package::get_specials`](crate::core::specs::Package::get_specials),
//!   [`CallableSpec::serialize_argument_spec`](crate::core::specs::CallableSpec::serialize_argument_spec),
//!   [`CallableSpec::deserialize_argument_spec`](crate::core::specs::CallableSpec::deserialize_argument_spec),
//!   and the preset's [`latexlike::serialize`](crate::latexlike::serialize) module
//!   ([`register`](crate::latexlike::serialize::register),
//!   [`register_package_recipes`](crate::latexlike::serialize::register_package_recipes)).

mod drivers;
mod engine;
mod error;
mod object;
mod serial_index;
mod value;
pub(crate) mod wire;

#[cfg(feature = "serde")]
mod base64;
#[cfg(feature = "serde")]
pub(crate) mod bridge;
#[cfg(feature = "serde")]
mod render;

pub use drivers::{
    register_core_readers, DeserializedCondition, DiagnosticIndex, DiagnosticSerdeDriver,
    DiagnosticSerialization, KnownProviders, ParseResultIndex, ParseResultSerdeDriver,
    ParseResultSerialization, ProviderIndex, ProviderRecipe, ProviderSerdeDriver,
    ReferencedSource, SourceDigest, SourceIndex, SourceSerdeDriver, SourceTextForm,
    SourceTextPolicy, SourceTextSupplier, SpecIndex, SpecSerdeDriver, StandardTableInterning,
    StandardTableReading, StandardTables, StateIndex, StateSerdeDriver, TreeIndex,
    TreeSerdeDriver, TreeSerialization,
};
pub use engine::{
    DeserializeContext, DispatchingSerdeDriver, IdentifierResolver, ObjectReader,
    ObjectSerdeDriver, SerdeSession, Segment, SegmentMeta, SegmentTable, SerializeContext, TableHandle,
};
pub use error::{DeserializeError, RegistrationError, SerialValueError, SerializeError};
pub use object::{
    DeserializableObject, DeserializableValue, SerializableLang, SerializableObject,
    SerializableValue,
};
pub use value::{SerialEntry, SerialIndex, SerialValue, TableId};

// Shared bodies of the crate's own spec serialization, for the preset's impls.
pub(crate) use drivers::specs::{
    read_unit_recipe, serialize_stamped_spec, spec_and_provider_tables, unit_recipe_value,
};

// The typed-position macro is defined at the crate root (as every `macro_rules!`
// export is) and hidden there; this is its canonical, documented path.
#[doc(inline)]
pub use crate::serial_index;

#[cfg(feature = "serde")]
pub use bridge::{from_value, serial_bytes, to_value};

#[cfg(test)]
mod tests;
#[cfg(all(test, feature = "serde"))]
mod serde_tests;

// Test helpers reachable in-crate (the tree driver's tests, and the preset's later).
#[cfg(test)]
pub(crate) mod tree_support;
