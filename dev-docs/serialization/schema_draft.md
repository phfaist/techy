# Serialized form — draft schema description (M6)

**STATUS: TRANSIENT WORKING DOCUMENT** (M6 acceptance: "a written draft schema description
(input for the v1 freeze)"). Written from the wire structs at `techy-serialize` HEAD by
the M6 implementer (2026-08-17); the structs are the schema (P1), this text describes them.
Every name is PROVISIONAL until the Q3 naming pass; `wire_vocabulary.md` (same folder)
lists each with its anchor and the open questions. After Q3 the surviving text moves to
its permanent home (an ARCHITECTURE section and/or a `docs/` schema page — M7 decides).

Reading guide: §1 the value model and its canonical JSON; §2 the segment (the unit of a
stream) and its table directory; §3 the standard tables and their entries, one subsection
per object kind, each with the abstract structure and a worked example from ONE real
parse (below); §4 the language-owned parts (latexlike); §5 stream conventions; §6 the
compatibility policy placeholder.

**The worked example.** Everything in §3 is cut from one segment: a tolerant latexlike
parse of the text `\e{x} {` (a macro `\e` with one mandatory argument, defined in a shared
package `d`; then an unclosed group), serialized as a parse result with
`session.serialize_parse_result(&Arc::new(result))` into a fresh
`SerdeSession::<Latexlike>::new()`, rendered with `serde_json::to_string`. Line breaks and
indentation are added here for reading; the canonical rendering is compact.

---

## 1. The value model and its canonical JSON rendering

Every serialized thing is a `SerialValue`:

| Variant | Meaning | Canonical JSON |
|---|---|---|
| `Null` | the absent value | `null` |
| `Bool` | | `true` / `false` |
| `Int` (i64) | every integer width; out-of-range values are errors, never truncated | number |
| `Str` | | string |
| `Bytes` | a byte string | `{"$bytes": "<base64>"}` — standard alphabet, `=` padding, no line breaks; strict decoder |
| `List` | | array |
| `Map` | string-keyed, insertion-ordered; keys unique by construction | object, in entry order |
| `Index {table, index}` | a reference to entry `index` of table `table` | `{"$index": [<table ordinal>, <index>]}` — the ordinal is the WRITER's table numbering, translated by the reader through the segment's directory (§2) |

A user map key beginning with `$` is written with one more leading `$` (`"$foo"` →
`"$$foo"`) and unescaped on reading; an object key beginning with `$` that is neither a
reserved form nor `$$`-escaped is a read error. There are no floating-point numbers. Two
values render identically exactly when they compare equal (the canonical-form
discipline, P7). Any other serde format receives the compact rendering (serde's
externally tagged form of the enum; a private same-version pairing, D2).

Value-level shapes reused everywhere below:

- **span** — `{"source": <index into sources>, "start": <int>, "end": <int>}`, byte
  offsets, half-open, validated on reading against the source's text (within bounds, on
  character boundaries).
- **range** — `{"start": <int>, "end": <int>}`, half-open.
- **text content** — `{"spanned": {"start", "end"}}` (a byte range into the carrying
  node's own source; validated on reading) or `{"owned": "<text>"}`; language payloads
  carry owned text only.
- **language-owned values** — a value the language's own conversion produces (a mode,
  an ext, an invocation syntax, a callable type): rendered verbatim where they occur;
  `()` renders `null`; the crate's `Option<T>` conversion renders `None` as `null`.
- **omitted keys** — a wire struct's optional field that is absent is an OMITTED key
  (never `null`); a present `null` reads back as absent too.

## 2. The segment

A stream is a sequence of segments; a segment is what one `SerdeSession::take_segment`
emits and one `push_segment` absorbs. Its rendering:

```json
{
  "version": 1,
  "tables": [
    {"name": "sources",       "id": 0, "start": 0, "entries": [ … ]},
    {"name": "states",        "id": 1, "start": 0, "entries": [ … ]},
    {"name": "specs",         "id": 2, "start": 0, "entries": [ … ]},
    {"name": "providers",     "id": 3, "start": 0, "entries": [ … ]},
    {"name": "trees",         "id": 4, "start": 0, "entries": [ … ]},
    {"name": "diagnostics",   "id": 5, "start": 0, "entries": [ … ]},
    {"name": "parse-results", "id": 6, "start": 0, "entries": [ … ]}
  ]
}
```

- `version` — the layout version, in every segment (`Segment::VERSION` = 1); a reader
  accepts exactly its own version (`DeserializeError::UnsupportedVersion`).
- `tables` — the **table directory**: every table the emitting session has registered,
  in its registration order, whether or not the segment carries entries for it —
  `name` (how a reading session finds its own table, whatever its registration order),
  `id` (the writer's ordinal, which every `$index` inside the segment uses), `start` (the
  position of the part's first entry — the reader checks it continues its table:
  `SegmentOutOfOrder` otherwise), `entries` (in position order).
- An entry of a **homogeneous** table (one kind of object; the table implies the
  identifier) is the bare payload; an entry of a **heterogeneous** table is
  `{"id": "<identifier>", "data": <payload>}`.
- Positions are stream-scoped: a later segment's entries refer to earlier segments'
  entries by position; a reader absorbs one stream's segments in order, each once.

The example segment's directory is exactly the block above (all seven tables present;
this segment has entries in every one of them).

## 3. The standard tables

### 3.1 `sources` (ordinal 0) — homogeneous, identifier `core.source`

Abstract structure (`WireSource`): `origin` (language-owned value; `Option<String>` by
default → string or `null`), `provenance` (`"primary"` | `{"resolved": {"reference",
"triggered_at": span}}` | `{"synthesized": {"description", "triggered_at": span}}` — a
resolved or synthesized source refers to the source its triggering location lies in, so
provenance chains are references between entries of this table, always to earlier
positions), `line_number_offset`, `column_number_offset` (integers), `text` (`{"embedded":
"<text>"}` | `{"referenced": {"length": <int>, "digest"?: {"algorithm": "<name>",
"bytes": <bytes>}}}` — the writer's policy decides per source; a referenced source's text
comes from the reader's supplier, checked against `length` and, when present, `digest`).

Example (entry 0):

```json
{"origin": null, "provenance": "primary", "line_number_offset": 1, "column_number_offset": 1,
 "text": {"embedded": "\\e{x} {"}}
```

A referenced source with a digest renders `"text": {"referenced": {"length": 48210,
"digest": {"algorithm": "sha256", "bytes": {"$bytes": "…"}}}}` (illustrative; the digest algorithm and its verification are the caller's).

### 3.2 `states` (ordinal 1) — homogeneous, identifier `core.state`

Abstract structure (`WireState`): `rules` (the token rules: one section per feature the
language declares present — a section for an absent feature is omitted; a missing section
for a present feature reads as that feature's empty rules — `whitespace {enabled,
chars}`, `paragraphs {enabled}`, `groups {enabled, rules: [group rule…], temporary: [group
rule…], expecting_close?: group rule}`, `commands {enabled, rules: [{escape_char (a
one-character string), name_chars}]}`, `comments {enabled, rules: [{start}]}`, `specials
{enabled}`, `forbidden_chars {chars}`; a group rule is `{group_type: <language-owned>,
open, close}`), `mode` and `ext` (language-owned values), `scopes` (the scope stack:
references into `providers`, outermost first). The derived caches (delimiter prefix table,
specials trigger characters) are never written.

Example (entry 0, the seed state; entry 1 is the state inside `\e`'s argument group — the
same shape with `"expecting_close": {"group_type": "content", "open": "{", "close": "}"}`
added under `groups`):

```json
{"rules": {
   "whitespace": {"enabled": true, "chars": " \t\n\r\u000b\f"},
   "paragraphs": {"enabled": true},
   "groups": {"enabled": true,
              "rules": [{"group_type": "content", "open": "{", "close": "}"},
                        {"group_type": {"math": "inline"}, "open": "$", "close": "$"},
                        {"group_type": {"math": "display"}, "open": "$$", "close": "$$"},
                        {"group_type": {"math": "inline"}, "open": "\\(", "close": "\\)"},
                        {"group_type": {"math": "display"}, "open": "\\[", "close": "\\]"}],
              "temporary": []},
   "commands": {"enabled": true, "rules": [{"escape_char": "\\", "name_chars": "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ"}]},
   "comments": {"enabled": true, "rules": [{"start": "%"}]},
   "specials": {"enabled": true},
   "forbidden_chars": {"chars": ""}},
 "mode": "text", "ext": null,
 "scopes": [{"$index": [3, 0]}, {"$index": [3, 1]}]}
```

### 3.3 `specs` (ordinal 2) — heterogeneous (`{"id", "data"}`)

Entries of the specs table are callable specs, each under its own identifier:

- `core.provider-spec` — a spec named **by identity** through its provenance stamp:
  `{"provider": <index into providers>, "callable_type": <language-owned>, "key":
  {"name": "<registered name>"} | {"trigger": "<specials trigger>"}}`. Read by looking
  the key up in the reading side's package of that name — the very instance that package
  holds. Used by every spec type that holds parsers (core `StdCallableSpec`, latexlike
  `MacroSpec`, `EnvironmentSpec`, `SpecialsSpec`, and stamped `BeginSpec`/`InputMacroSpec`).
- `core.error-spec` — `{"detail"?: "<text>"}` (self-contained).
- language-owned self-contained forms — `latexlike.begin {end_command_name}`,
  `latexlike.end {}`, `latexlike.paragraph-break {}`, `latexlike.input {persist_state,
  attached_slot_ext}` (§4).
- a framework's own identifiers, read through the readers/resolvers it registers.

Example (entry 0 — `\e`, defined in package `d`, which is providers entry 1):

```json
{"id": "core.provider-spec",
 "data": {"provider": {"$index": [3, 1]}, "callable_type": "macro", "key": {"name": "e"}}}
```

### 3.4 `providers` (ordinal 3) — heterogeneous

- `core.package` — `{"name": "<package name>"}` (identity: resolved through the reading
  program's `KnownProviders` — a held provider of that name, else a registered recipe).
- `core.scope` — `{"name", "definitions": [{"callable_type", "name", "spec": <index into
  specs>}…]}` (in full; definitions ordered by callable type, then name).
- `core.fallback-provider` — `{"name", "fallbacks": [{"callable_type", "spec"}…]}`.

Example (entries 0 and 1 — the seed package and `d`):

```json
{"id": "core.package", "data": {"name": "_builtin"}}
{"id": "core.package", "data": {"name": "d"}}
```

### 3.5 `trees` (ordinal 4) — heterogeneous by annotation type; `core.tree` = the unit annotation

Abstract structure (`WireTree`): `nodes` — every node in storage order (the root first,
then each node's children as one contiguous block; the list is EXACTLY the set reachable
from the root), each `{"kind", "span", "state": <index into states>, "ext": <language-
owned>, "children": range}` where `children` is a storage-index range and `kind` is one
of `"list"`, `{"chars": {"content": text}}`, `{"group": {"group_type": <language-owned>,
"open": text, "close": text}}`, `{"comment": {"start": text, "content": text, "post_space":
text}}`, `{"callable": {"callable_type": <language-owned>, "name", "spec": <index into
specs>, "arguments": [argument…], "slots": [slot…], "invocation_syntax": <language-
owned>}}`; `annotations` — omitted for the unit annotation, else one language- or
caller-owned value per node (registered with `TableHandle::register_annotation`).

An **argument** is `{"region"?: region, "ext"?: <language-owned>, "spec_payload"?: <spec-
owned>}` — a provided argument carries `region` and its `ext` (written as `null` for the
unit ext; a reader accepts a `null` value and an omitted key alike), an absent argument
is `{}`; `spec_payload` is present only when the callable spec serializes an argument
spec out of band (D21). A
**slot** is `{"name"?, "region", "role": "content"|"attached"|"hidden", "ext": <language-
owned>}`. A **region** is `{"children": range, "content": range, "content_parent":
<storage index>}` — `children` are offsets into the callable's own child list;
`content_parent` is the storage index of the node whose children hold the content: the
callable itself (then `content` are offsets within the region's own node list) or a node
inside the region (then offsets within that node's children). Tree layout tags and the
parent table are never written; the reader rebuilds through the node builder.

Example (entry 0 — the whole tree; six nodes: the root list, `\e`, a chars ` `, the
unclosed group `{` (its `close` recovered as owned `""`), then `\e`'s argument group and
its chars `x`; the argument's region names node 4, the group, as its content parent):

```json
{"id": "core.tree", "data": {"nodes": [
  {"kind": "list", "span": {"source": {"$index": [0, 0]}, "start": 0, "end": 7},
   "state": {"$index": [1, 0]}, "ext": null, "children": {"start": 1, "end": 4}},
  {"kind": {"callable": {"callable_type": "macro", "name": "e", "spec": {"$index": [2, 0]},
     "arguments": [{"region": {"children": {"start": 0, "end": 1}, "content": {"start": 0, "end": 1}, "content_parent": 4}, "ext": null}],
     "slots": [],
     "invocation_syntax": {"macro": {"escape_char": "\\", "post_space": {"owned": ""}}}}},
   "span": {"source": {"$index": [0, 0]}, "start": 0, "end": 5},
   "state": {"$index": [1, 0]}, "ext": null, "children": {"start": 4, "end": 5}},
  {"kind": {"chars": {"content": {"spanned": {"start": 5, "end": 6}}}},
   "span": {"source": {"$index": [0, 0]}, "start": 5, "end": 6},
   "state": {"$index": [1, 0]}, "ext": null, "children": {"start": 5, "end": 5}},
  {"kind": {"group": {"group_type": "content", "open": {"spanned": {"start": 6, "end": 7}}, "close": {"owned": ""}}},
   "span": {"source": {"$index": [0, 0]}, "start": 6, "end": 7},
   "state": {"$index": [1, 0]}, "ext": null, "children": {"start": 5, "end": 5}},
  {"kind": {"group": {"group_type": "content", "open": {"spanned": {"start": 2, "end": 3}}, "close": {"spanned": {"start": 4, "end": 5}}}},
   "span": {"source": {"$index": [0, 0]}, "start": 2, "end": 5},
   "state": {"$index": [1, 0]}, "ext": null, "children": {"start": 5, "end": 6}},
  {"kind": {"chars": {"content": {"spanned": {"start": 3, "end": 4}}}},
   "span": {"source": {"$index": [0, 0]}, "start": 3, "end": 4},
   "state": {"$index": [1, 1]}, "ext": null, "children": {"start": 6, "end": 6}}
]}}
```

(The `\e` argument's `"ext": null`: the region is present and the latexlike argument
ext is `()`, so the key is written with `null`; an absent argument writes neither
`region` nor `ext`. The M4 progress-log line "omits the wire key" describes the reader's
tolerance — an omitted key and a `null` both read as the unit ext — not the writer.)

### 3.6 `diagnostics` (ordinal 5) — homogeneous, identifier `core.diagnostic`

Abstract structure (`WireDiagnostic`): `severity` (`"note"` | `"warning"` | `"error"`),
`identifier` (the condition's wire identity, e.g. `core.groups.unclosed-group`), `data`
(the condition's `serializable_data()` projection — a `DiagnosticValue`: `null`,
booleans, integers, strings, lists, string-keyed maps; never a byte string or a table
position), `message` (the human message as rendered when the diagnostic was written),
`span`, `frames` (the traceback snapshot, innermost first, each `{"title": "<rendered
frame title>", "span": span}`). Read back as a diagnostic whose condition is a
`DeserializedCondition` holding those values: same identifier, projection, and message;
no downcast to the original condition type.

Example (entry 0 — the unclosed group; a top-level group carries no traceback frame):

```json
{"severity": "error", "identifier": "core.groups.unclosed-group",
 "data": {"expected_close": "}", "found": "end-of-input"},
 "message": "unclosed group: expected ‘}’ before end of input",
 "span": {"source": {"$index": [0, 0]}, "start": 6, "end": 7},
 "frames": []}
```

A diagnostic with frames (from `\emph{x`, latexlike, tolerant): `"frames": [{"title":
"argument #1 of macro ‘\\emph’", "span": {…, "start": 5, "end": 5}}, {"title": "macro
‘\\emph’", "span": {…, "start": 0, "end": 5}}]`.

### 3.7 `parse-results` (ordinal 6) — homogeneous, identifier `core.parse-result`

Abstract structure (`WireParseResult`): `tree` (index into `trees`; a `core.tree` entry),
`diagnostics` (`{"items": [index into diagnostics…] in recording order, "limit": <int>,
"suppressed": <int>, "error_count": <int>}` — the collection's retention cap and counts,
cross-checked on reading: `items ≤ limit`, `suppressed > 0` only when `items == limit`,
`retained errors ≤ error_count ≤ retained errors + suppressed`), `session_ext` (language-
owned value; `null` for `()`). A parse result is interned by identity (its `Arc`) and read
back as the shared `Arc<ParseResult>`.

Example (entry 0):

```json
{"tree": {"$index": [4, 0]},
 "diagnostics": {"items": [{"$index": [5, 0]}], "limit": 1000, "suppressed": 0, "error_count": 1},
 "session_ext": null}
```

## 4. Language-owned parts (latexlike, as the template of what a language supplies)

A language declares itself serializable (`SerializableLang`) by supplying value
conversions for every type it hands the parse; those values render verbatim inside the
core structures above:

| Slot | latexlike value | Rendering |
|---|---|---|
| callable type (`CallableTypeId`) | `CallableType` | `"macro"` / `"environment"` / `"specials"` |
| group type (`GroupTypeId`) | `GroupType` | `"content"` / `{"math": "inline"|"display"}` / `"verbatim"` |
| mode (`ModeId`) | `Mode` | `"text"` / `"math"` |
| event | `Event` | `"exit-math-context"` |
| state ext, node ext, argument ext | `()` | `null` |
| slot ext | `BodyMarker` | `{"body": true|false}` |
| session ext | `()` | `null` |
| source origin | `Option<String>` | string or `null` |
| invocation syntax | `InvocationSyntaxData` | `{"macro": {"escape_char": "\\", "post_space": text}}` / `{"environment": {"begin": side, "end"?: side}}` with side = `{"escape_char", "command_word": text, "post_space": text, "name_group_rule": {"group_type", "open", "close"}}` / `"specials"` |

Its spec forms: identity (`core.provider-spec`) for stamped `MacroSpec`, `SpecialsSpec`,
`EnvironmentSpec`, `BeginSpec`, `InputMacroSpec`; self-contained `latexlike.begin
{"end_command_name"}`, `latexlike.end {}`, `latexlike.paragraph-break {}`, `latexlike.input
{"persist_state": bool, "attached_slot_ext": <slot ext>}`. Its providers: `core.package`
by name (`_builtin`, `minilatex`, `minilatex.item`), resolved by the reading side's
`KnownProviders` (the language's `register_package_recipes` helpers add recipes for them).
Its condition identifiers: `latexlike.environments.{malformed-begin, orphan-end,
unknown-environment}`.

## 5. Stream conventions

- **JSON Lines** (canonical stream rendering): one segment per line
  (`serde_json::to_string(&segment)` — the compact rendering contains no raw line
  break), lines in emission order; a reader decodes each line
  (`serde_json::from_str::<Segment>`) and pushes it in order. Every line is an
  independently valid segment (its own `version` and full directory) but positions are
  stream-scoped (a line pushed without its predecessors is `SegmentOutOfOrder`). No
  end-of-stream marker: the stream ends where the input ends; appending = appending
  lines; a truncated last line loses only itself. (Tests: `techy/tests/serialize_stream.rs`.)
- **Other formats**: one segment per framed value (postcard with a length prefix, a
  file per segment, a message per segment), same ordering rules; the compact rendering
  is a private same-version pairing.
- **Reading then appending**: a session that absorbed a stream may intern further
  objects and emit segments continuing it; the objects its reading environment holds by
  identity (providers) are written once for the whole stream; live objects created anew
  (states of a fresh parse) are new entries — sharing follows identity, not equality.
- **Stream identity** is the caller's obligation (Q6): the segments pushed into one
  session must belong to one stream, in order.

## 6. Compatibility policy (placeholder for the freeze)

- `version` (`Segment::VERSION`, currently 1) is carried in every segment; a reader
  accepts exactly its own version. Until the freeze, breaking changes to the layout are
  allowed and the version stays 1 (pre-release streams are not preserved).
- At the freeze: the abstract structure + the canonical JSON rendering (§1–§4) become
  the public contract; table names, entry identifiers, key names, and enum strings are
  hard-stable; payload keys grow additively only; the compact (binary) rendering stays a
  same-version pairing; language-owned values are the language's stability obligation
  (`latexlike.*` for the preset); a layout change bumps `version` and comes with a
  reading policy for older versions (to be decided then — read-old / convert / refuse).
- Not part of the contract: message wording (diagnostic `message`, frame `title`),
  Rust type names (never on the wire), table ordinals (the directory maps them per
  segment), the memory layout of live objects.
