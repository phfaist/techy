# Parser Library Parity — pylatexenc `latexnodes.parsers` vs techy `constructs`

Status: survey settled July 2026 (parser-library survey session); **all rows landed as
of the N2–N6 implementation session (July 2026)** — no todo rows remain. Companion
decision entries: DESIGN_RATIONALE.md §3.6 ("parser-library gap list" and the
"deferred parsers N2/N3/N4/N6" landed entry). Overlapping informal items in
TODO_Big.md ("Parsers that are worth implementing", "Delimited Group Parser Helper
Utility") are subsumed by the table below.

**Scope.** Loose parity, not exact: some pylatexenc construct parsers are better served
by postprocessing parsed content or by composition of simpler parsers. Excluded from
the comparison (per instructions): the core dispatch machinery that directly drives
declared arguments and the environment body — pylatexenc's
`LatexStandardArgumentParser`/`get_standard_argument_parser` and macrospec's
`LatexMacroCallParser`/`LatexArgumentsParser`/`LatexEnvironmentBodyContentsParser` —
which techy covers with `StdInvocationParser`, `parse_declared_arguments`, the standard
`ArgumentParser` implementations, and `EnvironmentBodyParser`. Parsers that custom
specs would plug in to parse *specific* arguments or bodies (verbatim contents,
embellishments, chars groups, …) **are** in scope.

**Strategy legend.**

- **implemented** — exists in `src/constructs/` today.
- **todo** — agreed gap, to implement (target phase noted where known).
- **absorbed** — the capability exists in a different form; no dedicated type needed.
- **discarded** — deliberately not ported (replacement noted).

## Summary table

| pylatexenc parser (`latexnodes.parsers`) | techy equivalent | strategy |
|---|---|---|
| `LatexParserBase` | `ConstructParser` trait (argument positions: `ArgumentParser`) | implemented |
| `LatexGeneralNodesParser` | `NodesParser` + `StopSpec` | implemented |
| `LatexSingleNodeParser` | `NodesParser` with a node-stop condition | absorbed |
| `LatexDelimitedExpressionParser` + `…ParserInfo` | composition: `GroupParser` + minted `GroupRule` + `ChildStateSpec` | absorbed |
| `LatexDelimitedExpressionParserOpeningDelimiterNotFound` | argument-probe protocol (`Ok(None)` + noise rewind) | absorbed |
| `LatexDelimitedGroupParser` + `…ParserInfo` | `GroupParser` | implemented — interior-state plug settled by design, Phase 7 plan session [N1] |
| `LatexDelimitedMultiDelimGroupParser` + `…ParserInfo` | `GroupArgumentParser::any_of` / `OptionalGroupArgumentParser::any_of` (multi-rule forms of the existing types) | implemented (July 2026) [N2] |
| `LatexMathParser` | `Group` node under a preset math group class + parsing-mode delta | implemented (7.5 preset wiring: `LatexlikeDriver::group_interior_delta` + `Mode::Math` + `math_style()`; 7.9 acceptance: pylatexenc mathmode-suite parity incl. `\text`/`\mbox` text-mode resets) [N1] |
| `LatexExpressionParser` | `ExpressionParser` | implemented |
| `LatexOptionalSquareBracketsParser` | `OptionalGroupArgumentParser` | implemented |
| `LatexOptionalCharsMarkerParser` | `MarkerArgumentParser` (single literal marker) + `EmbellishmentsArgumentParser` (marker alternatives with followers) | implemented — the alternatives-without-follower slice deliberately not ported [N3] |
| `LatexOptionalEmbellishmentArgsParser` | `EmbellishmentsArgumentParser` | implemented (July 2026) [N3] |
| `LatexStandardArgumentParser`, `get_standard_argument_parser` | `ArgumentSpec` → `parse_declared_arguments` + standard `ArgumentParser`s; preset factory `latexlike::argument_specs` | implemented (excluded dispatch machinery; factory landed 7.7, deferred codes noted) [N8] |
| `LatexCharsGroupParser` | `CharsGroupArgumentParser` (`read_rigid_name_group` remains a different role) | implemented (July 2026) [N4] |
| `LatexCharsCommaSeparatedListParser` | `node::extract::split_at_chars` (+ `parse_keyval`) | implemented (7.8) — postprocessing helpers, per pylatexenc's own recommendation [N5] |
| `LatexTackOnInformationFieldMacrosParser` | `TackOnFieldsArgumentParser` | implemented (July 2026) — construct parser, per decision [N6] |
| `LatexVerbatimBaseParser` | `verbatim_state_delta` (the recipe as data) | implemented (7.7) [N7] |
| `LatexDelimitedVerbatimParser` | `VerbatimArgumentParser` | implemented (7.7) [N7] |
| `LatexVerbatimEnvironmentContentsParser` | `VerbatimBodyParser` + preset `VerbatimBehavior` | implemented (7.7) [N7] |

Type names user-reviewed July 2026 (N2–N6 session); NAMING_STRATEGY.md carries the
rows.

## Notes

### N1 — `GroupParser`: preset-pluggable interior state change (the `LatexMathParser` lesson)

Requirement decided (user, July 2026): a preset must have an *easy, pluggable* way to
attach a state change/event to a group class's interior — the motivating case is math
mode entering on the preset's math-group class (`$…$`, `\[…\]`). Direction: a
"contents parsing state / state-delta" plug.

Current state of affairs: `GroupParser` derives its interior state as
caller-resolved base + `expecting_group_close` (memoized
`ParserSession::group_interior_state`); the only shaping mechanism is
`ChildStateSpec`, which is a **per-use** call-site config and deliberately
one-level-deep (DESIGN_RATIONALE.md §3.6 decided semantics 3) — so it is *not* a
preset-wide mechanism. The plug's shape is an open Phase 7 design question; candidates:

- an optional interior `ParsingStateDelta` carried on `GroupRule` (data-first; travels
  with the very rule the tokenizer resolved for the open delimiter);
- routing interior-state derivation through a `Lang`/preset hook keyed on
  `GroupTypeId`.

`LatexMathParser`'s remaining jobs (inline/display record, math-node accessors) stay
preset business per ARCHITECTURE.md §nodes (no core math concept).

**Settled (July 2026, Phase 7 plan session): neither candidate.** The plug is the
`ParseDriver`'s `group_interior_delta(prev, rule)` hook (pure per `(state, rule)`, merged
into the memoized `group_interior_state` derivation), and the payload is a first-class
parsing-mode override (`StateData.mode: L::ModeId`, interpreted by
`Lang::finalize_transition`) — a math group's interior delta is one line of data.
`GroupRule` stays pure comparable data. DESIGN_RATIONALE.md §3.3/§3.6;
Phase7Execution.md D1/D2.

**Landed (7.5) and acceptance-verified (7.9):** the preset wiring shipped in 7.5
(`LatexlikeDriver::group_interior_delta`, single `GroupType::Math` class,
`MathStyle` off recorded delimiters); the 7.9 suite ports pylatexenc's
`test_get_latex_nodes_mathmodes`/`…_dollardollar` slice against it span-exactly,
including the `\text`/`\mbox` text-mode argument resets
(`ArgumentSpec::with_state_delta`) and the dollar-boundary tail pylatexenc's own
test leaves commented out.

### N2 — multi-delimited group parser

Decided (user): worth having ready-made even though it is wireable on `GroupParser` —
presets/libraries commonly use it as an argument parser (one position accepting any of
several delimiter pairs, e.g. `{}`/`[]`/`()`/`<>`, recording which pair matched).
Likely argument-parser shaped (sibling of `OptionalGroupArgumentParser`, minting
`GroupRule`s the same way). pylatexenc subtlety worth porting: the *group* parsing
state recognizes the whole delimiter list, while the *contents* state keeps only the
outer/default delimiters plus the pair actually encountered.

**Implemented (July 2026)** by folding into the existing types, per the user's inline
`### PhF` note (`Rules(Vec<…>)` supersedes the scalar `Rule`): `GroupArgumentParser`'s
rule form now holds a rules list (`with_rule` = one-element sugar, the `r<c1><c2>`
code unchanged; `any_of` = the multi-rule constructor, expression fallback off) and
`OptionalGroupArgumentParser` gains the same `any_of`. The contents-state subtlety
ports onto the temporary-groups lifecycle with two derivations (shared helper
`probe_minted_group`): probe under temporaries = all configured pairs; contents under
temporaries = the matched pair only (one shared derivation when a single rule is
configured — the 7.7 behavior unchanged). The matched pair is recorded on the staged
group node as always; brace protection comes out *stronger* than pylatexenc (the
stripping reaches any depth). Codes `AnyDelimited`/`AnyDelimitedOptional` wired in
`latexlike::argument_specs`, **list-form only** (whole elements; a compact string
reads `A` as an unknown code — pylatexenc likewise only uses them as whole `arg_spec`
strings); the optional flavor gets the `o`-code lone-brace-group unwrap.

### N3 — embellishment arguments parser (xparse `e{tokens}`-type)

Decided (user): ready-made. pylatexenc composes `LatexOptionalEmbellishmentArgsParser`
from the generalized `LatexOptionalCharsMarkerParser` (marker alternatives + a
following-argument parser + collect-marker-with-arg-as-group + `max_num_args`).
techy's `MarkerArgumentParser` covers only the single-literal-marker `*` case, so this
todo carries the generalization: marker alternatives (e.g. `^`, `_`, `'`), each marker
followed by an expression argument, repetition until no marker matches.

**Implemented (July 2026)** as `EmbellishmentsArgumentParser`, with the record-shape
question settled: **one `ParsedArgument`** for the whole position (per-marker entries
cannot express free source order through sequential per-spec parsing), each matched
pair staged as a classless wrapper `Group` (`GroupData::untyped`, open = the marker
span-backed, close empty — pylatexenc's `(marker, '')` shape), content designation =
the run of wrappers (interior noise included, leading noise excluded). Semantics
(user): each marker at most once (xparse), **longest match** among available markers
(diverging from pylatexenc's shortest-wins accumulate loop), multi-char markers
contiguous, follower hardwired to the expression core, and **marker + expression
atomic** — noise is allowed before a marker, and plain **whitespace** (only) between
a marker and its expression (revised July 2026; pylatexenc `allow_pre_space` parity,
staged inside the wrapper and filtered out of `split_embellishments` values); any
other separation — a comment, a paragraph break — rewinds the pair whole and ends
the run silently. Absent
is silent (`can_match_empty` true). By-marker reading is
`node::extract::split_embellishments` (a `KeyVals`: marker key, argument-nodes
value). `MarkerArgumentParser` stays single-literal — the alternatives-without-
follower slice of pylatexenc's generalized chars-marker parser has no consumer and
was deliberately not ported. `max_num_args` dropped (the at-most-once rule bounds the
run by the marker set).

### N4 — chars-group parser (node-staging)

Decided (user; the role split is already recorded in DESIGN_RATIONALE.md §3.6): a
parser distinct from `read_rigid_name_group`. The environment-name reader is
value-returning *scaffolding* (reconstructed, never recorded, §3.5) used by the
`\begin` composition; this parser instead **stages parsed nodes** for `\label{…}`-style
chars-only argument groups — a `{…}` group parsed under a restricted contents state
(commands/environments/math/specials off; comments and nested groups optionally on).

**Implemented (July 2026)** as `CharsGroupArgumentParser` (class-form opening,
mandatory, no expression fallback; content = group children). The restriction is
contents-only — leading noise scans under the outer state — and "math off" is
**data-driven**: with nested groups on, the contents keep only the base state's group
rules *of the entered class* (math pairs, being another class, drop away — no math
gate exists in the core); with nested groups off, `enable_groups` clears and the
ungated expected close still terminates (first close ends the group, pylatexenc's
`enable_groups=False` behavior for free). Descent (user, the `\cite{…,manual:{…rich
content…}}` case): nested group interiors **restore the outer state by default**
(pylatexenc behavior, carried by the `ChildStateSpec` chars-except-groups policy);
`with_restricted_descent` keeps the restriction at every depth instead. No argument
code — pylatexenc has none either; the parser is wired programmatically into specs
(and pairs naturally with N6's field specs).

### N5 — comma-separated chars list

Discarded as a construct parser — pylatexenc's own docstring recommends the
postprocessing route ("use a standard argument … and `split_at_chars()`"). Replacement
todo lives on the read/extraction side, not in `constructs/`: a
split-at-delimiter-chars helper over parsed children (cf. TODO_Big.md's
"Read/extraction API for content" item and pylatexenc's
`LatexNodeList.split_at_chars`).

**Implemented (Phase 7.8)** as `node::extract::split_at_chars` (segments = `NodeSlice`
views into a minted result tree; groups protect their interior, empty segments
dropped — pylatexenc defaults), plus `parse_keyval` (its `parse_keyval_content`,
no-knobs shape: source-ordered duplicate-preserving entries, `get` = last-wins,
`value_content()` lone-group unwrap accessor) and `content_as_chars`
(`get_content_as_chars` — strict, `Cow` fast path). pylatexenc's regex/callable
separator variants deliberately not ported (its own source marks the method
"untested code!"); the literal-separator form covers the recommended uses.

### N6 — tack-on information-field macros parser

Decided (user, July 2026): implement as a **construct parser**, *not* postprocessing.
Reasons recorded:

1. Postprocessing would require tree surgery exploring sibling nodes; the parser gets
   the association for free at parse time, attaching the `\label` calls directly to
   the `\section` invocation node.
2. Postprocessing would force `\label` to be a primary language command with defined
   behavior *everywhere*. Recognizing it only where a spec requests tack-ons makes it
   easy to disallow LaTeX's quirk of a `\label` placed anywhere attaching to
   *something*.

Shape (mirroring pylatexenc): after a construct's declared arguments, absorb a
specified set of trailing info-field macros — per-macro argument parsers (default: one
expression), multiplicity policy per macro name — and attach the parsed fields to the
construct's node.

**Implemented (July 2026)** as `TackOnFieldsArgumentParser` — an `ArgumentParser`
used as the callable's *last declared argument* (the FLM `label_arg` precedent), so
attachment is the argument's region and no invocation-parser change was needed.
Staged shape (user): **full `Callable` nodes**, not pylatexenc's
`(\label, '')`-group wrapper — the parser is configured with a `callable_type` and
per-name `Arc<dyn CallableSpec>`s (`with_field` / `with_repeatable_field`), and a
recognized `Command` token dispatches through `ParseDriver::make_invocation_parser`
with the configured spec, never touching the scope stack (decision reason 2 holds:
`\label` need not exist as a language command; per-name argument structure is just
the spec's `ArgumentSpec`s). Multiplicity is per field; a repeated non-repeatable
field diagnoses `RepeatedTackOnField` and — tolerant — is **parsed and kept** in the
region (diverging from pylatexenc's parse-and-discard: techy trees keep every byte).
Noise **between fields** is scanned as region noise (user; diverging from
pylatexenc, whose peek loop stops at a comment). Content designation = the run of
field nodes (leading noise excluded); by-name reading is
`node::extract::split_tack_on_fields` (a `KeyVals`: field name key, provided-argument
content value; a field providing no argument records no value).

### N7 — verbatim family — **implemented (Phase 7.7)**

Landed in `constructs::verbatim_parser` per the recipe (a features-disabled derived
state plus an `expecting_group_close` override; no char-level reader API, unlike
pylatexenc's `next_chars()`). The three pieces resolved as:

- **Base** → `verbatim_state_delta(rule)`: the recipe as a reusable delta builder
  (custom raw-content parsers start from it); the pluggable-stop-condition base *type*
  was not ported — two concrete parsers share one private loop.
- **Delimited** (`\verb|…|`) → `VerbatimArgumentParser`: auto-matched closing
  delimiter (`{`→`}`, `[`→`]`, `<`→`>`, `(`→`)`, else same char; table customizable)
  with the depth counter for paired delimiters; fixed-pair form for `v<c1><c2>`.
  Stages the group+chars shape, content = group children.
- **Environment contents** → `VerbatimBodyParser` (core, literal-terminator
  parameterized, produces `EnvironmentBody`) + `latexlike::VerbatimBehavior` (the
  `make_body_parser` override composing `\end{name}`); the gobbled newline is staged
  but designated out of the slot content (`EnvironmentBody.content`, added 7.7 —
  DESIGN_RATIONALE.md §3.13).

### N8 — the standard-argument factory (xparse-like string codes) — **implemented (7.7: `latexlike::argument_specs`)**

Decided (user, July 2026, Phase 7 plan session): `LatexStandardArgumentParser`'s code
interpretation is *not* replicated as a parser type. It becomes a plain constructor
**function** in the latexlike preset (landed as `argument_specs`): xparse-like
codes in — one code string per argument; compact whole-spec strings via
`argument_specs_from_str` (July 2026 revision) — configured
`Arc<dyn ArgumentParser<L>>` out, resolved eagerly at spec-construction time — parser choice depends only on the code, never on parse-time
facts. No wrapper indirection; `get_standard_argument_parser`'s flyweight cache
dissolves (parameterless codes may return shared singletons — specs are built once per
language, not per parse). The explicit-parser escape hatch is untouched
(`ArgumentSpec` accepts any parser; the factory is convenience, never a requirement).
A malformed code is embedder input: `Err`, not panic. Preset placement is forced by
content — the codes embody LaTeX spellings and the configured parsers need the
preset's group types.

Per-code mapping (the string codes are worth accepting verbatim: pylatexenc's default
spec database — the Phase 7 std-library porting target — is written in them, as are
FLM's feature definitions):

| code | parser | status |
|---|---|---|
| `m` / `{` | `GroupArgumentParser` (class form, `Content`) | implemented — factory wired 7.7. *Refines the survey's `ExpressionParser` row:* the class parser is the decided parse-time realization of `'{'` + `unwrap_double_group` (content = group children) and keeps `ExpressionParser` as its fallback engine; the `expression_single_token_requiring_arg_is_error` switch is absorbed by the emptiness surface |
| `o` / `[` | `OptionalGroupArgumentParser` (+ lone-`{…}` unwrap) | implemented — factory wired 7.7 |
| `s` / `*`, `t<char>` | `MarkerArgumentParser` | implemented — factory wired 7.7 (`t` = same parser, other marker char) |
| `r<c1><c2>` / `d<c1><c2>` | `GroupArgumentParser::with_rule` (7.7: the mandatory minted-rule form, no expression fallback by default — `with_expression_fallback` opts in, a techy extension; the same knob turned off on the class form is the other extension pylatexenc cannot spell) / `OptionalGroupArgumentParser` with per-use delimiters | implemented (7.7) — the second consumer of `TokenRules::temporary_groups` |
| `e{<chars>}` | `EmbellishmentsArgumentParser` | implemented (July 2026) — record shape settled: one `ParsedArgument`, wrapper groups inside [N3]; both factory forms (`e` + immediate `{…}`, no whitespace inside) |
| `v` / `v<c1><c2>` | `VerbatimArgumentParser` | implemented (7.7) — `v` alone = autodetected delimiter; in a compact whole-spec string, `v` takes two delimiter chars exactly when directly followed by a non-whitespace char (`argument_specs_from_str`'s disambiguation rule; the list form needs none) |
| `AnyDelimited` / `AnyDelimitedOptional` | `GroupArgumentParser::any_of` / `OptionalGroupArgumentParser::any_of` (default pairs `{} [] () <>`, content class) | implemented (July 2026) — list-form-only word codes [N2] |

Constructor knobs that do **not** carry over — the factory is deliberately thinner
than the original: `return_full_node_list` (superseded by parser-designated
`ContentNodes`), `allow_pre_space` (the regions machinery records pre-argument
whitespace as noise nodes), the single-token-error switch (emptiness surface).

Landed 7.7 as `latexlike::argument_specs` (`Err(ArgumentCodeError)` on malformed
codes); `e{…}` [N3] and `AnyDelimited` [N2], deferred beyond Phase 7 at the plan
session, landed in the July 2026 N2–N6 session. Revised July 2026 (user): the primary
signature takes one code string per argument (`argument_specs(["o", "m"])`); the
compact concatenated form is the twin `argument_specs_from_str` — pylatexenc's spec
database and FLM's feature definitions stay directly portable through it
(DESIGN_RATIONALE §3.13).
