# Serializing parses

This chapter introduces [`techy::serialize`](crate::serialize): writing what a
parse produced — the node tree, its parsing states and sources, the specs and
packages it used, the diagnostics — into a format-independent value model, and
rebuilding it elsewhere. It is an introduction to the everyday flow; the
[module documentation](crate::serialize) is the reference (vocabulary, the write
and read paths step by step, every table's layout, the engine, the errors) and
this chapter does not repeat it.

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

## A cache: write today, read tomorrow

The whole flow in one scenario. Today's process parses, serializes the parse
result as its segment's *main entry*, and keeps the segment as one JSON line
(this needs the `serde` feature). Tomorrow's process holds its own language and
its own session; it needs two things the writer did not: the *readers* of specs
and providers, registered by the preset's
[`latexlike::serialize::register`](crate::latexlike::serialize::register), and a
*reading environment* — a [`KnownProviders`](crate::serialize::KnownProviders)
holding the packages the parse used, so that every spec resolves to the very
instance this program holds.
[`push_segment`](crate::serialize::SerdeSession::push_segment) then hands back
the main entry in the reader's own numbering. Both sides declare the same
*profile*: a stream written for another configuration is refused up front.

```rust
# #[cfg(feature = "serde")]
# fn main() -> Result<(), Box<dyn std::error::Error>> {
use std::sync::Arc;
use techy::core::{Language, ParsingState};
use techy::error::Recovery;
use techy::latexlike::minidefs::{self, minilatex_package};
use techy::latexlike::serialize::{register, register_package_recipes};
use techy::latexlike::{Latexlike, LatexlikeDriver};
use techy::serialize::{KnownProviders, ParseResultSerialization, SerdeSession, Segment};

// The program's language, built the same way on both days.
fn build_language() -> Result<Language<Latexlike>, Box<dyn std::error::Error>> {
    Ok(Language::new(
        LatexlikeDriver::new(Recovery::Tolerant),
        ParsingState::lang_initial_with_packages([minilatex_package()])?,
    ))
}

// Today: parse, serialize, keep the line (in a file, a database row, …).
let language = build_language()?;
let result = Arc::new(language.parse(r"Hello \emph{world}")?);
let mut writer = SerdeSession::<Latexlike>::new();
writer.set_profile("my-app cache");
let position = writer.serialize_parse_result(&result)?;
let cached: String = serde_json::to_string(&writer.take_segment_with_main(position)?)?;

// Tomorrow: a fresh process, its own language, a reading session.
let language = build_language()?;
let mut known = KnownProviders::<Latexlike>::new();
for provider in language.initial_state().scopes().providers() {
    known.insert(Arc::clone(provider));                 // the packages this program holds
}
register_package_recipes(&mut known);                   // recipes for the preset's own
minidefs::register_package_recipes(&mut known);         // packages, in case it does not
let mut reader = SerdeSession::<Latexlike>::new();
reader.set_profile("my-app cache");
reader.set_user_data(known);
register(&mut reader)?;                                 // the readers, once per session

let segment: Segment = serde_json::from_str(&cached)?;
let (table, index) = reader.push_segment(segment)?.expect("the segment names its main entry");
let parse_results = reader.standard_tables().expect("standard tables").parse_results;
assert_eq!(table, parse_results.id());
let result = reader.parse_result(parse_results.position(index))?;
assert_eq!(result.tree.root().child(1).unwrap().macro_name(), Some("emph"));
# Ok(()) }
# #[cfg(not(feature = "serde"))]
# fn main() {}
```

A cache of many parses is the same stream continued: every further parse
result goes into the writer with `serialize_parse_result`, and each
`take_segment_with_main` yields the next line.

## Reading then appending

A session that absorbed a stream can continue it: intern the next parse and
emit a segment that refers back to what it already holds — nothing is written
twice.

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

## Where to read on

A parse read back is a new tree over the same shared sources, states, and spec
instances; what exactly a round trip preserves and what it does not (node ids,
tree tags, the concrete condition types of diagnostics) is stated at the end of
the module documentation's [Reading](crate::serialize#reading) section. The
rules that bind a stream (one stream per session, absorb before append,
profiles, JSON Lines) are its [Streams](crate::serialize#streams) section, and
everything else — the value model and its rendering, each table's layout,
sources kept outside the stream, custom tables and annotation types, how a
language or framework opts in, every error — follows on the same page.

Read next: back to the [Developer Guide](crate::guide#developer-guide) index —
the other chapters on extending and embedding techy.
