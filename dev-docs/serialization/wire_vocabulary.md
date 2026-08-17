# Wire vocabulary inventory — input for the Q3 naming pass (M6)

**STATUS: TRANSIENT WORKING DOCUMENT** (deleted at M7, or its surviving content promoted
into the schema description). Prepared by the M6 implementer (2026-08-17) from the wire
structs at `techy-serialize` HEAD; every string below is what the code emits **today**
(provisional). Line numbers are the anchors at the time of writing.

How to read it: for each wire-visible name — table names, entry identifiers, key names,
enum strings, reserved JSON forms — the current string, where it is defined, and, where a
change seems worth considering, a proposal with a one-line reason. **OPEN** marks a
question for the user. Nothing here is decided; the code changes only after the user's
Q3 rulings (the rename pass follows this document).

Vocabulary conventions in force today (worth ruling on as a whole, §16 item 1):

- **Table names** — lowercase plural nouns; the one two-word name is kebab-cased
  (`parse-results`).
- **Entry identifiers** — `<owner>.<kind>` (`core.source`, `latexlike.begin`), kebab-cased
  kinds; two segments, unlike condition identifiers' three (`core.groups.unclosed-group`,
  [§dd-dr:wire-identifier-stability]).
- **Key names** — snake_case (`line_number_offset`, `callable_type`), matching the Rust
  field names they came from (chosen explicitly through `#[serial(name = …)]`, never by
  accident, but no key currently differs from its Rust name).
- **Enum strings** (unit variants, forms) — kebab-case (`end-of-input`,
  `exit-math-context`; single words otherwise: `primary`, `resolved`, `macro`).
- **Reserved JSON keys** — `$`-prefixed (`$index`, `$bytes`), `$$` escaping for user keys.

---

## 1. Segment envelope and entry forms

Defined in `techy/src/serialize/engine/segment.rs` (`Segment` :51, `SegmentTable` :61,
`WireEntry` :75; `Segment::VERSION` :85).

| Wire name | Current | Where | Meaning | Proposal / note |
|---|---|---|---|---|
| segment key | `version` | segment.rs:53 | `Segment::VERSION` (= 1), in every segment (Q6) | keep |
| segment key | `tables` | segment.rs:55 | the table directory, registration order | keep |
| directory key | `name` | segment.rs:63 | table name | keep |
| directory key | `id` | segment.rs:65 | the WRITER's `TableId` ordinal (an integer) | **OPEN** — see §11 (`TableId` naming; if the `Index` rendering moves to table names, `id` becomes redundant) |
| directory key | `start` | segment.rs:67 | position of the part's first entry | keep (`first`? no: `start` matches the range vocabulary elsewhere) |
| directory key | `entries` | segment.rs:69 | the entries in position order | keep |
| entry key (heterogeneous) | `id` | segment.rs:77 | the entry's identifier | **proposal**: `identifier` — everywhere else the concept is called "identifier" (`Diagnostic::identifier`, the diagnostic entry's own `identifier` key, `UnknownIdentifier`); `id` collides in spirit with the directory's `id` (a table ordinal) — one word, two meanings, side by side in one segment |
| entry key (heterogeneous) | `data` | segment.rs:79 | the payload | keep (mirrors `SerialEntry::data`) |
| homogeneous entry | (bare data) | session.rs (write path) | no envelope | keep |

## 2. Table names and ordinals (`SerdeSession::new()`)

Defined in `techy/src/serialize/drivers/mod.rs` :47-59 and :63-71 (`STANDARD_TABLE_NAMES`);
registered in `drivers/standard.rs` (`with_source_driver`).

| Ordinal | Current | Driver | Kind | Proposal / note |
|---|---|---|---|---|
| 0 | `sources` | `SourceSerdeDriver` | homogeneous, `core.source` | keep |
| 1 | `states` | `StateSerdeDriver` | homogeneous, `core.state` | keep |
| 2 | `specs` | `SpecSerdeDriver` | heterogeneous | keep |
| 3 | `providers` | `ProviderSerdeDriver` | heterogeneous | keep |
| 4 | `trees` | `TreeSerdeDriver` | heterogeneous by annotation type | keep |
| 5 | `diagnostics` | `DiagnosticSerdeDriver` | homogeneous, `core.diagnostic` | keep |
| 6 | `parse-results` | `ParseResultSerdeDriver` | homogeneous, `core.parse-result` | **OPEN** kebab vs snake for the one multi-word table name (`parse-results` matches identifier segments; `parse_results` matches keys). Recommendation: kebab — table names and identifiers are "vocabulary strings" chosen like identifiers, keys are struct fields |

The ordinal is what a table reference (`$index`) names in the canonical JSON today; a
reader matches tables by NAME through the directory and never relies on the ordinal (§11).

## 3. Entry identifiers

| Table | Current | Where | Payload (§ below) | Proposal / note |
|---|---|---|---|---|
| sources | `core.source` | drivers/mod.rs:88 | §4 | keep |
| states | `core.state` | drivers/mod.rs:90 | §5 | keep |
| trees | `core.tree` (unit annotation) | drivers/mod.rs:93 | §7 | keep; other annotation types are registered under caller-chosen identifiers (`TableHandle::register_annotation`) |
| diagnostics | `core.diagnostic` | drivers/mod.rs:95 | §8 | keep |
| parse-results | `core.parse-result` | drivers/mod.rs:97 | §9 | keep (kebab inside the identifier, as `core.fallback-provider`) |
| specs | `core.provider-spec` | drivers/mod.rs:100 | §6.1 identity form | **proposal**: `core.spec-identity` (M5 reviewer) — the entry is not "a provider's spec" but "a spec named by identity through its provider"; alternative `core.spec-by-identity`. Both read better than `provider-spec` next to `core.package` |
| specs | `core.error-spec` | drivers/mod.rs:102 | §6.5 | keep (the type is `ErrorCallableSpec`; `core.error-callable-spec` is longer for no gain) |
| providers | `core.package` | drivers/mod.rs:104 | §6.2 | keep |
| providers | `core.scope` | drivers/mod.rs:106 | §6.3 | keep |
| providers | `core.fallback-provider` | drivers/mod.rs:108 | §6.4 | keep |
| specs | `latexlike.begin` | latexlike/serialize.rs:473 | §10.3 | keep (self-contained `BeginSpec`) |
| specs | `latexlike.end` | latexlike/serialize.rs:475 | §10.3 | keep |
| specs | `latexlike.paragraph-break` | latexlike/serialize.rs:477 | §10.3 | keep |
| specs | `latexlike.input` | latexlike/serialize.rs:479 | §10.3 | keep (`latexlike.input-macro`? the callable is `\input`; `input` alone reads fine under `latexlike.`) |
| (adapter condition) | `core.serialization.deserialized-condition` | drivers/diagnostic.rs (`DeserializedCondition::IDENTIFIER`) | not an entry — the adapter type's own condition identifier (three-part scheme) | **OPEN**: area name `serialization` vs `serialize` (the module) — the scheme wants a concept, not a module: `serialization` |

**Scheme question (OPEN, §16 item 1):** entry identifiers are two-part (`core.<kind>`),
condition identifiers three-part (`core.<area>.<condition>`). Both are "identifiers" in
the [§dd-dr:wire-identifier-stability] sense (hard-stable strings, owner-first). Options:
(a) keep two-part for entries (the table is the "area"); (b) three-part with a fixed
middle segment for entry kinds, e.g. `core.entry.source`? (nothing gained); (c) two-part
everywhere is impossible (conditions need the area). Recommendation: (a), stated
explicitly in the schema description.

## 4. Sources (`core.source`) — `techy/src/serialize/wire/source.rs`

| Key / string | Current | Where | Value | Proposal / note |
|---|---|---|---|---|
| key | `origin` | source.rs:18 | `SourceOrigin`'s value conversion (`Option<String>` default: `null` or string) | keep |
| key | `provenance` | source.rs:21 | see below | keep |
| key | `line_number_offset` | source.rs:24 | integer | keep (mirrors `Source::line_number_offset`) |
| key | `column_number_offset` | source.rs:27 | integer | keep |
| key | `text` | source.rs:30 | `{embedded: str}` or `{referenced: {…}}` | keep |
| text form | `embedded` | source.rs:40 | the text | keep |
| text form | `referenced` | source.rs:43 | `{length, digest?}` | keep |
| key | `length` | source.rs:51 | integer (bytes) | keep |
| key | `digest` | source.rs:54 | `{algorithm, bytes}`; **omitted** when none | keep |
| key | `algorithm` | source.rs:62 | string (caller's hash name, e.g. `sha256`) | keep |
| key | `bytes` | source.rs:65 | `$bytes` (base64) | keep |
| provenance form | `primary` | source.rs:73 | bare string | keep |
| provenance form | `resolved` | source.rs:76 | `{reference, triggered_at}` | keep |
| provenance form | `synthesized` | source.rs:86 | `{description, triggered_at}` | keep |
| key | `reference` | source.rs:80 | string (the reference that was resolved) | keep |
| key | `description` | source.rs:90 | string | keep |
| key | `triggered_at` | source.rs:83/:93 | a span (below) | keep |
| span key | `source` | source.rs:101 | `$index` into `sources` | keep |
| span key | `start` | source.rs:104 | byte offset (inclusive) | keep |
| span key | `end` | source.rs:107 | byte offset (exclusive) | keep |

`Option` note: `origin` is a verbatim `SerialValue` (the language's conversion) → `None`
renders `null`; `digest` is a derive `Option` → **omitted** (§13).

## 5. States (`core.state`) — `techy/src/serialize/wire/state.rs`

| Key / string | Current | Where | Value | Proposal / note |
|---|---|---|---|---|
| key | `rules` | state.rs:24 | the token rules (sections below) | keep |
| key | `mode` | state.rs:27 | `ModeId`'s value conversion (`null` for `()`; latexlike `text`/`math`) | keep |
| key | `ext` | state.rs:31 | `StateExt`'s value conversion (`null` for `()`) | keep |
| key | `scopes` | state.rs:34 | list of `$index` into `providers`, outermost first | keep |
| section | `whitespace` | state.rs:42 | `{enabled, chars}` — **omitted** when the feature is absent | keep |
| section | `paragraphs` | state.rs:45 | `{enabled}` | keep |
| section | `groups` | state.rs:48 | `{enabled, rules, temporary, expecting_close?}` | keep |
| section | `commands` | state.rs:51 | `{enabled, rules}` | keep |
| section | `comments` | state.rs:54 | `{enabled, rules}` | keep |
| section | `specials` | state.rs:57 | `{enabled}` | keep |
| section | `forbidden_chars` | state.rs:60 | `{chars}` | keep |
| key | `enabled` | state.rs:68/79/87/119/141/160 | bool | keep |
| key | `chars` | state.rs:71/168 | string | keep |
| key | `rules` | state.rs:90/122/144 | list of rules (nested `rules` inside `rules` — see note) | **note**: `rules.groups.rules` reads oddly (the outer `rules` = token rules, the inner = the group rules list). Alternative outer name: `token_rules`; alternative inner: `delimiters`/`items`. Recommendation: rename the OUTER key to `token_rules` (it IS `TokenRules`), keep the inner `rules` |
| key | `temporary` | state.rs:93 | list of group rules | keep |
| key | `expecting_close` | state.rs:96 | a group rule; **omitted** when none | keep |
| group rule key | `group_type` | state.rs:105 | `GroupTypeId`'s value conversion | keep |
| group rule key | `open` | state.rs:108 | string | keep |
| group rule key | `close` | state.rs:111 | string | keep |
| command rule key | `escape_char` | state.rs:130 | one-character string | keep |
| command rule key | `name_chars` | state.rs:133 | string | keep |
| comment rule key | `start` | state.rs:152 | string | keep |

## 6. Specs and providers — `techy/src/serialize/wire/specs.rs`

### 6.1 `core.provider-spec` (spec identity through the provenance stamp) — `WireSpecIdentity` :78

| Key / string | Current | Where | Value | Proposal / note |
|---|---|---|---|---|
| key | `provider` | specs.rs:80 | `$index` into `providers` | keep |
| key | `callable_type` | specs.rs:83 | `CallableTypeId`'s value conversion | keep |
| key | `key` | specs.rs:86 | `{name: str}` or `{trigger: str}` | **OPEN** (M5 reviewer): the shape `key: {name: …} | {trigger: …}` — keep the tagged form (a name and a trigger are different keys, D19 refinement), or flatten to two optional keys? Recommendation: keep tagged; consider renaming the key `key` → `definition` (`DefinitionKey`) to avoid the "key: {name}" doubling |
| definition-key form | `name` | specs.rs:94 | string | keep |
| definition-key form | `trigger` | specs.rs:97 | string | keep |

### 6.2 `core.package` — `WirePackage` :21

| key | `name` | specs.rs:23 | string | keep |
|---|---|---|---|---|

### 6.3 `core.scope` — `WireScope` :29, `WireDefinition` :40

| Key | Current | Where | Value |
|---|---|---|---|
| key | `name` | specs.rs:31 | string |
| key | `definitions` | specs.rs:34 | list of `{callable_type, name, spec}` (BTreeMap order: callable type, then name) |
| definition key | `callable_type` | specs.rs:43 | value conversion |
| definition key | `name` | specs.rs:46 | string |
| definition key | `spec` | specs.rs:49 | `$index` into `specs` |

### 6.4 `core.fallback-provider` — `WireFallbackProvider` :55, `WireFallback` :66

| key | `name` | specs.rs:57 | string |
|---|---|---|---|
| key | `fallbacks` | specs.rs:60 | list of `{callable_type, spec}` |
| fallback key | `callable_type` | specs.rs:68 | value conversion |
| fallback key | `spec` | specs.rs:71 | `$index` into `specs` |

### 6.5 `core.error-spec` — `WireErrorSpec` :103

| key | `detail` | specs.rs:105 | string; **omitted** when none |
|---|---|---|---|

## 7. Trees (`core.tree` and registered annotation identifiers) — `techy/src/serialize/wire/tree.rs`

| Key / string | Current | Where | Value | Proposal / note |
|---|---|---|---|---|
| tree key | `nodes` | tree.rs:131 | list of nodes, storage order (root first) | keep |
| tree key | `annotations` | tree.rs:135 | list, one per node; **omitted** for the unit annotation | keep |
| node key | `kind` | tree.rs:143 | `"list"` or `{chars: …}` / `{group: …}` / `{callable: …}` / `{comment: …}` | keep |
| node key | `span` | tree.rs:146 | span (§4) | keep |
| node key | `state` | tree.rs:149 | `$index` into `states` | keep |
| node key | `ext` | tree.rs:152 | `NodeExt`'s value conversion (`null` for `()`) | keep |
| node key | `children` | tree.rs:155 | `{start, end}` storage-index range | keep |
| kind | `chars` | tree.rs:164 | `{content}` | keep |
| kind | `group` | tree.rs:171 | `{group_type, open, close}` | keep |
| kind | `callable` | tree.rs:185 | `{callable_type, name, spec, arguments, slots, invocation_syntax}` | keep |
| kind | `comment` | tree.rs:209 | `{start, content, post_space}` | keep |
| kind | `list` | tree.rs:222 | bare string | keep |
| chars key | `content` | tree.rs:167 | text content (§7.1) | keep |
| group key | `group_type` | tree.rs:175 | value conversion (`null` for an untyped group) | keep |
| group key | `open` / `close` | tree.rs:178/181 | text content | keep |
| callable key | `callable_type` | tree.rs:189 | value conversion | keep |
| callable key | `name` | tree.rs:192 | string | keep |
| callable key | `spec` | tree.rs:195 | `$index` into `specs` | keep |
| callable key | `arguments` | tree.rs:198 | list of arguments | keep |
| callable key | `slots` | tree.rs:201 | list of slots | keep |
| callable key | `invocation_syntax` | tree.rs:205 | `InvocationSyntax`'s value conversion (§10.2 for latexlike) | keep |
| comment key | `start` / `content` / `post_space` | tree.rs:212-218 | text content | keep |
| argument key | `region` | tree.rs:233 | region; **omitted** for an absent argument | keep |
| argument key | `ext` | tree.rs:237 | `ArgumentExt`'s value conversion; written for a provided argument (`null` for the unit ext), **omitted** for an absent argument (which is `{}`); the reader accepts an omitted key or `null` alike | **note**: the M4 log's "omits the key when null" describes the reader's tolerance, not the writer (verified on the pinned renderings: `"ext":null` is written) |
| argument key | `spec_payload` | tree.rs:242 | the callable spec's out-of-band argument-spec description; **omitted** under the index rule | keep |
| slot key | `name` | tree.rs:250 | string; **omitted** for an unnamed slot | keep |
| slot key | `region` | tree.rs:253 | region | keep |
| slot key | `role` | tree.rs:256 | `content` / `attached` / `hidden` | keep |
| slot key | `ext` | tree.rs:259 | `SlotExt`'s value conversion (`{body: bool}` in latexlike) | keep |
| region key | `children` | tree.rs:278 | `{start, end}` offsets into the callable's child list | keep |
| region key | `content` | tree.rs:281 | `{start, end}` offsets (see the discriminator, §11) | keep |
| region key | `content_parent` | tree.rs:284 | storage index of the node whose children hold the content | see §11 (content-frame discriminator) |
| range key | `start` / `end` | tree.rs:292/295 | integers, half-open | keep |

### 7.1 Context-free core value shapes (used inside trees; `wire/tree.rs` :47-118)

| Type | Current | Where | Proposal / note |
|---|---|---|---|
| `Span` | `{start, end}` | tree.rs:47 | keep (crate-internal on the wire — appears only inside `spanned`) |
| `TextContent` | `{spanned: {start, end}}` / `{owned: "text"}` | tree.rs:72 | keep; owned-only inside language payloads (D23) |
| `SlotRole` | `content` / `attached` / `hidden` | tree.rs:98 | keep |
| `GroupRule` (value) | `{group_type, open, close}` | drivers/tree.rs:1083 | keep (same shape as the state's group rules) |
| `SourceSpan` (value) | `{source, start, end}` | drivers/source.rs:401 | keep |

## 8. Diagnostics (`core.diagnostic`) — `techy/src/serialize/wire/diagnostic.rs`

| Key / string | Current | Where | Value | Proposal / note |
|---|---|---|---|---|
| key | `severity` | diagnostic.rs:23 | `note` / `warning` / `error` | keep (matches `Severity`'s `Display`) |
| key | `identifier` | diagnostic.rs:26 | the condition's wire identity | keep — and see §1 (`id` → `identifier` for entries, for consistency with this key) |
| key | `data` | diagnostic.rs:31 | the `serializable_data()` projection (a `DiagnosticValue`; §12 lists condition payload keys) | keep (`SerialEntry::data`, `Diagnostic::data()` — the same word) |
| key | `message` | diagnostic.rs:35 | the rendered `Display` at write time | **OPEN (supervisor's addition, not in D26)**: keep the message on the wire? Cost: bytes per diagnostic; benefit: a diagnostic read back can render without the condition type (the projection alone has no wording). Recommendation: keep; document "not stable wording" |
| key | `span` | diagnostic.rs:38 | span | keep |
| key | `frames` | diagnostic.rs:41 | list of `{title, span}`, innermost first | keep |
| frame key | `title` | diagnostic.rs:84 | rendered frame title (`group ‘{’`, `argument #1 of macro ‘\emph’`) | keep; note the wording is presentation (not stable) |
| frame key | `span` | diagnostic.rs:87 | span | keep |
| severity string | `note` / `warning` / `error` | diagnostic.rs:46-57 | | keep |

Field order (canonical rendering): `severity, identifier, data, message, span, frames`.
**OPEN**: move `message` after `data`→ before? Cosmetic; `identifier, severity, message,
data, span, frames` reads more like a report — a free choice before freeze.

## 9. Parse results (`core.parse-result`) — `techy/src/serialize/wire/parse_result.rs`

| Key | Current | Where | Value | Proposal / note |
|---|---|---|---|---|
| key | `tree` | parse_result.rs:21 | `$index` into `trees` (must be a `core.tree` entry) | keep |
| key | `diagnostics` | parse_result.rs:24 | `{items, limit, suppressed, error_count}` — a NESTED collection object (mirrors `Diagnostics`, P1) | **OPEN**: the brief sketched a flat form (`diagnostics: [..], limit, suppressed, error_count`); nested was chosen because it mirrors the live `Diagnostics` struct and is reusable should a collection ever be serialized alone |
| key | `session_ext` | parse_result.rs:28 | `SessionExt`'s value conversion (`null` for `()`) | keep |
| collection key | `items` | parse_result.rs:38 | list of `$index` into `diagnostics`, recording order | keep (`retained`? no — `items` is the field name and the neutral word) |
| collection key | `limit` | parse_result.rs:41 | integer (`Diagnostics::limit`) | keep |
| collection key | `suppressed` | parse_result.rs:44 | integer | keep |
| collection key | `error_count` | parse_result.rs:47 | integer | keep |

## 10. The latexlike vocabulary — `techy/src/latexlike/serialize.rs` (+ serde renames in `latexlike/mod.rs`)

### 10.1 Closed vocabulary values (`names` module :130-155; serde renames pinned equal by test)

| Type | Strings | Rust variants | Note |
|---|---|---|---|
| `CallableType` | `macro`, `environment`, `specials` | Macro, Environment, Specials | keep |
| `GroupType` | `content`, `{math: <form>}`, `verbatim` | Content, Math(form), Verbatim | keep |
| `MathGroupForm` | `inline`, `display` | Inline, Display | keep |
| `Mode` | `text`, `math` | Text, Math | keep |
| `Event` | `exit-math-context` | ExitMathContext | keep |
| `BodyMarker` (slot ext) | `{body: bool}` | | keep |

### 10.2 The invocation syntax (`InvocationSyntaxData`; :327-470)

| Form | Current | Where | Note |
|---|---|---|---|
| macro | `{macro: {escape_char, post_space}}` | serialize.rs:327 (`WireMacroSyntax`) | `post_space` is a `TextContent` — owned on the wire (`{owned: " "}`) |
| environment | `{environment: {begin, end?}}` | serialize.rs:385 (`WireEnvironmentSyntax`) | `end` **omitted** when none |
| specials | `specials` (bare string) | serialize.rs:351 | |
| side syntax | `{escape_char, command_word, post_space, name_group_rule}` | serialize.rs:424 (`WireEnvironmentSideSyntax`) | `command_word`/`post_space` owned text; `name_group_rule` = a group rule inlined |

### 10.3 Self-contained spec forms

| Identifier | Payload | Where |
|---|---|---|
| `latexlike.begin` | `{end_command_name}` | serialize.rs:516 |
| `latexlike.end` | `{}` | serialize.rs (unit recipe) |
| `latexlike.paragraph-break` | `{}` | serialize.rs (unit recipe) |
| `latexlike.input` | `{persist_state, attached_slot_ext}` | serialize.rs:597 |

Stamped `MacroSpec`/`SpecialsSpec`/`EnvironmentSpec`/`BeginSpec`/`InputMacroSpec` and core
`StdCallableSpec` all use `core.provider-spec` (§6.1).

### 10.4 Provider names that appear as data (`core.package {name}`)

`_builtin` (the seed package; the leading underscore marks it as not user-facing),
`minilatex`, `minilatex.item`. **OPEN**: are package NAMES part of the frozen vocabulary?
They are data (`KnownProviders` keys) but a stream from an older techy names them; the
schema description should say "package names are the language's own vocabulary and their
stability obligation".

## 11. Structural conventions with a naming/rendering question

1. **`Index` rendering — ordinal vs table name (Q3).** Today `{"$index": [<table ordinal>,
   <position>]}` (render.rs:68, `INDEX_KEY`; the ordinal is the WRITER's `TableId`,
   translated by the reader through the segment directory). Alternative:
   `{"$index": ["specs", 12]}` (name string). Costs/benefits: the ordinal is compact and
   the directory already maps it; the name is self-describing when an entry is quoted
   alone (use case 4/5, golden files and inspection dumps) and would make the
   directory's `id` redundant. Binary formats are unaffected (`(u32, u32)` pair either
   way; a name string would need interning to stay compact). Recommendation: **OPEN** —
   ordinal is what M1–M6 pinned; a name rendering is a rendering-only change (the
   in-memory `TableId` stays) but touches the bridge's index sentinel and every pinned
   test.
2. **`TableId` naming tension** (M2 review): §3.G says `…Id` = process-local identity, yet
   a `TableId` travels on the wire inside `SerialValue::Index` and as `SegmentTable::id`.
   Options: rename the type (`TableOrdinal`? `TableNumber`?) or accept the tension with a
   doc sentence (it IS session-local identity that the directory translates — a reader
   never trusts a writer's `TableId` as its own). Recommendation: keep `TableId`, document
   the translation; rename the wire key `id` → `table` or `ordinal` if the ordinal stays.
3. **Region content-frame discriminator** (M4 review): a region's `content` offsets are
   read within the region's own node list when `content_parent == the callable's own
   storage index`, otherwise within `content_parent`'s children (`WireRegion` docs,
   tree.rs:262-274). Implicit today. Alternative: an explicit tag, e.g. `content: {in:
   "region", start, end}` / `{in: "node", node: 7, start, end}`, or two variants
   `{"in_region": {start, end}}` / `{"in_children_of": {node, start, end}}` (mirroring
   the live `ContentNodes::{InRegion, InChildrenOf}`). Cost: a few bytes per region;
   benefit: no arithmetic identity to explain in the schema. Recommendation: the explicit
   two-variant form (P1: the wire mirrors the live enum) — **OPEN**.
4. **Reserved JSON forms** (render.rs): `{"$bytes": "<base64>"}` — standard alphabet
   `A–Z a–z 0–9 + /`, `=` padding, no line breaks, strict decoder (base64.rs:1-7);
   `{"$index": [t, i]}`; a user key beginning with `$` is written `$$…` and unescaped on
   read; any other `$`-key is an error. Keep. **OPEN**: is base64 STANDARD (vs URL-safe)
   the final choice? Standard is what most JSON tooling expects; keep.
5. **Compact (binary) rendering names** (render.rs `COMPACT_VARIANTS` :50, sentinels
   bridge.rs:73 `techy::serialize::Index`, render.rs:44 `techy::serialize::SerialValue`):
   `Null, Bool, Int, Str, Bytes, List, Map, Index` — variant names appear only in
   self-describing non-human-readable formats (postcard uses indices). Private
   same-version pairing per D2; not part of the public contract. Note that the sentinel
   names contain `::` — fine (never rendered by the formats that matter).

## 12. Condition identifiers and projection keys (what a diagnostic's `data` carries)

The condition payload keys are the derive's: raw Rust field names (snake_case),
kebab-cased unit-variant names for `ToDiagnosticValue` enums, `null` for `None`,
strings for `char`s, lists for `Vec`, and — for the two error-chain fields — a list of
rendered messages (`HookFailed.cause`) or `{reference, message, cause-chain}`
(`ResolveError`, error.rs:294-315; note the KEBAB key `cause-chain` next to snake_case
keys everywhere else — **proposal**: `cause_chain`, pre-freeze). These are the
identifiers frozen by [§dd-dr:wire-identifier-stability]; the keys are "stable,
additive-only". Listed for completeness (they are wire-visible through diagnostics):

| Identifier | Type | Where | Payload keys |
|---|---|---|---|
| `core.arguments.expected-expression-argument` | `ExpectedExpressionArgument` | constructs/argument_parsers.rs:99 | `argument_name` |
| `core.arguments.expression-callable-requires-content` | `ExpressionCallableRequiresContent` | constructs/nodes_parser.rs:170 | `callable` |
| `core.arguments.missing-mandatory-argument` | `MissingMandatoryArgument` | constructs/argument_parsers.rs:77 | `argument_name` |
| `core.arguments.repeated-tack-on-field` | `RepeatedTackOnField` | constructs/tack_on_parser.rs:79 | `name`, `escape_char` |
| `core.constructs.descent-limit-approaching` | `DescentLimitApproaching` | constructs/mod.rs:1065 | `detail` |
| `core.constructs.descent-limit-exceeded` | `DescentLimitExceeded` | constructs/mod.rs:1043 | `detail` |
| `core.constructs.implementation-error` | `ImplementationError` | constructs/mod.rs:1006 | `detail` |
| `core.environments.malformed-terminator` | `MalformedEnvironmentTerminator` | constructs/environment_parser.rs:38 | `environment` |
| `core.environments.missing-terminator` | `MissingEnvironmentTerminator` | constructs/environment_parser.rs:52 | `environment`, `found` (`end-of-input` / `stray-group-close`) |
| `core.environments.terminator-mismatch` | `EnvironmentTerminatorMismatch` | constructs/environment_parser.rs:22 | `expected`, `found` |
| `core.groups.stray-group-close` | `StrayGroupClose` | constructs/nodes_parser.rs:357 | `delim` |
| `core.groups.unclosed-group` | `UnclosedGroup` | constructs/group_parser.rs:69 | `expected_close`, `found` (`end-of-input` / `stray-close`) |
| `core.hooks.hook-failed` | `HookFailed` | error.rs:887 | `detail`, `cause` (list of rendered messages) |
| `core.recovery.unusable-recovery-token` | `UnusableRecoveryToken` | constructs/nodes_parser.rs:187 | `spelling`, `kind` (`specials` / `group-open`) |
| `core.sources.no-resolver` | `NoSourceResolver` | constructs/attached_source.rs:279 | `reference` |
| `core.sources.unresolvable-reference` | `UnresolvableSourceReference` | constructs/attached_source.rs:304 | `reference`, `error` (`{reference, message, cause-chain}`) |
| `core.specs.callable-defined-as-error` | `CallableDefinedAsError` | scopes/mod.rs:1555 | `name`, `detail` |
| `core.specs.command-resolution-failed` | `CommandResolutionFailed` | constructs/nodes_parser.rs:140 | `name`, `escape_char`, `detail` |
| `core.specs.provider-commands-shadowed-by-escape` | `ProviderCommandsShadowedByEscape` | scopes/mod.rs:656 | `provider`, `callable_type`, `count`, `example`, `escape_chars` |
| `core.specs.scope-op-failed` | `ScopeOpFailed` | constructs/mod.rs:1022 | `detail` |
| `core.specs.unresolvable-command` | `UnresolvableCommand` | constructs/nodes_parser.rs:103 | `name`, `escape_char`, `detail` |
| `core.token.end-of-stream-after-escape` | `EndOfStreamAfterEscape` | token/error.rs:33 | `escape_char` |
| `core.token.forbidden-char` | `ForbiddenChar` | token/error.rs:46 | `ch` (**proposal**: `char`? `ch` is a Rust-ism; a wire key can be `char`) |
| `core.verbatim.expected-verbatim-delimiter` | `ExpectedVerbatimDelimiter` | constructs/verbatim_parser.rs:87 | `expected` |
| `core.verbatim.unterminated-verbatim` | `UnterminatedVerbatim` | constructs/verbatim_parser.rs:73 | `close` |
| `latexlike.environments.malformed-begin` | `MalformedBegin` | latexlike/environments.rs:117 | `command` |
| `latexlike.environments.orphan-end` | `OrphanEnd` | latexlike/environments.rs:153 | `name`, `terminator` |
| `latexlike.environments.unknown-environment` | `UnknownEnvironment` | latexlike/environments.rs:133 | `name` |

(Note: the derive emits keys from field names — the projection keys are NOT declared
explicitly like `#[serial(name)]` keys are; a field rename silently changes the wire.
**OPEN**: does the Q3 pass want explicit `#[diagnostic(key = …)]` renames on the
condition derive, or a test pinning each condition's projection keys?)

## 13. `Option` conventions — where "omitted key" and where `null`

Two mechanisms, one convention (D8): a derive-level `Option` field that is `None` is an
**omitted key** (read back as `None` from a missing key OR a `null`); a **verbatim
`SerialValue`** field produced by a value conversion renders whatever the conversion
produced — the crate's `Option<T>` value conversion writes `null` for `None`, and `()`
writes `null`. So:

| Where | `None` renders as | Why |
|---|---|---|
| source `digest` | omitted | derive `Option` |
| state `groups.expecting_close` | omitted | derive `Option` |
| state `rules.<section>` (feature absent) | omitted | derive `Option` |
| tree `annotations` (unit) | omitted | derive `Option` |
| argument `region`, `spec_payload` (and `ext` of an ABSENT argument) | omitted | derive `Option` |
| argument `ext` of a provided argument whose ext is `()` | `null` | the ext value is `Some(null)` |
| slot `name` | omitted | derive `Option` |
| error-spec `detail` | omitted | derive `Option` |
| environment syntax `end` | omitted | derive `Option` (latexlike) |
| source `origin` (`Option<String>` = `None`) | `null` | verbatim value: the language's `SourceOrigin` conversion |
| state `mode`, `ext`; node `ext`; slot `ext`; parse-result `session_ext`; group `group_type` (untyped) | `null` | verbatim values whose type is `()` (or an `Option` that is `None`) |

**OPEN (M3 review):** accept the asymmetry (it is principled — "field absent" vs "value
that is null" — but a reader of the JSON sees `origin: null` beside an omitted `digest`),
or make verbatim-value `Option`s omit too (would need the value conversion to signal
absence — the `is_absent_field` hook exists only on the internal derive traits). A
`SerialValue::Null` reads back as `None` in both mechanisms already, so a rendering
change here would be one-directional and safe. Recommendation: accept and document
(the values that render `null` are LANGUAGE-typed slots whose form the language owns).

## 14. Names that violate a scheme or read badly (collected proposals)

| # | Current | Proposal | Reason |
|---|---|---|---|
| 1 | entry envelope key `id` | `identifier` | one concept, one word (`identifier` everywhere else); `id` also names the table ordinal one level up |
| 2 | `core.provider-spec` | `core.spec-identity` | says what the entry IS (a spec named by identity), reviewer's suggestion |
| 3 | state key `rules` (outer) | `token_rules` | `rules.groups.rules` — the outer key names the `TokenRules` struct, the inner the rule list |
| 4 | spec-identity key `key` | `definition` | `key: {name: …}` doubles the word "key"/"name" |
| 5 | `ForbiddenChar` payload key `ch` | `char` | Rust-ism on the wire |
| 6 | `ResolveError` projection key `cause-chain` | `cause_chain` | the only kebab key among snake_case keys |
| 7 | directory key `id` | `table` or `ordinal` (or drop if `$index` moves to names) | "id" of what — the writer's ordinal |
| 8 | region `content_parent` + implicit frame | explicit `{in_region: …}` / `{in_children_of: {node, …}}` | mirror `ContentNodes`; no arithmetic identity |
| 9 | `core.serialization.deserialized-condition` | (keep; area = `serialization`) | flag: the only `core.serialization.*` identifier |
| 10 | `parse-results` | keep kebab (or `parse_results`) | decide the multi-word table-name case once |

None of these is applied; the rename pass follows the user's rulings.

## 15. Q7 — read-side verification levels (proposals; NONE implemented)

The D21 baseline today: every reference bounds- and table-checked; every span validated
against its source (bounds + char boundaries); tree structure re-established through the
builder (children ranges, exact node set, region tiling, content parents) and
`validate_tree` re-run; state rules rebuilt through the constructor; argument specs by
index bound-checked; diagnostics counts cross-checked; digests verified when the writer
recorded them; segment continuity (`start == len`), version, duplicate tables. Everything
below is OPTIONAL extra evidence the writer could record and the reader could check,
ordered by my estimate of value per wire byte:

| # | Check | Wire cost | Catches | Recommendation |
|---|---|---|---|---|
| Q7-a | **per-entry `node_count`** on tree entries (`{nodes: [...], node_count: n}`) | one integer per tree | truncated/edited node lists (already caught by the exact-node-set rule and children ranges — the count is redundant evidence) | skip: redundant with structure checks |
| Q7-b | **argument-count evidence** on callable nodes (`argument_count`) or, better, the callable spec's declared argument count at write time | one integer per callable | environment drift: the reading side's spec (identity-resolved) declares a different number of arguments than the writer's — today caught only when an argument index exceeds `arguments().len()`, NOT when the reader's spec has MORE arguments (the tree then silently pairs its arguments with the first N of a different spec) | **worth it**: cheap, catches a real drift class; alternative without bytes: check `arguments.len() == spec.arguments().len()` on read (the tree already lists every argument, provided or absent) — that check needs NO wire bytes and should simply be added |
| Q7-c | **embedded-source length** (`length` next to `embedded` text) | one integer per source | a text edited in transit (already caught by span validation only if the edit moves offsets past the end) | skip: JSON string integrity is the format's job; referenced sources already carry `length` |
| Q7-d | **per-segment content digest** (`digest` of the segment's canonical bytes) | ~40 bytes per segment + a hash function techy does not have | corruption/tampering of a whole segment | skip for v1: techy implements no hash; a caller can wrap segments in a checksummed container (and formats like postcard-with-CRC exist) |
| Q7-e | **spec identity evidence** — record the spec's declared argument codes/names next to the identity reference (`{provider, callable_type, key, arity: 2}`) | a few bytes per spec entry | the reading environment's package defines the same name with a different shape (drift) | overlaps Q7-b; if b's zero-byte check is added, e is unnecessary for arity; names of arguments could be checked the same way (`ArgumentSpec::name()`) at zero wire cost when the tree stores… it does not store argument names. Optional: `argument_names` on the spec-identity entry — moderate value for FLM (argument by-name access) |
| Q7-f | **stream identity** (`stream: "<uuid>"` in every segment) — Q6 left it as a caller obligation | ~40 bytes per segment | a foreign segment whose `start` happens to line up | defer (Q6 ruling); revisit on a use case |
| Q7-g | **language identity** (`lang: "latexlike"` in the segment or per state) | a few bytes per segment | reading a stream with the wrong `SerializableLang` (today caught late by value-conversion errors or `FeatureAbsent`) | plausible for v1: one string per segment; the language name would need a home (a `SerializableLang::NAME`?) — a small design question |

Bottom line for the user: adopt Q7-b's zero-cost check (`arguments.len()` vs the spec's
declared count) as part of the D21 baseline; consider Q7-g (a language identity string)
as the one addition worth wire bytes; leave the rest out of v1.

## 16. Consolidated OPEN questions for the user (the naming pass agenda)

1. Vocabulary conventions as a whole: table names (kebab for multi-word?), entry
   identifiers two-part vs three-part, keys snake_case, enum strings kebab — confirm.
2. `id` → `identifier` on heterogeneous entries (§1) and the directory `id` key (§11.2).
3. `$index` rendering: ordinal vs table name (§11.1); `TableId` name (§11.2).
4. `core.provider-spec` → `core.spec-identity`; the `key: {name}|{trigger}` shape (§6.1).
5. State outer key `rules` → `token_rules` (§5).
6. Region content-frame: explicit variants or the implicit `content_parent` rule (§11.3).
7. Diagnostic `message` on the wire (§8) and the field order.
8. Parse-result `diagnostics` nested collection object vs flat (§9).
9. `Option` rendering asymmetry (§13): accept or unify.
10. Condition projection keys: explicit renames/pins for the derive-emitted keys; `ch`,
    `cause-chain` (§12).
11. Package names as vocabulary (§10.4).
12. Q7: adopt the zero-byte argument-count check; language identity string (§15).
