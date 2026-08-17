# Serializing parses

This chapter introduces [`techy::serialize`](crate::serialize): writing what a
parse produced — the node tree, its parsing states and sources, the specs and
packages it used, the diagnostics — into a format-independent value model, and
rebuilding it elsewhere. It is an introduction to the everyday flow; the
[module documentation](crate::serialize) is the reference (vocabulary, every
table's layout, the engine, the errors).

**When you would serialize.** To cache parses (parse many inputs over time,
append each parse to a stream, reread them later without reparsing — a corrupt
cache is a clean error, never a wrong tree); to hand parse results to another
process (a parser service feeding a renderer or a site builder); to pin the
output of a parse in a snapshot test (the rendering is deterministic and
diffable); or simply to inspect a parse as readable JSON.

**The model in one paragraph.** A [`SerdeSession`](crate::serialize::SerdeSession)
holds one *table* per kind of object — sources, states, specs, providers, trees,
diagnostics, parse results — and writes each object into its table **once**,
referring to it everywhere else by its *position*; that is how the sharing a
tree relies on (one state for thousands of nodes, one source for every span)
survives. What a session emits is a [`Segment`](crate::serialize::Segment): for
every table, the entries new since the last emission. The segments one session
emits form a *stream*; positions are numbered across the stream, so a reader
absorbs one stream's segments in order. Nothing here needs a cargo feature;
the optional `serde` feature adds the rendering through serde formats.

## Writing

Interning goes through extension traits named by kind — here
[`ParseResultSerialization`](crate::serialize::ParseResultSerialization),
whose `serialize_parse_result` interns the tree, its diagnostics, and
everything they refer to. Writing needs no registration.

```rust
use std::sync::Arc;
use techy::core::{Language, ParsingState};
use techy::error::Recovery;
use techy::latexlike::minidefs::minilatex_package;
use techy::latexlike::{Latexlike, LatexlikeDriver};
use techy::serialize::{ParseResultSerialization, SerdeSession};

let language: Language<Latexlike> = Language::new(
    LatexlikeDriver::new(Recovery::Tolerant),
    ParsingState::lang_initial_with_packages([minilatex_package()])?,
);
let result = Arc::new(language.parse(r"Hello \emph{world}")?);

let mut writer = SerdeSession::<Latexlike>::new();
let position = writer.serialize_parse_result(&result)?;
let segment = writer.take_segment_with_main(position)?;  // names the parse result as the main entry
# assert_eq!(segment.tables().len(), 7);
# Ok::<(), Box<dyn std::error::Error>>(())
```

With the `serde` feature, a segment renders through any serde format; the
canonical stream rendering is JSON Lines — one segment per line:

```rust
# #[cfg(feature = "serde")]
# fn main() -> Result<(), Box<dyn std::error::Error>> {
# use techy::core::{Language, ParsingState};
# use techy::error::Recovery;
# use techy::latexlike::{Latexlike, LatexlikeDriver};
# use techy::serialize::{SerdeSession, TreeSerialization};
# let language: Language<Latexlike> = Language::new(
#     LatexlikeDriver::new(Recovery::Tolerant), ParsingState::lang_initial()?);
# let result = language.parse("Hello {world}")?;
# let mut writer = SerdeSession::<Latexlike>::new();
# writer.serialize_tree(&result.tree)?;
# let segment = writer.take_segment();
let line = serde_json::to_string(&segment)?;
let back: techy::serialize::Segment = serde_json::from_str(&line)?;
assert_eq!(back, segment);
# Ok(()) }
# #[cfg(not(feature = "serde"))]
# fn main() {}
```

## Reading

A reading session needs what a writing session does not: the *readers* of
specs and providers (registered once, by the preset's
[`latexlike::serialize::register`](crate::latexlike::serialize::register)) and
a *reading environment* — a [`KnownProviders`](crate::serialize::KnownProviders)
holding the packages the parse used, so that a spec is resolved to the very
instance your program holds. Then
[`push_segment`](crate::serialize::SerdeSession::push_segment) absorbs each
segment of the stream in order and hands back its main entry, translated into
the reader's own table numbering.

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
# let result = Arc::new(language.parse(r"Hello \emph{world}")?);
# let mut writer = SerdeSession::<Latexlike>::new();
# let position = writer.serialize_parse_result(&result)?;
# let segment = writer.take_segment_with_main(position)?;

// The reading environment: the packages of the language that parsed, held by
// identity, plus recipes for the preset's packages your program may not hold.
let mut known = KnownProviders::<Latexlike>::new();
for provider in language.initial_state().scopes().providers() {
    known.insert(Arc::clone(provider));
}
register_package_recipes(&mut known);
minidefs::register_package_recipes(&mut known);

let mut reader = SerdeSession::<Latexlike>::new();
reader.set_user_data(known);
register(&mut reader)?;

let (table, index) = reader.push_segment(segment)?.expect("the segment names a main entry");
let parse_results = reader.standard_tables().expect("standard tables").parse_results;
assert_eq!(table, parse_results.id());
let back = reader.parse_result(parse_results.position(index))?;
assert_eq!(back.tree.root().child(1).unwrap().macro_name(), Some("emph"));
# Ok::<(), Box<dyn std::error::Error>>(())
```

**Reading then appending.** A session that absorbed a stream can continue it:
intern the next parse and emit a segment that refers back to what it already
holds — nothing is written twice.

```rust
# use std::sync::Arc;
# use techy::core::{Language, ParsingState};
# use techy::error::Recovery;
# use techy::latexlike::{Latexlike, LatexlikeDriver};
# use techy::serialize::{ParseResultSerialization, SerdeSession};
# let language: Language<Latexlike> = Language::new(
#     LatexlikeDriver::new(Recovery::Tolerant), ParsingState::lang_initial()?);
# let mut known = techy::serialize::KnownProviders::<Latexlike>::new();
# for provider in language.initial_state().scopes().providers() { known.insert(Arc::clone(provider)); }
# let mut reader = SerdeSession::<Latexlike>::new();
# reader.set_user_data(known);
# techy::latexlike::serialize::register(&mut reader)?;
# let mut writer = SerdeSession::<Latexlike>::new();
# writer.serialize_parse_result(&Arc::new(language.parse("a")?))?;
# reader.push_segment(writer.take_segment())?;
let next = Arc::new(language.parse("The next input")?);
let position = reader.serialize_parse_result(&next)?;
let continuation = reader.take_segment_with_main(position)?;   // the stream's next segment
# assert!(!continuation.is_empty());
# Ok::<(), Box<dyn std::error::Error>>(())
```

## What survives, and what does not

A tree read back has the same structure, the same spans into the same sources
(shared as before), the same parsing states (shared as before), and — for
specs resolved by identity — the very spec instances the reading environment
holds. Diagnostics keep their identifiers, projections, messages, and spans.
What does not survive: node ids and tree tags (a rebuilt tree is a new tree;
durable node identity travels in annotations), and the concrete condition
types of diagnostics (a diagnostic read back carries a
[`DeserializedCondition`](crate::serialize::DeserializedCondition); match on
the identifier). Identity resolution needs the reading side to hold the *same*
packages — a package of the same name that defines things differently is not
detected; declare a *profile* ([`set_profile`](crate::serialize::SerdeSession::set_profile))
on both sides so that a stream written for one configuration is refused up
front by a reader configured for another.

Everything else — the value model and its JSON rendering, the layout of every
table, sources kept outside the stream and verified by digest, custom tables
and annotation types, how a language or framework opts in, every error — is in
the [`serialize`](crate::serialize) module documentation.

Read next: [Migrating from pylatexenc](crate::guide::pylatexenc_migration)
— the concept mappings for readers arriving from the Python library.
