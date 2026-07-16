# Parser Library Parity — pylatexenc `latexnodes.parsers` vs techy `constructs`

Status: survey settled July 2026 (parser-library survey session). Companion decision
entry: DESIGN_RATIONALE.md §3.6 ("parser-library gap list"). Overlapping informal
items in TODO_Big.md ("Parsers that are worth implementing", "Delimited Group Parser
Helper Utility") are subsumed by the table below.

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
| `LatexDelimitedGroupParser` + `…ParserInfo` | `GroupParser` | implemented — plus todo: preset-pluggable interior state delta [N1] |
| `LatexDelimitedMultiDelimGroupParser` + `…ParserInfo` | — | todo [N2] |
| `LatexMathParser` | `Group` node under a preset math group class + state event | absorbed (core, by design) / todo (Phase 7 preset wiring) [N1] |
| `LatexExpressionParser` | `ExpressionParser` | implemented |
| `LatexOptionalSquareBracketsParser` | `OptionalGroupArgumentParser` | implemented |
| `LatexOptionalCharsMarkerParser` | `MarkerArgumentParser` | implemented (single-marker case; full generality folds into [N3]) |
| `LatexOptionalEmbellishmentArgsParser` | — | todo [N3] |
| `LatexStandardArgumentParser`, `get_standard_argument_parser` | `ArgumentSpec` → `parse_declared_arguments` + standard `ArgumentParser`s | implemented (excluded dispatch machinery) |
| `LatexCharsGroupParser` | — (`read_rigid_name_group` is a different role) | todo [N4] |
| `LatexCharsCommaSeparatedListParser` | — | discarded as parser → todo: node-list split helper [N5] |
| `LatexTackOnInformationFieldMacrosParser` | — | todo — construct parser, decided [N6] |
| `LatexVerbatimBaseParser` | recipe validated test-side only (`RawBlockParser`) | todo, Phase 7 [N7] |
| `LatexDelimitedVerbatimParser` | — | todo, Phase 7 [N7] |
| `LatexVerbatimEnvironmentContentsParser` | — (takeover-hatch demo is test-only) | todo, Phase 7 [N7] |

All techy type names for **todo** rows are placeholders pending a NAMING_STRATEGY.md
review with the user.

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

### N2 — multi-delimited group parser

Decided (user): worth having ready-made even though it is wireable on `GroupParser` —
presets/libraries commonly use it as an argument parser (one position accepting any of
several delimiter pairs, e.g. `{}`/`[]`/`()`/`<>`, recording which pair matched).
Likely argument-parser shaped (sibling of `OptionalGroupArgumentParser`, minting
`GroupRule`s the same way). pylatexenc subtlety worth porting: the *group* parsing
state recognizes the whole delimiter list, while the *contents* state keeps only the
outer/default delimiters plus the pair actually encountered.

### N3 — embellishment arguments parser (xparse `e{tokens}`-type)

Decided (user): ready-made. pylatexenc composes `LatexOptionalEmbellishmentArgsParser`
from the generalized `LatexOptionalCharsMarkerParser` (marker alternatives + a
following-argument parser + collect-marker-with-arg-as-group + `max_num_args`).
techy's `MarkerArgumentParser` covers only the single-literal-marker `*` case, so this
todo carries the generalization: marker alternatives (e.g. `^`, `_`, `'`), each marker
followed by an expression argument, repetition until no marker matches.

### N4 — chars-group parser (node-staging)

Decided (user; the role split is already recorded in DESIGN_RATIONALE.md §3.6): a
parser distinct from `read_rigid_name_group`. The environment-name reader is
value-returning *scaffolding* (reconstructed, never recorded, §3.5) used by the
`\begin` composition; this parser instead **stages parsed nodes** for `\label{…}`-style
chars-only argument groups — a `{…}` group parsed under a restricted contents state
(commands/environments/math/specials off; comments and nested groups optionally on).

### N5 — comma-separated chars list

Discarded as a construct parser — pylatexenc's own docstring recommends the
postprocessing route ("use a standard argument … and `split_at_chars()`"). Replacement
todo lives on the read/extraction side, not in `constructs/`: a
split-at-delimiter-chars helper over parsed children (cf. TODO_Big.md's
"Read/extraction API for content" item and pylatexenc's
`LatexNodeList.split_at_chars`).

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

### N7 — verbatim family (Phase 7, per ARCHITECTURE.md §constructs)

The recipe is validated test-side (`RawBlockParser` in `environment_parser.rs` tests):
a features-disabled derived state plus an `expecting_group_close` override makes the
body arrive as per-byte `Char` tokens and the terminator as one `GroupClose` — no
char-level reader API is needed, unlike pylatexenc's `next_chars()`. Three pieces:

- **Base** (`LatexVerbatimBaseParser` analog): reusable raw-content reading under the
  verbatim state, stop condition pluggable.
- **Delimited** (`\verb|…|`): auto-matched closing delimiter (`{`→`}`, `[`→`]`,
  `<`→`>`, `(`→`)`, else same char) with a depth counter for paired delimiters.
- **Environment contents** (`verbatim` environment body): in scope per refined
  instructions — a production, reusable body parser plugging in via the Phase 7
  `EnvironmentSpec` body-parser hook (§3.6 `make_body_parser` leaning), including
  gobbling the single newline after `\begin{verbatim}`.
