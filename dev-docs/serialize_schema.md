# The serialized form: schema description

This document describes the serialized form that `techy::serialize` writes and reads —
the layout of a stream, of the segments it consists of, and of every kind of entry —
for a developer who needs to read, produce, or reason about the data without deep
knowledge of techy's code base. It is a developer document: the user-facing
description of the API is the rustdoc of `techy::serialize` (its module page defines
the vocabulary and walks the write and read paths), and the guide chapter
`docs/serialize.md` is the introduction. The design decisions behind the form are in
the Design Rationale ([§dd-dr:serialize-capability-traits],
[§dd-dr:serial-value-model], [§dd-dr:instance-not-lookup],
[§dd-dr:serialize-sessions-segments]); the architecture summary is
[§dd-arch:serialization].

**The structures are the schema.** The layouts below are what the crate's own wire
structures (`techy/src/serialize/wire/*.rs`) and value conversions produce; this text
describes them and must be kept in step with them. Where the two disagree, the code
is right and this file has a bug.

Reading guide: §1 what a stream and a segment are, and why; §2 the value model and its
canonical JSON rendering; §3 the segment: envelope and table directory; §4 the standard
tables, entry by entry, with a worked example from one real parse; §5 the parts a
language supplies (the latexlike preset as the template); §6 stream conventions; §7
identity entries, self-contained entries, and the reading environment; §8 the
compatibility policy.

---

## 1. Streams and segments

techy parses text into node trees. A tree refers to other objects: every node records
the parsing state it was parsed under, every span names its source, every callable
node names its callable spec, and every state names the providers (packages, scopes)
on its scope stack. Many of these objects are shared — one state by thousands of nodes,
one source by every span into it, one spec by every invocation of a macro — and the
sharing is meaningful (span equality compares source identity, for one). Serialization
therefore does not write objects inline where they are used: it writes each object
**once**, into a **table** of objects of its kind, and refers to it everywhere else by
its **position** in that table. A serialization **session** owns the tables; what it
emits is a **segment**: for every table, the entries added since the previous
emission. The sequence of segments one session emits is a **stream**; positions are
numbered across the whole stream, so a later segment's entries refer to earlier
segments' entries by position, and a reader must absorb one stream's segments in
order, each once.

Why this shape: it serves a cache of parses (a tool parses many inputs over time and
appends each parse's segment to a stream; sources can be kept outside the stream and
verified by digest; a corrupt cache is a clean error, never wrong offsets), a complete
self-contained record of a parse (a framework serializes a whole parse result to
render it later — sources embedded, version carried, long-lived), inter-process
exchange (a parser service sends batches of trees that share tables), golden-file
tests (the rendering is deterministic and diffable), and inspection (the canonical
JSON is readable). Streams can be read then appended to: a session that absorbed a
stream may add new objects and emit further segments continuing it, and everything it
already holds is referred to, never written twice.

## 2. The value model and its canonical JSON rendering

Every serialized thing is a **serialized value** (`SerialValue`) — a small,
format-independent tree of values:

| Variant | Meaning | Canonical JSON |
|---|---|---|
| `Null` | the absent value | `null` |
| `Bool` | a boolean | `true` / `false` |
| `Int` (i64) | an integer of any width; out-of-range values are errors, never truncated (see "Integer widths") | number |
| `Str` | a string | string |
| `Bytes` | a byte string | `{"$bytes": "<base64>"}` — standard alphabet (`A–Z a–z 0–9 + /`), `=` padding, no line breaks; strict decoder |
| `List` | an ordered sequence | array |
| `Map` | string-keyed, an **ordered list of entries**: equality is order-sensitive and the rendering preserves the order — the same entries in another order are a different value with a different rendering; keys unique by construction, never beginning with `$` | object, in entry order |
| `Index {table, index}` | a reference to entry `index` of table `table` | `{"$index": [<table ordinal>, <index>]}` — the ordinal is the **writer's** table numbering, translated by the reader through the segment's table directory (§3) |

**Why so few variants.** The public contract is the canonical JSON rendering, so the
value model holds nothing the rendering cannot round-trip distinguishably: there are no
floating-point numbers and no sized-integer variants (they would all render as the same
JSON number — two unequal values with identical bytes), maps are string-keyed, and the
base64 form of a byte string is pinned. Two values render identically exactly when
they compare equal, which is what makes golden files and content-addressed caches
work.

**Reserved `$` keys.** `$bytes` and `$index` are the only keys beginning with `$` the
rendering ever writes, and the value model **reserves the `$` prefix**: a `Map` holding
a key that begins with `$` is a rendering error (canonical and compact renderings
alike), the serde bridge refuses such a key, field name, or variant name used as a map
key (`SerialValueError::ReservedMapKey`; a unit variant rendered as a string *value* is
unrestricted), and on reading, an object key beginning with `$` that is not one of the
two reserved forms is an error. There is no escaping.

**Integer widths.** Every integer on the wire is an `i64` whatever its Rust width.
Counts, byte offsets, positions, and limits that are `usize` in the live types (a
diagnostics collection's `limit`, `suppressed`, and `error_count`; a source's `length`;
span offsets; node counts; line and column offsets) are written as `i64` — a `usize`
above `i64::MAX` cannot be written (`SerialValueError::IntegerOutOfRange`, target
`i64`) — and read back with a range check against the reader's own type: a value that
fits `i64` but not the reader's `usize` (a 64-bit writer's value above `u32::MAX` read
on a 32-bit target) is `IntegerOutOfRange` (target `usize`), a typed error, never a
truncation. The schema states no portable bound; a stream meant for 32-bit readers
keeps such quantities within `u32`. (Wire integers that are `u32` in the live types —
table ids, positions, node indices — fit every reader.)

**Nesting depth.** A value nests at most `SerialValue::MAX_NESTING_DEPTH` (64) levels
of lists and maps. The segment's own structure (the segment map, `tables`, a table's
map, `entries`) counts, so an entry's stored form nests at most 60 levels (59 for a
heterogeneous table's `{identifier, data}` wrapper). Every reader enforces the bound as
it reads (`Deserialize` for `SerialValue`/`Segment`, `Segment::from_serial_value`,
`push_segment`; `SerialValueError::NestingTooDeep`), and a session refuses to intern a
deeper entry — no writer emits a segment no reader accepts. The bound is well below
serde_json's own default recursion limit (128), so the same bound is in force in every
format.

**Other formats.** Any serde format other than a human-readable one receives the
*compact* rendering (serde's externally tagged form of the `SerialValue` enum, `Bytes`
through the format's own byte-string channel, `Index` as a two-integer pair). It is a
private same-version pairing between writer and reader, not part of the public
contract.

**Value-level shapes reused everywhere below:**

- **span** — `{"source": <index into sources>, "start": <int>, "end": <int>}`: byte
  offsets, half-open, validated on reading against the source's text (within bounds,
  on character boundaries).
- **range** — `{"start": <int>, "end": <int>}`, half-open.
- **text content** — `{"spanned": {"start", "end"}}` (a byte range into the carrying
  node's own source; validated on reading) or `{"owned": "<text>"}`. Language-owned
  payloads carry owned text only.
- **language-owned values** — values a language's own conversions produce (a mode, an
  ext value, an invocation syntax, a callable type): rendered verbatim where they
  occur; `()` renders `null`; the crate's `Option<T>` conversion renders `None` as
  `null`.
- **omitted keys and `null`** — an optional field of a wire structure that is absent is
  an **omitted key** (never `null`); a present `null` reads back as absent too. So two
  spellings of "nothing" occur side by side — an omitted key (the structure's) and a
  `null` value (a language-owned slot's) — and both read back the same. The `Option`
  conventions are collected in §2.1.

### 2.1 `Option` conventions: where a key is omitted and where `null` appears

Two mechanisms produce the wire, and one convention governs both: an optional field of
a wire structure that is `None` is an **omitted key** (read back as `None` from a
missing key or from a `null`); a field holding a **language-owned value** renders
whatever the language's conversion produced, and the crate's conversions of `()` and
of an `Option` that is `None` write `null`.

| Where | `None` renders as | Why |
|---|---|---|
| source `digest` | omitted | optional structure field |
| state `groups.expecting_close` | omitted | optional structure field |
| state `token_rules.<section>` (feature absent) | omitted | optional structure field |
| tree `annotations` (unit annotation) | omitted | optional structure field |
| argument `region`, `spec_payload` (and `ext` of an ABSENT argument) | omitted | optional structure field |
| argument `ext` of a provided argument whose ext is `()` | `null` | the ext value is `Some(null)` |
| slot `name` | omitted | optional structure field |
| error-spec `detail` | omitted | optional structure field |
| environment syntax `end` (latexlike) | omitted | optional structure field |
| segment `main`; meta `profile` | omitted | optional structure field |
| source `origin` (`Option<String>` = `None`) | `null` | language-owned value: the language's `SourceOrigin` conversion |
| state `mode`, `ext`; node `ext`; slot `ext`; parse-result `session_ext`; group `group_type` (a language whose type is `()`) | `null` | language-owned values whose type is `()` (or an `Option` that is `None`) |

## 3. The segment

A segment is what one `SerdeSession::take_segment` (or `take_segment_with_main`)
emits and one `push_segment` absorbs. Its rendering has four keys, in this order:
`version`, `meta`, `tables`, `main`.

- `version` — the layout version, in every segment (`Segment::VERSION` = 1); a reader
  accepts exactly its own version (`DeserializeError::UnsupportedVersion`).
- `meta` — what the segment says about itself as a whole, as opposed to its entries
  (`SegmentMeta`); **always present** (an empty object when nothing is set, so that
  later layouts can add segment-wide keys). Today one optional key: `profile` — the
  emitting session's *profile*, a caller-chosen string naming the configuration that
  reads the stream fully (the environment and version whose packages, spec types,
  annotation identifiers, and readers resolve every identity and identifier in the
  stream — the caller's contract, which techy only compares; `SerdeSession::set_profile`).
  A reading session that declares a profile refuses a segment whose profile differs or
  is missing (`DeserializeError::ProfileMismatch`, before anything is absorbed); one
  without a profile accepts any.
- `tables` — the **table directory**: every table the emitting session has registered,
  in its registration order, whether or not the segment carries entries for it. Each
  directory row is `{"name", "table", "start", "entries"}`: `name` (how a reading
  session finds its own table, whatever its registration order), `table` (the writer's
  ordinal — its `TableId` — which every `$index` inside the segment uses), `start`
  (the position of the row's first entry — the reader checks it continues its table:
  `SegmentOutOfOrder` otherwise), `entries` (in position order).
- `main` — **optional**: the segment's main entry, the one entry the segment is about
  (the parse result of this line in a stream of parse results), as a table position in
  the writer's numbering; omitted when the session named none. `push_segment` returns
  it translated into the reading session's numbering and bounds-checked against the
  tables after the push (`Result<Option<(TableId, u32)>, _>`), so a reader finds each
  segment's payload without knowing the tables' layout.
- An entry of a **homogeneous** table (one kind of object; the table implies the
  identifier) is the bare payload; an entry of a **heterogeneous** table is
  `{"identifier": "<identifier>", "data": <payload>}`.
- Positions are stream-scoped: a later segment's entries refer to earlier segments'
  entries by position; a reader absorbs one stream's segments in order, each once.

An abridged illustration of a segment with the seven standard tables (the `…` stand
for the entries listed table by table in §4; the exact, unabridged rendering of this
very segment is the worked example there):

```json
{
  "version": 1,
  "meta": {"profile": "schema example"},
  "tables": [
    {"name": "sources",       "table": 0, "start": 0, "entries": [ … ]},
    {"name": "states",        "table": 1, "start": 0, "entries": [ … ]},
    {"name": "specs",         "table": 2, "start": 0, "entries": [ … ]},
    {"name": "providers",     "table": 3, "start": 0, "entries": [ … ]},
    {"name": "trees",         "table": 4, "start": 0, "entries": [ … ]},
    {"name": "diagnostics",   "table": 5, "start": 0, "entries": [ … ]},
    {"name": "parse-results", "table": 6, "start": 0, "entries": [ … ]}
  ],
  "main": {"$index": [6, 0]}
}
```

## 4. The standard tables, with a worked example

`SerdeSession::new()` registers seven tables, in this order:

| Ordinal | Name | Kind | Entry identifier |
|---|---|---|---|
| 0 | `sources` | homogeneous | `core.source` |
| 1 | `states` | homogeneous | `core.state` |
| 2 | `specs` | heterogeneous | per entry (`core.provider-spec-identity`, `core.error-spec`, `latexlike.begin`, …) |
| 3 | `providers` | heterogeneous | per entry (`core.package`, `core.scope`, `core.fallback-provider`) |
| 4 | `trees` | heterogeneous by annotation type | `core.tree` for the unit annotation; a registered identifier per other annotation type |
| 5 | `diagnostics` | homogeneous | `core.diagnostic` |
| 6 | `parse-results` | homogeneous | `core.parse-result` |

The ordinal is what a table reference (`$index`) names in the canonical JSON; a reader
matches tables by NAME through the directory and never relies on the ordinal.
Vocabulary conventions: table names are lowercase plural nouns, kebab-cased when
multi-word (`parse-results`); entry identifiers are two-part `<owner>.<kind>`
(`core.source`, `latexlike.begin`) with kebab-cased kinds; key names are snake_case
(`line_number_offset`, `callable_type`); enum strings are kebab-case (`end-of-input`,
`exit-math-context`; single words otherwise: `primary`, `macro`).

**The worked example.** Everything below is cut from ONE segment: a tolerant latexlike
parse of the text `\e{x} {` (a macro `\e` with one mandatory argument, defined in a
shared package `d`; then an unclosed group), serialized as a parse result with
`session.serialize_parse_result(&Arc::new(result))` into a fresh
`SerdeSession::<Latexlike>::new()` that declared the profile `schema example`,
emitted with `take_segment_with_main(position)` (the parse result as the segment's main
entry), and rendered with `serde_json::to_string`. **The example is generated, never
edited by hand.** Regeneration recipe: the ignored test
`latexlike::serialize_tests::rendering::schema_worked_example` prints the exact
canonical line, an envelope summary line, and the readable per-entry layout reproduced
below —

```text
cargo test --features serde -p techy --lib schema_worked_example -- --ignored --nocapture
```

— after any wire change, rerun it and paste; the readable layout puts one key per line
at the first two levels (one node per line for a tree) and is compact below that. The
same parse, pretty-printed in full (profile `techy golden files` instead), is the golden
file `techy/tests/golden/serialize/schema_example.json`, which
`cargo test --features serde -p techy --test serialize_golden` pins byte for byte — so
this document, the generator, and the test suite cannot silently disagree.

The generator's envelope summary of the example, verbatim:

```text
version 1; meta {"profile": "schema example"}; main {"$index": [6, 0]}
```

and its table headers, verbatim (ordinal, start position, entry count):

```text
=== table `sources` (ordinal 0, start 0, 1 entries) ===
=== table `states` (ordinal 1, start 0, 2 entries) ===
=== table `specs` (ordinal 2, start 0, 1 entries) ===
=== table `providers` (ordinal 3, start 0, 2 entries) ===
=== table `trees` (ordinal 4, start 0, 1 entries) ===
=== table `diagnostics` (ordinal 5, start 0, 1 entries) ===
=== table `parse-results` (ordinal 6, start 0, 1 entries) ===
```

### 4.1 `sources` (ordinal 0) — homogeneous, identifier `core.source`

Abstract structure (`WireSource`): `origin` (language-owned value; `Option<String>` by
default → string or `null`), `provenance` (`"primary"` | `{"resolved": {"reference",
"triggered_at": span}}` | `{"synthesized": {"description", "triggered_at": span}}` — a
resolved or synthesized source refers to the source its triggering location lies in,
so provenance chains are references between entries of this table, always to earlier
positions), `line_number_offset`, `column_number_offset` (integers), `text`
(`{"embedded": "<text>"}` | `{"referenced": {"length": <int>, "digest"?: {"algorithm":
"<name>", "bytes": <bytes>}}}` — the writer's policy decides per source; a referenced
source's text comes from the reader's supplier, checked against `length` and, when
present, `digest`).

Example (entry 0):

```json
{
  "origin": null,
  "provenance": "primary",
  "line_number_offset": 1,
  "column_number_offset": 1,
  "text": {"embedded":"\\e{x} {"}
}
```

A referenced source with a digest renders `"text": {"referenced": {"length": 48210,
"digest": {"algorithm": "sha256", "bytes": {"$bytes": "…"}}}}` (illustrative; the
digest algorithm and its verification are the caller's — techy neither chooses nor
implements a hash function).

### 4.2 `states` (ordinal 1) — homogeneous, identifier `core.state`

Abstract structure (`WireState`): `token_rules` (the token rules: one section per
feature the language declares present — a section for an absent feature is omitted; a
missing section for a present feature reads as that feature's empty rules —
`whitespace {enabled, chars}`, `paragraphs {enabled}`, `groups {enabled, rules: [group
rule…], temporary: [group rule…], expecting_close?: group rule}`, `commands {enabled,
rules: [{escape_char (a one-character string), name_chars}]}`, `comments {enabled,
rules: [{start}]}`, `specials {enabled}`, `forbidden_chars {chars}`; a group rule is
`{group_type: <language-owned>, open, close}`), `mode` and `ext` (language-owned
values), `scopes` (the scope stack: references into `providers`, outermost first). The
derived caches (delimiter prefix table, specials trigger characters) are never written.

Example (entry 0, the seed state; entry 1 is the state inside `\e`'s argument group —
the same shape with `"expecting_close":{"group_type":"content","open":"{","close":"}"}`
added inside `groups`):

```json
{
  "token_rules": {
    "whitespace": {"enabled":true,"chars":" \t\n\r\u000b\f"},
    "paragraphs": {"enabled":true},
    "groups": {"enabled":true,"rules":[{"group_type":"content","open":"{","close":"}"},{"group_type":{"math":"inline"},"open":"$","close":"$"},{"group_type":{"math":"display"},"open":"$$","close":"$$"},{"group_type":{"math":"inline"},"open":"\\(","close":"\\)"},{"group_type":{"math":"display"},"open":"\\[","close":"\\]"}],"temporary":[]},
    "commands": {"enabled":true,"rules":[{"escape_char":"\\","name_chars":"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ"}]},
    "comments": {"enabled":true,"rules":[{"start":"%"}]},
    "specials": {"enabled":true},
    "forbidden_chars": {"chars":""}
  },
  "mode": "text",
  "ext": null,
  "scopes": [{"$index":[3,0]},{"$index":[3,1]}]
}
```

### 4.3 `specs` (ordinal 2) — heterogeneous (`{"identifier", "data"}`)

Entries of the specs table are callable specs, each under its own identifier:

- `core.provider-spec-identity` — a spec named **by identity** through its provenance
  stamp: `{"provider": <index into providers>, "callable_type": <language-owned>,
  "definition": {"name": "<registered name>"} | {"trigger": "<specials trigger>"}}`.
  Read by looking the definition key up in the reading side's package of that name —
  the very instance that package holds. Used by every spec type that holds parsers
  (core `StdCallableSpec`, latexlike `MacroSpec`, `EnvironmentSpec`, `SpecialsSpec`,
  and stamped `BeginSpec`/`InputMacroSpec`).
- `core.error-spec` — `{"detail"?: "<text>"}` (self-contained).
- language-owned self-contained forms — `latexlike.begin {end_command_name}`,
  `latexlike.end {}`, `latexlike.paragraph-break {}`, `latexlike.input {persist_state,
  attached_slot_ext}` (§5).
- a framework's own identifiers, read through the readers or resolvers it registers.

Example (entry 0 — `\e`, defined in package `d`, which is providers entry 1):

```json
{
  "identifier": "core.provider-spec-identity",
  "data": {
    "provider": {"$index":[3,1]},
    "callable_type": "macro",
    "definition": {"name":"e"}
  }
}
```

### 4.4 `providers` (ordinal 3) — heterogeneous

- `core.package` — `{"name": "<package name>"}` (identity: resolved through the reading
  program's `KnownProviders` — a held provider of that name, else a registered recipe).
  Package names are the language's stable vocabulary (`_builtin`, `minilatex`,
  `minilatex.item` for the preset).
- `core.scope` — `{"name", "definitions": [{"callable_type", "name", "spec": <index
  into specs>}…]}` (in full; definitions ordered by callable type, then name).
- `core.fallback-provider` — `{"name", "fallbacks": [{"callable_type", "spec"}…]}`.

Example (entries 0 and 1 — the seed package and `d`):

```json
{
  "identifier": "core.package",
  "data": {"name":"_builtin"}
}
{
  "identifier": "core.package",
  "data": {"name":"d"}
}
```

### 4.5 `trees` (ordinal 4) — heterogeneous by annotation type; `core.tree` = the unit annotation

Abstract structure (`WireTree`): `nodes` — every node in storage order (the root first,
then each node's children as one contiguous block; the list is EXACTLY the set
reachable from the root), each `{"kind", "span", "state": <index into states>, "ext":
<language-owned>, "children": range}` where `children` is a storage-index range and
`kind` is one of `"list"`, `{"chars": {"content": text}}`, `{"group": {"group_type":
<language-owned>, "open": text, "close": text}}`, `{"comment": {"start": text,
"content": text, "post_space": text}}`, `{"callable": {"callable_type":
<language-owned>, "name", "spec": <index into specs>, "arguments": [argument…],
"slots": [slot…], "invocation_syntax": <language-owned>}}`; `annotations` — omitted for
the unit annotation, else one language- or caller-owned value per node (registered
with `TableHandle::register_annotation`).

An **argument** is `{"region"?: region, "ext"?: <language-owned>, "spec_payload"?:
<spec-owned>}` — a provided argument carries `region` and its `ext` (written as `null`
for the unit ext; a reader accepts a `null` value and an omitted key alike), an absent
argument is `{}`; `spec_payload` is present only when the callable spec serializes an
argument spec out of band (see `CallableSpec::serialize_argument_spec`). A **slot** is
`{"name"?, "region", "role": "content"|"attached"|"hidden", "ext": <language-owned>}`.
A **region** is `{"children": range, "content": <content>}` — `children` are offsets
into the callable's own child list; `content` is one of two explicit variants
mirroring the live `ContentNodes`: `{"in_region": {"start", "end"}}` (offsets within
the region's own node list — content sitting directly among the callable's children)
or `{"in_children_of": {"node": <storage index>, "start", "end"}}` (offsets within the
child list of the node stored at `node`, a node inside the region — an argument's
group, a slot's body list; the reader requires it to be stored after the callable and
the builder checks it lies inside the region's subtree). Tree layout tags and the
parent table are never written; the reader rebuilds through the node builder.

Example (entry 0 — the whole tree; six nodes: the root list, `\e`, a chars ` `, the
unclosed group `{` (its `close` recovered as owned `""`), then `\e`'s argument group
and its chars `x`; the argument's region names node 4, the group, as the node whose
children are its content):

```json
{"identifier": "core.tree", "data": {"nodes": [
  {"kind":"list","span":{"source":{"$index":[0,0]},"start":0,"end":7},"state":{"$index":[1,0]},"ext":null,"children":{"start":1,"end":4}},
  {"kind":{"callable":{"callable_type":"macro","name":"e","spec":{"$index":[2,0]},"arguments":[{"region":{"children":{"start":0,"end":1},"content":{"in_children_of":{"node":4,"start":0,"end":1}}},"ext":null}],"slots":[],"invocation_syntax":{"macro":{"escape_char":"\\","post_space":{"owned":""}}}}},"span":{"source":{"$index":[0,0]},"start":0,"end":5},"state":{"$index":[1,0]},"ext":null,"children":{"start":4,"end":5}},
  {"kind":{"chars":{"content":{"spanned":{"start":5,"end":6}}}},"span":{"source":{"$index":[0,0]},"start":5,"end":6},"state":{"$index":[1,0]},"ext":null,"children":{"start":5,"end":5}},
  {"kind":{"group":{"group_type":"content","open":{"spanned":{"start":6,"end":7}},"close":{"owned":""}}},"span":{"source":{"$index":[0,0]},"start":6,"end":7},"state":{"$index":[1,0]},"ext":null,"children":{"start":5,"end":5}},
  {"kind":{"group":{"group_type":"content","open":{"spanned":{"start":2,"end":3}},"close":{"spanned":{"start":4,"end":5}}}},"span":{"source":{"$index":[0,0]},"start":2,"end":5},"state":{"$index":[1,0]},"ext":null,"children":{"start":5,"end":6}},
  {"kind":{"chars":{"content":{"spanned":{"start":3,"end":4}}}},"span":{"source":{"$index":[0,0]},"start":3,"end":4},"state":{"$index":[1,1]},"ext":null,"children":{"start":6,"end":6}}
]}}
```

(The `\e` argument's `"ext": null`: the region is present and the latexlike argument
ext is `()`, so the key is written with `null`; an absent argument writes neither
`region` nor `ext`. The reader tolerates an omitted key and a `null` alike for the unit
ext.)

### 4.6 `diagnostics` (ordinal 5) — homogeneous, identifier `core.diagnostic`

Abstract structure (`WireDiagnostic`), in this key order: `severity` (`"note"` |
`"warning"` | `"error"`), `identifier` (the condition's wire identity, e.g.
`core.groups.unclosed-group`), `message` (the human message as rendered when the
diagnostic was written; its wording is not stable), `data` (the condition's
`serializable_data()` projection — a `DiagnosticValue`: `null`, booleans, integers,
strings, lists, string-keyed maps; never a byte string or a table position; its keys
are the condition's stable, additive-only projection keys — the derive's field names or
a field's `#[diagnostic(key = "…")]`), `span`, `frames` (the traceback snapshot,
innermost first, each `{"title": "<rendered frame title>", "span": span}`). Read back
as a diagnostic whose condition is a `DeserializedCondition` holding those values:
same identifier, projection, and message; no downcast to the original condition type.
The condition identifiers themselves and their projection keys are the wire vocabulary
frozen by [§dd-dr:wire-identifier-stability].

Example (entry 0 — the unclosed group; a top-level group carries no traceback frame):

```json
{
  "severity": "error",
  "identifier": "core.groups.unclosed-group",
  "message": "unclosed group: expected ‘}’ before end of input",
  "data": {
    "expected_close": "}",
    "found": "end-of-input"
  },
  "span": {
    "source": {"$index":[0,0]},
    "start": 6,
    "end": 7
  },
  "frames": []
}
```

A diagnostic with frames (from `\emph{x`, latexlike, tolerant): `"frames": [{"title":
"argument #1 of macro ‘\\emph’", "span": {…, "start": 5, "end": 5}}, {"title": "macro
‘\\emph’", "span": {…, "start": 0, "end": 5}}]`.

### 4.7 `parse-results` (ordinal 6) — homogeneous, identifier `core.parse-result`

Abstract structure (`WireParseResult`): `tree` (index into `trees`; a `core.tree`
entry), `diagnostics` (`{"items": [index into diagnostics…] in recording order, "limit":
<int>, "suppressed": <int>, "error_count": <int>}` — the collection's retention cap and
counts, a NESTED object mirroring the live `Diagnostics`, cross-checked on reading:
`items ≤ limit`, `suppressed > 0` only when `items == limit`, `retained errors ≤
error_count ≤ retained errors + suppressed`), `session_ext` (language-owned value;
`null` for `()`). A parse result is interned by identity (its `Arc`) and read back as
the shared `Arc<ParseResult>`. (A collection created with a `limit` above `i64::MAX`
cannot be serialized: `IntegerOutOfRange`.)

Example (entry 0):

```json
{
  "tree": {"$index":[4,0]},
  "diagnostics": {
    "items": [{"$index":[5,0]}],
    "limit": 1000,
    "suppressed": 0,
    "error_count": 1
  },
  "session_ext": null
}
```

## 5. Language-owned parts (latexlike, as the template of what a language supplies)

A language declares itself serializable (`SerializableLang`) by supplying value
conversions for every type it hands the parse; those values render verbatim inside the
core structures above:

| Slot | latexlike value | Rendering |
|---|---|---|
| callable type (`CallableTypeId`) | `CallableType` | `"macro"` / `"environment"` / `"specials"` |
| group type (`GroupTypeId`) | `GroupType` | `"content"` / `{"math": "inline"}` / `{"math": "display"}` / `"verbatim"` |
| mode (`ModeId`) | `Mode` | `"text"` / `"math"` |
| event | `Event` | `"exit-math-context"` |
| state ext, node ext, argument ext | `()` | `null` |
| slot ext | `BodyMarker` | `{"body": true}` / `{"body": false}` |
| session ext | `()` | `null` |
| source origin | `Option<String>` | string or `null` |
| invocation syntax | `InvocationSyntaxData` | `{"macro": {"escape_char": "\\", "post_space": text}}` / `{"environment": {"begin": side, "end"?: side}}` with side = `{"escape_char", "command_word": text, "post_space": text, "name_group_rule": {"group_type", "open", "close"}}` / `"specials"` |

Its spec forms: identity (`core.provider-spec-identity`) for stamped `MacroSpec`,
`SpecialsSpec`, `EnvironmentSpec`, `BeginSpec`, `InputMacroSpec`; self-contained
`latexlike.begin {"end_command_name"}`, `latexlike.end {}`, `latexlike.paragraph-break {}`,
`latexlike.input {"persist_state": bool, "attached_slot_ext": <slot ext>}`. Its
providers: `core.package` by name (`_builtin`, `minilatex`, `minilatex.item` — stable
vocabulary), resolved by the reading side's `KnownProviders` (the language's
`register_package_recipes` helpers add recipes for them). Its condition identifiers:
`latexlike.environments.{malformed-begin, orphan-end, unknown-environment}`.

## 6. Stream conventions

- **JSON Lines** (canonical stream rendering): one segment per line
  (`serde_json::to_string(&segment)` — the compact rendering contains no raw line
  break), lines in emission order; a reader decodes each line
  (`serde_json::from_str::<Segment>`) and pushes it in order. Every line is an
  independently valid segment (its own `version`, `meta`, and full directory) but
  positions are stream-scoped (a line pushed without its predecessors is
  `SegmentOutOfOrder`). No end-of-stream marker: the stream ends where the input ends;
  appending = appending lines; a truncated last line loses only itself. A line's `main`
  names the payload it is about; `push_segment` hands it back translated. (Tests:
  `techy/tests/serialize_stream.rs`.)
- **Other formats**: one segment per framed value (postcard with a length prefix, a
  file per segment, a message per segment), same ordering rules; the compact rendering
  is a private same-version pairing.
- **Contiguity and order**: each directory row's `start` must equal the reader's
  current length of that table; the segments pushed into one session must belong to
  ONE stream, in order — the session detects a skipped or repeated segment but cannot
  recognize a foreign stream whose positions happen to line up (the caller's
  obligation; a declared profile narrows what a reader accepts). A session absorbs
  before it appends: pushing a segment while entries are pending emission is
  `DeserializeError::UnemittedEntries`.
- **Positions are session-scoped in Rust code**: inside a segment a position is a
  stream-scoped `u32` paired with the writer's table ordinal, translated by the reader
  through the directory; a *typed* position held in Rust code (`SourceIndex`,
  `TreeIndex`, …) carries the holding session's `TableId` and is exchanged between
  sessions only as (table name, `u32`), rebuilt on the receiving side with
  `TableHandle::position(u32)`.
- **Reading then appending**: a session that absorbed a stream may intern further
  objects and emit segments continuing it; the objects its reading environment holds by
  identity (providers) are written once for the whole stream; live objects created anew
  (states of a fresh parse) are new entries — sharing follows identity, not equality.

## 7. Identity entries, self-contained entries, and the reading environment

A serialized spec or provider is either an **identity** entry — a reference the reading
side resolves against live objects it already holds — or a **self-contained** entry
the reading side rebuilds an equivalent object from. Which one a type uses is that
type's decision, recorded in its `SerializableObject` impl:

- Packages are identity entries (`core.package {name}`): a package is part of the
  reading program's own configuration, and the name is enough. Specs that hold parsers
  (argument parsers, environment behaviors have no serialized form) are identity
  entries through their **provenance stamp** (`core.provider-spec-identity`): the
  stamp records the provider that defined the spec and the key it was defined under,
  and only a package built shared (`Package::new_shared`) hands stamps out; an
  unstamped spec of such a type is a write error (`SerializeError::MissingProvenance`).
  Rationale: [§dd-dr:instance-not-lookup].
- Scopes and fallback providers (definitions made during a parse) are self-contained
  entries whose definitions refer to spec entries; the error spec and the preset's
  simple specs (`\begin`, `\end`, paragraph break, `\input`) are self-contained.

The **reading environment** is what identity entries resolve against: for the crate's
own providers, a `KnownProviders` directory the reading program sets as the session's
user data — the providers it holds, by name (the very instances its own parses use, so
that read data shares them), plus **recipes** that build the ones it does not hold
(the preset registers recipes for `_builtin` and the minilatex packages). A stamped
spec resolves by looking its definition key up in the resolved package (`get` for a
name, `get_specials` for a trigger). What is missing is a typed error naming it
(`MissingProvider`, `MissingDefinition`); nothing is guessed. Reading a heterogeneous
entry dispatches on its identifier through the readers registered on the session (the
preset's `latexlike::serialize::register` registers the crate's and its own) and, for
identifiers no reader covers, through registered resolvers (a framework's own, for its
identifier prefix); an identifier nothing recognizes is `UnknownIdentifier`.

## 8. Compatibility policy

- `version` (`Segment::VERSION`, currently 1) is carried in every segment; a reader
  accepts exactly its own version. **Until the layout is declared final ("v1"),
  breaking changes to the layout are allowed and the version stays 1**; streams written
  before such a change are not preserved.
- At the freeze: the abstract structure + the canonical JSON rendering (§2–§5) become
  the public contract; table names, entry identifiers, key names, and enum strings are
  hard-stable; payload keys grow additively only (`meta` is the extension point for
  segment-wide keys); the compact (binary) rendering stays a same-version pairing;
  language-owned values are the language's stability obligation (`latexlike.*` for the
  preset, its package names included); a layout change bumps `version` and comes with a
  reading policy for older versions (to be decided then — read-old / convert / refuse).
- Identifier stability — for condition identifiers, their projection keys, entry
  identifiers, and table names alike — follows [§dd-dr:wire-identifier-stability]:
  chosen strings owned by whoever defines the vocabulary (`core.*` for the crate,
  `latexlike.*` for the preset, a framework's own prefix for its types), never Rust
  type names.
- Not part of the contract: message wording (diagnostic `message`, frame `title`),
  Rust type names (never on the wire), table ordinals (the directory maps them per
  segment), the memory layout of live objects, the profile strings (the caller's).
