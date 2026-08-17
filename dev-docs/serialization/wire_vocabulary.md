# Wire vocabulary inventory — the Q3 record (M6, applied by the M6b rulings pass)

**STATUS: TRANSIENT WORKING DOCUMENT** (deleted at M7, or its surviving content promoted
into the schema description). Prepared by the M6 implementer (2026-08-17) from the wire
structs at `techy-serialize` HEAD as the input to the user's Q3 naming session; updated by
the M6b rename pass (2026-08-17) so that every string below is what the code emits
**after the user's rulings**. Anchors are file + item names (line numbers drifted with
every pass and were dropped): `wire/state.rs WireState.rules` means the field `rules` of
the struct `WireState` in `techy/src/serialize/wire/state.rs`.

How to read it: for each wire-visible name — table names, entry identifiers, key names,
enum strings, reserved JSON forms — the current string, where it is defined, and its
meaning. §16 records the rulings that closed the M6 agenda; the one OPEN list left for
M7 is §17.

Vocabulary conventions in force (confirmed by the user, 2026-08-17):

- **Table names** — lowercase plural nouns; a multi-word name is kebab-cased
  (`parse-results`).
- **Entry identifiers** — two-part `<owner>.<kind>` (`core.source`, `latexlike.begin`),
  kebab-cased kinds; the table is the "area" — unlike condition identifiers' three parts
  (`core.groups.unclosed-group`, [§dd-dr:wire-identifier-stability]).
- **Key names** — snake_case (`line_number_offset`, `callable_type`), chosen explicitly
  through `#[serial(name = …)]` on the wire structs (`token_rules` and `definition` now
  differ from their Rust field names) and, for condition projections, the derive's field
  names or a field's `#[diagnostic(key = "…")]` (`char`).
- **Enum strings** (unit variants, forms) — kebab-case (`end-of-input`,
  `exit-math-context`, `in_children_of` being the one snake_case exception: it is a KEY of
  the region's `content` object, mirroring `ContentNodes`, not an enum string; single
  words otherwise: `primary`, `resolved`, `macro`).
- **Reserved JSON keys** — `$`-prefixed (`$index`, `$bytes`); NO escaping: a user map key
  beginning with `$` is a typed error on writing (`SerialValueError::ReservedMapKey`,
  from every rendering and from the bridge) and on reading.
- **Package names** — the language's stable vocabulary (`_builtin`, `minilatex`,
  `minilatex.item`), documented on the recipe registrations.

---

## 1. Segment envelope and entry forms

Defined in `techy/src/serialize/engine/segment.rs` (`Segment`, `SegmentMeta`,
`SegmentTable`, `WireEntry`, the private `WireMain`; `Segment::VERSION`).

| Wire name | String | Where | Meaning |
|---|---|---|---|
| segment key | `version` | `Segment.version` | `Segment::VERSION` (= 1), in every segment (Q6) |
| segment key | `meta` | `Segment.meta` (`SegmentMeta`) | what the segment says about itself as a whole; **always present** (empty object when nothing is set) — the extension point for segment-wide keys |
| meta key | `profile` | `SegmentMeta.profile` | optional; the emitting session's profile string (`SerdeSession::set_profile`); a reader with a profile requires the same one (`DeserializeError::ProfileMismatch`) |
| segment key | `tables` | `Segment.tables` | the table directory, registration order |
| segment key | `main` | `Segment.main` (`WireMain`) | optional; the segment's main entry as a table position `{"$index": [writer table, position]}` (`SerdeSession::take_segment_with_main`); `push_segment` returns it translated |
| directory key | `name` | `SegmentTable.name` | table name |
| directory key | `table` | `SegmentTable.id` | the WRITER's `TableId` ordinal (an integer) — RULED `id` → `table` (`TableId` keeps its name) |
| directory key | `start` | `SegmentTable.start` | position of the part's first entry |
| directory key | `entries` | `SegmentTable.entries` | the entries in position order |
| entry key (heterogeneous) | `identifier` | `WireEntry.identifier` | the entry's identifier — RULED `id` → `identifier` |
| entry key (heterogeneous) | `data` | `WireEntry.data` | the payload |
| homogeneous entry | (bare data) | `session.rs` (write path) | no envelope |

Key order: `version`, `meta`, `tables`, `main`.

## 2. Table names and ordinals (`SerdeSession::new()`)

Defined in `techy/src/serialize/drivers/mod.rs` (`*_TABLE` constants); registered in
`drivers/standard.rs` (`with_source_driver`).

| Ordinal | Name | Driver | Kind |
|---|---|---|---|
| 0 | `sources` | `SourceSerdeDriver` | homogeneous, `core.source` |
| 1 | `states` | `StateSerdeDriver` | homogeneous, `core.state` |
| 2 | `specs` | `SpecSerdeDriver` | heterogeneous |
| 3 | `providers` | `ProviderSerdeDriver` | heterogeneous |
| 4 | `trees` | `TreeSerdeDriver` | heterogeneous by annotation type |
| 5 | `diagnostics` | `DiagnosticSerdeDriver` | homogeneous, `core.diagnostic` |
| 6 | `parse-results` | `ParseResultSerdeDriver` | homogeneous, `core.parse-result` (kebab — RULED) |

The ordinal is what a table reference (`$index`) names in the canonical JSON; a reader
matches tables by NAME through the directory and never relies on the ordinal (§11).

## 3. Entry identifiers

| Table | Identifier | Where | Payload (§ below) |
|---|---|---|---|
| sources | `core.source` | `drivers/mod.rs SOURCE_IDENTIFIER` | §4 |
| states | `core.state` | `drivers/mod.rs STATE_IDENTIFIER` | §5 |
| trees | `core.tree` (unit annotation) | `drivers/mod.rs CORE_TREE_IDENTIFIER` | §7; other annotation types are registered under caller-chosen identifiers (`TableHandle::register_annotation`) |
| diagnostics | `core.diagnostic` | `drivers/mod.rs DIAGNOSTIC_IDENTIFIER` | §8 |
| parse-results | `core.parse-result` | `drivers/mod.rs PARSE_RESULT_IDENTIFIER` | §9 |
| specs | `core.provider-spec-identity` | `drivers/mod.rs SPEC_IDENTITY_IDENTIFIER` | §6.1 identity form — RULED (was `core.provider-spec`) |
| specs | `core.error-spec` | `drivers/mod.rs ERROR_SPEC_IDENTIFIER` | §6.5 |
| providers | `core.package` | `drivers/mod.rs PACKAGE_IDENTIFIER` | §6.2 |
| providers | `core.scope` | `drivers/mod.rs SCOPE_IDENTIFIER` | §6.3 |
| providers | `core.fallback-provider` | `drivers/mod.rs FALLBACK_PROVIDER_IDENTIFIER` | §6.4 |
| specs | `latexlike.begin` | `latexlike/serialize.rs BEGIN_IDENTIFIER` | §10.3 (self-contained `BeginSpec`) |
| specs | `latexlike.end` | `latexlike/serialize.rs END_IDENTIFIER` | §10.3 |
| specs | `latexlike.paragraph-break` | `latexlike/serialize.rs PARAGRAPH_BREAK_IDENTIFIER` | §10.3 |
| specs | `latexlike.input` | `latexlike/serialize.rs INPUT_IDENTIFIER` | §10.3 |
| (adapter condition) | `core.serialization.deserialized-condition` | `drivers/diagnostic.rs DeserializedCondition::IDENTIFIER` | not an entry — the adapter type's own condition identifier (three-part scheme; area `serialization`) |

## 4. Sources (`core.source`) — `techy/src/serialize/wire/source.rs`

| Key / string | String | Where | Value |
|---|---|---|---|
| key | `origin` | `WireSource.origin` | `SourceOrigin`'s value conversion (`Option<String>` default: `null` or string) |
| key | `provenance` | `WireSource.provenance` | see below |
| key | `line_number_offset` | `WireSource.line_number_offset` | integer |
| key | `column_number_offset` | `WireSource.column_number_offset` | integer |
| key | `text` | `WireSource.text` | `{embedded: str}` or `{referenced: {…}}` |
| text form | `embedded` | `WireSourceText::Embedded` | the text |
| text form | `referenced` | `WireSourceText::Referenced` | `{length, digest?}` |
| key | `length` | `WireSourceReference.length` | integer (bytes) |
| key | `digest` | `WireSourceReference.digest` | `{algorithm, bytes}`; **omitted** when none |
| key | `algorithm` | `WireDigest.algorithm` | string (caller's hash name, e.g. `sha256`) |
| key | `bytes` | `WireDigest.bytes` | `$bytes` (base64) |
| provenance form | `primary` | `WireProvenance::Primary` | bare string |
| provenance form | `resolved` | `WireProvenance::Resolved` | `{reference, triggered_at}` |
| provenance form | `synthesized` | `WireProvenance::Synthesized` | `{description, triggered_at}` |
| key | `reference` | `WireProvenance::Resolved.reference` | string (the reference that was resolved) |
| key | `description` | `WireProvenance::Synthesized.description` | string |
| key | `triggered_at` | `…::Resolved/Synthesized.triggered_at` | a span (below) |
| span key | `source` | `WireSpan.source` | `$index` into `sources` |
| span key | `start` | `WireSpan.start` | byte offset (inclusive) |
| span key | `end` | `WireSpan.end` | byte offset (exclusive) |

`Option` note: `origin` is a verbatim `SerialValue` (the language's conversion) → `None`
renders `null`; `digest` is a derive `Option` → **omitted** (§13).

## 5. States (`core.state`) — `techy/src/serialize/wire/state.rs`

| Key / string | String | Where | Value |
|---|---|---|---|
| key | `token_rules` | `WireState.rules` | the token rules (sections below) — RULED (was `rules`; the Rust field keeps its name) |
| key | `mode` | `WireState.mode` | `ModeId`'s value conversion (`null` for `()`; latexlike `text`/`math`) |
| key | `ext` | `WireState.ext` | `StateExt`'s value conversion (`null` for `()`) |
| key | `scopes` | `WireState.scopes` | list of `$index` into `providers`, outermost first |
| section | `whitespace` | `WireTokenRules.whitespace` | `{enabled, chars}` — **omitted** when the feature is absent |
| section | `paragraphs` | `WireTokenRules.paragraphs` | `{enabled}` |
| section | `groups` | `WireTokenRules.groups` | `{enabled, rules, temporary, expecting_close?}` |
| section | `commands` | `WireTokenRules.commands` | `{enabled, rules}` |
| section | `comments` | `WireTokenRules.comments` | `{enabled, rules}` |
| section | `specials` | `WireTokenRules.specials` | `{enabled}` |
| section | `forbidden_chars` | `WireTokenRules.forbidden_chars` | `{chars}` |
| key | `enabled` | each section struct | bool |
| key | `chars` | `WireWhitespaceRules.chars`, `WireForbiddenCharsRules.chars` | string |
| key | `rules` | `WireGroupRules/WireCommandRules/WireCommentRules.rules` | list of rules (the inner `rules` stays; `token_rules.groups.rules` reads fine now) |
| key | `temporary` | `WireGroupRules.temporary` | list of group rules |
| key | `expecting_close` | `WireGroupRules.expecting_close` | a group rule; **omitted** when none |
| group rule key | `group_type` | `WireGroupRule.group_type` | `GroupTypeId`'s value conversion |
| group rule key | `open` | `WireGroupRule.open` | string |
| group rule key | `close` | `WireGroupRule.close` | string |
| command rule key | `escape_char` | `WireCommandRule.escape_char` | one-character string |
| command rule key | `name_chars` | `WireCommandRule.name_chars` | string |
| comment rule key | `start` | `WireCommentRule.start` | string |

## 6. Specs and providers — `techy/src/serialize/wire/specs.rs`

### 6.1 `core.provider-spec-identity` (spec identity through the provenance stamp) — `WireSpecIdentity`

| Key / string | String | Where | Value |
|---|---|---|---|
| key | `provider` | `WireSpecIdentity.provider` | `$index` into `providers` |
| key | `callable_type` | `WireSpecIdentity.callable_type` | `CallableTypeId`'s value conversion |
| key | `definition` | `WireSpecIdentity.key` | `{name: str}` or `{trigger: str}` — RULED (was `key`; the tagged form kept) |
| definition-key form | `name` | `WireDefinitionKey::Name` | string |
| definition-key form | `trigger` | `WireDefinitionKey::Trigger` | string |

### 6.2 `core.package` — `WirePackage`

| key | `name` | `WirePackage.name` | string — the package name, stable vocabulary (§10.4) |
|---|---|---|---|

### 6.3 `core.scope` — `WireScope`, `WireDefinition`

| Key | String | Where | Value |
|---|---|---|---|
| key | `name` | `WireScope.name` | string |
| key | `definitions` | `WireScope.definitions` | list of `{callable_type, name, spec}` (BTreeMap order: callable type, then name) |
| definition key | `callable_type` | `WireDefinition.callable_type` | value conversion |
| definition key | `name` | `WireDefinition.name` | string |
| definition key | `spec` | `WireDefinition.spec` | `$index` into `specs` |

### 6.4 `core.fallback-provider` — `WireFallbackProvider`, `WireFallback`

| key | `name` | `WireFallbackProvider.name` | string |
|---|---|---|---|
| key | `fallbacks` | `WireFallbackProvider.fallbacks` | list of `{callable_type, spec}` |
| fallback key | `callable_type` | `WireFallback.callable_type` | value conversion |
| fallback key | `spec` | `WireFallback.spec` | `$index` into `specs` |

### 6.5 `core.error-spec` — `WireErrorSpec`

| key | `detail` | `WireErrorSpec.detail` | string; **omitted** when none |
|---|---|---|---|

## 7. Trees (`core.tree` and registered annotation identifiers) — `techy/src/serialize/wire/tree.rs`

| Key / string | String | Where | Value |
|---|---|---|---|
| tree key | `nodes` | `WireTree.nodes` | list of nodes, storage order (root first) |
| tree key | `annotations` | `WireTree.annotations` | list, one per node; **omitted** for the unit annotation |
| node key | `kind` | `WireNode.kind` | `"list"` or `{chars: …}` / `{group: …}` / `{callable: …}` / `{comment: …}` |
| node key | `span` | `WireNode.span` | span (§4) |
| node key | `state` | `WireNode.state` | `$index` into `states` |
| node key | `ext` | `WireNode.ext` | `NodeExt`'s value conversion (`null` for `()`) |
| node key | `children` | `WireNode.children` | `{start, end}` storage-index range |
| kind | `chars` | `WireNodeKind::Chars` | `{content}` |
| kind | `group` | `WireNodeKind::Group` | `{group_type, open, close}` |
| kind | `callable` | `WireNodeKind::Callable` | `{callable_type, name, spec, arguments, slots, invocation_syntax}` |
| kind | `comment` | `WireNodeKind::Comment` | `{start, content, post_space}` |
| kind | `list` | `WireNodeKind::List` | bare string |
| chars key | `content` | `WireNodeKind::Chars.content` | text content (§7.1) |
| group key | `group_type` | `WireNodeKind::Group.group_type` | value conversion (`null` for an untyped group) |
| group key | `open` / `close` | `WireNodeKind::Group.open/close` | text content |
| callable key | `callable_type` | `WireNodeKind::Callable.callable_type` | value conversion |
| callable key | `name` | `WireNodeKind::Callable.name` | string |
| callable key | `spec` | `WireNodeKind::Callable.spec` | `$index` into `specs` |
| callable key | `arguments` | `WireNodeKind::Callable.arguments` | list of arguments |
| callable key | `slots` | `WireNodeKind::Callable.slots` | list of slots |
| callable key | `invocation_syntax` | `WireNodeKind::Callable.invocation_syntax` | `InvocationSyntax`'s value conversion (§10.2 for latexlike) |
| comment key | `start` / `content` / `post_space` | `WireNodeKind::Comment.*` | text content |
| argument key | `region` | `WireArgument.region` | region; **omitted** for an absent argument |
| argument key | `ext` | `WireArgument.ext` | `ArgumentExt`'s value conversion; written for a provided argument (`null` for the unit ext), **omitted** for an absent argument (which is `{}`); the reader accepts an omitted key or `null` alike |
| argument key | `spec_payload` | `WireArgument.spec_payload` | the callable spec's out-of-band argument-spec description; **omitted** under the index rule |
| slot key | `name` | `WireSlot.name` | string; **omitted** for an unnamed slot |
| slot key | `region` | `WireSlot.region` | region |
| slot key | `role` | `WireSlot.role` | `content` / `attached` / `hidden` |
| slot key | `ext` | `WireSlot.ext` | `SlotExt`'s value conversion (`{body: bool}` in latexlike) |
| region key | `children` | `WireRegion.children` | `{start, end}` offsets into the callable's child list |
| region key | `content` | `WireRegion.content` (`WireContent`) | one of two explicit variants — RULED (was `content` offsets + `content_parent` with an implicit rule) |
| content form | `in_region` | `WireContent::InRegion` | `{start, end}` — offsets within the region's own node list |
| content form | `in_children_of` | `WireContent::InChildrenOf` | `{node, start, end}` — `node` the storage index of the node whose children hold the content (inside the region, stored after the callable), offsets within its child list |
| range key | `start` / `end` | `WireRange` | integers, half-open |

### 7.1 Context-free core value shapes (used inside trees; `wire/tree.rs`)

| Type | String | Where |
|---|---|---|
| `Span` | `{start, end}` | `wire/tree.rs impl … for Span` (crate-internal on the wire — appears only inside `spanned`) |
| `TextContent` | `{spanned: {start, end}}` / `{owned: "text"}` | `wire/tree.rs impl … for TextContent`; owned-only inside language payloads (D23) |
| `SlotRole` | `content` / `attached` / `hidden` | `wire/tree.rs impl … for SlotRole` |
| `GroupRule` (value) | `{group_type, open, close}` | `drivers/tree.rs` value conversion (same shape as the state's group rules) |
| `SourceSpan` (value) | `{source, start, end}` | `drivers/source.rs` value conversion |

## 8. Diagnostics (`core.diagnostic`) — `techy/src/serialize/wire/diagnostic.rs`

| Key / string | String | Where | Value |
|---|---|---|---|
| key | `severity` | `WireDiagnostic.severity` | `note` / `warning` / `error` |
| key | `identifier` | `WireDiagnostic.identifier` | the condition's wire identity |
| key | `message` | `WireDiagnostic.message` | the rendered `Display` at write time — RULED kept; wording not stable |
| key | `data` | `WireDiagnostic.data` | the `serializable_data()` projection (a `DiagnosticValue`; §12 lists condition payload keys) |
| key | `span` | `WireDiagnostic.span` | span |
| key | `frames` | `WireDiagnostic.frames` | list of `{title, span}`, innermost first |
| frame key | `title` | `WireTraceFrame.title` | rendered frame title (`group ‘{’`, `argument #1 of macro ‘\emph’`); wording is presentation (not stable) |
| frame key | `span` | `WireTraceFrame.span` | span |
| severity string | `note` / `warning` / `error` | `WireSeverity` | |

Field order (canonical rendering) — RULED: `severity, identifier, message, data, span, frames`.

## 9. Parse results (`core.parse-result`) — `techy/src/serialize/wire/parse_result.rs`

| Key | String | Where | Value |
|---|---|---|---|
| key | `tree` | `WireParseResult.tree` | `$index` into `trees` (must be a `core.tree` entry) |
| key | `diagnostics` | `WireParseResult.diagnostics` | `{items, limit, suppressed, error_count}` — a NESTED collection object (mirrors `Diagnostics`, P1) — RULED kept nested |
| key | `session_ext` | `WireParseResult.session_ext` | `SessionExt`'s value conversion (`null` for `()`) |
| collection key | `items` | `WireDiagnostics.items` | list of `$index` into `diagnostics`, recording order |
| collection key | `limit` | `WireDiagnostics.limit` | integer (`Diagnostics::limit`; a cap above `i64::MAX` cannot be written — documented on `with_limit`) |
| collection key | `suppressed` | `WireDiagnostics.suppressed` | integer |
| collection key | `error_count` | `WireDiagnostics.error_count` | integer |

## 10. The latexlike vocabulary — `techy/src/latexlike/serialize.rs` (+ serde renames in `latexlike/mod.rs`)

### 10.1 Closed vocabulary values (`names` module; serde renames pinned equal by test)

| Type | Strings | Rust variants |
|---|---|---|
| `CallableType` | `macro`, `environment`, `specials` | Macro, Environment, Specials |
| `GroupType` | `content`, `{math: <form>}`, `verbatim` | Content, Math(form), Verbatim |
| `MathGroupForm` | `inline`, `display` | Inline, Display |
| `Mode` | `text`, `math` | Text, Math |
| `Event` | `exit-math-context` | ExitMathContext |
| `BodyMarker` (slot ext) | `{body: bool}` | |

### 10.2 The invocation syntax (`InvocationSyntaxData`)

| Form | String | Where |
|---|---|---|
| macro | `{macro: {escape_char, post_space}}` | `WireMacroSyntax` (`post_space` a `TextContent` — owned on the wire, `{owned: " "}`) |
| environment | `{environment: {begin, end?}}` | `WireEnvironmentSyntax` (`end` **omitted** when none) |
| specials | `specials` (bare string) | the unit variant |
| side syntax | `{escape_char, command_word, post_space, name_group_rule}` | `WireEnvironmentSideSyntax` (`command_word`/`post_space` owned text; `name_group_rule` a group rule inlined) |

### 10.3 Self-contained spec forms

| Identifier | Payload | Where |
|---|---|---|
| `latexlike.begin` | `{end_command_name}` | `WireBeginSpec` |
| `latexlike.end` | `{}` | unit recipe |
| `latexlike.paragraph-break` | `{}` | unit recipe |
| `latexlike.input` | `{persist_state, attached_slot_ext}` | `WireInputMacroSpec` |

Stamped `MacroSpec`/`SpecialsSpec`/`EnvironmentSpec`/`BeginSpec`/`InputMacroSpec` and core
`StdCallableSpec` all use `core.provider-spec-identity` (§6.1).

### 10.4 Provider names that appear as data (`core.package {name}`)

`_builtin` (the seed package; the leading underscore marks it as not user-facing),
`minilatex`, `minilatex.item`. RULED: package names are part of the language's stable
serialized vocabulary (documented on `latexlike::serialize::register_package_recipes` and
`minidefs::register_package_recipes`).

## 11. Structural conventions (all RULED)

1. **`Index` rendering — ordinal.** `{"$index": [<table ordinal>, <position>]}`
   (`render.rs INDEX_KEY`; the ordinal is the WRITER's `TableId`, translated by the reader
   through the segment directory). Kept; the directory's `table` key is what maps it.
2. **`TableId` keeps its name**; a `TableId` on the wire is the writer's session-local
   ordinal that the directory translates (documented on `TableId`).
3. **Region content designation** — explicit variants `{"in_region": {start, end}}` /
   `{"in_children_of": {node, start, end}}` (`WireContent`, mirroring the live
   `ContentNodes::{InRegion, InChildrenOf}`); the implicit `content_parent == the
   callable` rule is gone. The reader validates as before: an `in_children_of` node must
   be in range and stored after its callable (naming the callable itself is that error);
   the builder checks it lies inside the region's subtree.
4. **Reserved JSON forms** (`render.rs`): `{"$bytes": "<base64>"}` — standard alphabet
   `A–Z a–z 0–9 + /`, `=` padding, no line breaks, strict decoder (`base64.rs`);
   `{"$index": [t, i]}`; any other key beginning with `$` is an error on reading; on
   writing, a `SerialValue::Map` key beginning with `$` is `SerialValueError::ReservedMapKey`
   from the canonical AND the compact rendering and from the bridge (map keys, struct
   field names, variant names). No escaping.
5. **`SerialValue::Map` is an ordered association list**: equality is order-sensitive and
   the rendering preserves order — the same entries in another order are a different
   value with a different rendering (pinned by
   `serde_tests::map_equality_and_rendering_are_order_sensitive`).
6. **Compact (binary) rendering names** (`render.rs COMPACT_VARIANTS`, sentinels
   `bridge.rs INDEX_SENTINEL` = `techy::serialize::Index`, `render.rs VALUE_SENTINEL` =
   `techy::serialize::SerialValue`): `Null, Bool, Int, Str, Bytes, List, Map, Index` —
   variant names appear only in self-describing non-human-readable formats (postcard uses
   indices). Private same-version pairing per D2; not part of the public contract.

## 12. Condition identifiers and projection keys (what a diagnostic's `data` carries)

The condition payload keys are the derive's: the Rust field names (snake_case) — or a
field's explicit `#[diagnostic(key = "…")]` (added by the rulings pass; used by
`ForbiddenChar`: `ch` → `char`) —, kebab-cased unit-variant names for `ToDiagnosticValue`
enums, `null` for `None`, strings for `char`s, lists for `Vec`, and — for the two
error-chain fields — a list of rendered messages (`HookFailed.cause`) or `{reference,
message, cause_chain}` (`ResolveError`, `error.rs`; RULED `cause-chain` → `cause_chain`).
These are the identifiers frozen by [§dd-dr:wire-identifier-stability]; the keys are
"stable, additive-only". Listed for completeness (they are wire-visible through
diagnostics):

| Identifier | Type | Where | Payload keys |
|---|---|---|---|
| `core.arguments.expected-expression-argument` | `ExpectedExpressionArgument` | `constructs/argument_parsers.rs` | `argument_name` |
| `core.arguments.expression-callable-requires-content` | `ExpressionCallableRequiresContent` | `constructs/nodes_parser.rs` | `callable` |
| `core.arguments.missing-mandatory-argument` | `MissingMandatoryArgument` | `constructs/argument_parsers.rs` | `argument_name` |
| `core.arguments.repeated-tack-on-field` | `RepeatedTackOnField` | `constructs/tack_on_parser.rs` | `name`, `escape_char` |
| `core.constructs.descent-limit-approaching` | `DescentLimitApproaching` | `constructs/mod.rs` | `detail` |
| `core.constructs.descent-limit-exceeded` | `DescentLimitExceeded` | `constructs/mod.rs` | `detail` |
| `core.constructs.implementation-error` | `ImplementationError` | `constructs/mod.rs` | `detail` |
| `core.environments.malformed-terminator` | `MalformedEnvironmentTerminator` | `constructs/environment_parser.rs` | `environment` |
| `core.environments.missing-terminator` | `MissingEnvironmentTerminator` | `constructs/environment_parser.rs` | `environment`, `found` (`end-of-input` / `stray-group-close`) |
| `core.environments.terminator-mismatch` | `EnvironmentTerminatorMismatch` | `constructs/environment_parser.rs` | `expected`, `found` |
| `core.groups.stray-group-close` | `StrayGroupClose` | `constructs/nodes_parser.rs` | `delim` |
| `core.groups.unclosed-group` | `UnclosedGroup` | `constructs/group_parser.rs` | `expected_close`, `found` (`end-of-input` / `stray-close`) |
| `core.hooks.hook-failed` | `HookFailed` | `error.rs` | `detail`, `cause` (list of rendered messages) |
| `core.recovery.unusable-recovery-token` | `UnusableRecoveryToken` | `constructs/nodes_parser.rs` | `spelling`, `kind` (`specials` / `group-open`) |
| `core.sources.no-resolver` | `NoSourceResolver` | `constructs/attached_source.rs` | `reference` |
| `core.sources.unresolvable-reference` | `UnresolvableSourceReference` | `constructs/attached_source.rs` | `reference`, `error` (`{reference, message, cause_chain}`) |
| `core.specs.callable-defined-as-error` | `CallableDefinedAsError` | `scopes/mod.rs` | `name`, `detail` |
| `core.specs.command-resolution-failed` | `CommandResolutionFailed` | `constructs/nodes_parser.rs` | `name`, `escape_char`, `detail` |
| `core.specs.provider-commands-shadowed-by-escape` | `ProviderCommandsShadowedByEscape` | `scopes/mod.rs` | `provider`, `callable_type`, `count`, `example`, `escape_chars` |
| `core.specs.scope-op-failed` | `ScopeOpFailed` | `constructs/mod.rs` | `detail` |
| `core.specs.unresolvable-command` | `UnresolvableCommand` | `constructs/nodes_parser.rs` | `name`, `escape_char`, `detail` |
| `core.token.end-of-stream-after-escape` | `EndOfStreamAfterEscape` | `token/error.rs` | `escape_char` |
| `core.token.forbidden-char` | `ForbiddenChar` | `token/error.rs` | `char` (RULED; the Rust field stays `ch`, the key is `#[diagnostic(key = "char")]`) |
| `core.verbatim.expected-verbatim-delimiter` | `ExpectedVerbatimDelimiter` | `constructs/verbatim_parser.rs` | `expected` |
| `core.verbatim.unterminated-verbatim` | `UnterminatedVerbatim` | `constructs/verbatim_parser.rs` | `close` |
| `latexlike.environments.malformed-begin` | `MalformedBegin` | `latexlike/environments.rs` | `command` |
| `latexlike.environments.orphan-end` | `OrphanEnd` | `latexlike/environments.rs` | `name`, `terminator` |
| `latexlike.environments.unknown-environment` | `UnknownEnvironment` | `latexlike/environments.rs` | `name` |

(Note: the derive emits keys from field names unless a field carries
`#[diagnostic(key = …)]`; a field rename without the attribute silently changes the
wire. The rulings pass added the attribute but no per-condition projection-key pin test;
`tests/derive_conditions.rs` pins the attribute's behavior and `ForbiddenChar`'s key.)

## 13. `Option` conventions — where "omitted key" and where `null` (RULED: accepted, documented once)

Two mechanisms, one convention (D8): a derive-level `Option` field that is `None` is an
**omitted key** (read back as `None` from a missing key OR a `null`); a **verbatim
`SerialValue`** field produced by a value conversion renders whatever the conversion
produced — the crate's `Option<T>` value conversion writes `null` for `None`, and `()`
writes `null`. So:

| Where | `None` renders as | Why |
|---|---|---|
| source `digest` | omitted | derive `Option` |
| state `groups.expecting_close` | omitted | derive `Option` |
| state `token_rules.<section>` (feature absent) | omitted | derive `Option` |
| tree `annotations` (unit) | omitted | derive `Option` |
| argument `region`, `spec_payload` (and `ext` of an ABSENT argument) | omitted | derive `Option` |
| argument `ext` of a provided argument whose ext is `()` | `null` | the ext value is `Some(null)` |
| slot `name` | omitted | derive `Option` |
| error-spec `detail` | omitted | derive `Option` |
| environment syntax `end` | omitted | derive `Option` (latexlike) |
| segment `main`; meta `profile` | omitted | derive `Option` |
| source `origin` (`Option<String>` = `None`) | `null` | verbatim value: the language's `SourceOrigin` conversion |
| state `mode`, `ext`; node `ext`; slot `ext`; parse-result `session_ext`; group `group_type` (untyped) | `null` | verbatim values whose type is `()` (or an `Option` that is `None`) |

RULED (2026-08-17): the asymmetry is accepted and documented once — the `techy::serialize`
module docs ("Absent values in the serialized form") and the schema draft §1 — as
"field absent" (the structure's) vs "value that is null" (a language-owned slot's); a
`SerialValue::Null` reads back as `None` in both mechanisms.

## 14. Names that violated a scheme or read badly — the M6 proposals and their rulings

| # | Was | Now | Ruling |
|---|---|---|---|
| 1 | entry envelope key `id` | `identifier` | applied |
| 2 | `core.provider-spec` | `core.provider-spec-identity` | applied (the user's spelling; the reviewer's `core.spec-identity` not taken) |
| 3 | state key `rules` (outer) | `token_rules` | applied |
| 4 | spec-identity key `key` | `definition` | applied |
| 5 | `ForbiddenChar` payload key `ch` | `char` | applied (field attribute) |
| 6 | `ResolveError` projection key `cause-chain` | `cause_chain` | applied |
| 7 | directory key `id` | `table` | applied |
| 8 | region `content_parent` + implicit frame | explicit `{in_region: …}` / `{in_children_of: {node, …}}` | applied |
| 9 | `core.serialization.deserialized-condition` | (unchanged) | kept |
| 10 | `parse-results` | (unchanged; kebab) | kept |

## 15. Q7 — read-side verification levels — RULED

The D21 baseline (every reference bounds- and table-checked; every span validated
against its source; tree structure re-established through the builder and `validate_tree`
re-run; state rules rebuilt through the constructor; argument specs by index
bound-checked; diagnostics counts cross-checked; digests verified when recorded; segment
continuity, version, duplicate tables) IS the verification level. Of the M6 proposals:

| # | Check | Ruling |
|---|---|---|
| Q7-a | per-entry `node_count` | out (redundant with structure checks) |
| Q7-b | argument-count evidence / `arguments.len() == spec.arguments().len()` | **REJECTED** (imprecise and fragile against custom specs) — not added |
| Q7-c | embedded-source `length` | out |
| Q7-d | per-segment digest | out |
| Q7-e | spec identity evidence (arity, argument names) | **REJECTED** — not added |
| Q7-f | stream identity | deferred (Q6 ruling stands) |
| Q7-g | a per-segment identity string | **ADOPTED as the caller-provided `profile`** (`SerdeSession::set_profile`; carried in `meta.profile`; a reader with a profile refuses a mismatch or a missing one — fail-closed; a reader without one accepts any). It names the configuration that reads the stream fully — the caller's contract, not techy's. The name `profile` is provisional (see the M6b log entry). |

## 16. The M6 agenda — RESOLVED (user, 2026-08-17; applied by the M6b pass)

1. Vocabulary conventions as a whole — confirmed (kebab multi-word table names, two-part
   entry identifiers, snake_case keys, kebab enum strings).
2. `id` → `identifier` on heterogeneous entries; directory `id` → `table`.
3. `$index` rendering stays `[ordinal, position]`; `TableId` keeps its name.
4. `core.provider-spec` → `core.provider-spec-identity`; payload key `key` → `definition`
   (tagged `{name}` | `{trigger}` form kept).
5. State outer key `rules` → `token_rules`.
6. Region content frame → explicit `in_region` / `in_children_of` variants.
7. Diagnostic `message` kept; order `severity, identifier, message, data, span, frames`.
8. Parse-result `diagnostics` stays nested.
9. `Option` asymmetry accepted and documented once.
10. Condition keys `ch` → `char`, `cause-chain` → `cause_chain` (a field-level
    `#[diagnostic(key = …)]` attribute on the derive; no per-condition pin test).
11. Package names `_builtin` / `minilatex` / `minilatex.item` are stable vocabulary.
12. Q7: no argument-count check, no spec evidence; the `profile` string only.
13. NEW: segment `main` (optional; `take_segment_with_main`, returned translated by
    `push_segment`) and `meta.profile` (optional inside the always-present `meta`;
    `SerdeSession::set_profile`); base64 standard alphabet; NO `$` escaping (a `$`-key is
    a typed error); `SerialValue::Map` order-sensitive; `Deserialize for DiagnosticValue`.

## 17. OPEN for M7 (not vocabulary)

- **`usize` widths on the wire.** Counts and offsets that are `usize` in the live types
  (`Diagnostics::limit/suppressed/error_count`, source `length`, node counts) are written
  as `i64` and read back into `usize` with range checks; a value that fits `i64` but not
  the reading target's `usize` (a 32-bit reader of a 64-bit writer's stream) is an
  `IntegerOutOfRange` read error. The test suite uses values valid on both widths
  (`1 << 20`, not `1 << 40`). Whether the schema should state a portable bound (e.g. "counts
  fit `u32`") is M7's to decide.
- The permanent home of §1–§13 (schema page vs ARCHITECTURE section) — M7.
