# Action 01 — Structured errors and diagnostics

**Status: steps 1–4 implemented (July 2026) — in working tree, uncommitted; awaiting user
review. Step 5 deferred; final naming/identifier pass still open.**

Decision record with full rationale: DESIGN_RATIONALE.md §3.8 (July 2026 entries, from
"Structured diagnostics: condition payloads, not prose" onward). This file previously held
the analysis report that proposed promoting `ParseErrorKind` enum variants; that proposal is
**superseded** by the design below (the report's producer-site inventory survives, corrected,
in the inventory section).

## Target design

### Traits (`error.rs`)

```rust
/// Implementor-facing: implemented on plain public-field data structs.
pub trait DiagnosticInfo:
    Any + Clone + fmt::Display + fmt::Debug + Send + Sync
{
    /// Wire/config identity, namespaced "<crate-or-lang>.<area>.<condition>".
    /// Semver-stable; deliberately decoupled from the type/module name.
    const IDENTIFIER: &'static str;

    /// Serialization-boundary projection only — never a programmatic access path
    /// (consumers downcast to the concrete type instead).
    fn serializable_data(&self) -> DiagnosticValue { DiagnosticValue::empty_map() }
}

/// Dyn-compatible facade. Sealed: the blanket impl over DiagnosticInfo is the only way in.
pub trait DiagnosticData: Any + fmt::Display + fmt::Debug + Send + Sync /* + Sealed */ {
    fn identifier(&self) -> &str;
    fn serializable_data(&self) -> DiagnosticValue;
    fn clone_box(&self) -> Box<dyn DiagnosticData>;
}
impl<T: DiagnosticInfo> DiagnosticData for T { /* IDENTIFIER, delegate, Box::new(self.clone()) */ }

/// Minimal alloc-only value tree for the serialization boundary (decided: no float
/// variant — serialize floats as strings if ever needed).
pub enum DiagnosticValue {
    Null, Bool(bool), Int(i64), Str(String),
    List(Vec<DiagnosticValue>), Map(Vec<(String, DiagnosticValue)>),
}
```

### Carriers

```rust
pub struct Diagnostic<O: SourceOrigin = Option<String>> {
    severity: Severity,
    data: Box<dyn DiagnosticData>,
    span: SourceSpan<O>,
    frames: Vec<TraceFrame<O>>,     // traceback snapshot, innermost first
}
pub struct ParseError<O: SourceOrigin = Option<String>> {
    data: Box<dyn DiagnosticData>,
    span: SourceSpan<O>,
    frames: Vec<TraceFrame<O>>,
}   // still implements core::error::Error
```

- **No `message` field and no string constructors.** `message()` renders the payload's
  `Display` on demand (returns `String`). Construction takes `impl DiagnosticInfo`.
- `Clone` via `clone_box`; the planned `PartialEq`/`Eq` derives are dropped — tests compare
  `identifier()` and/or `downcast_ref` fields.
- `render()` = message + position + `format_traceback(&frames)` + provenance chain. The
  `@ char pos N` fallback gains a parenthetical explaining that line info was unavailable
  (line-index scan limit) — carried over from the original report.
- `Diagnostics` gains identifier-filtering and downcast-based accessors.

### The recover funnel

`ParseContext::recover(condition: impl DiagnosticInfo, span)` becomes the single producer:

1. box the condition;
2. `L::refine_diagnostic(boxed, &cx.state)` (default: identity) — the funnel is at the
   `ParseContext` level because refinement needs the state;
3. snapshot the session's live frame stack into `Vec<TraceFrame<O>>` (titles rendered here,
   on the cold path);
4. `Tolerant` → push `Diagnostic { Error, … }`, return `Ok(())`; `Strict` →
   `Err(ParseError { … })`.

### Frames

```rust
struct Frame<L: Lang> {              // live stack entry: allocation-free (Arc bumps only)
    title: FrameTitle<L>,
    span: SourceSpan<L::SourceOrigin>,
}
enum FrameTitle<L: Lang> {           // mechanisms, NOT a construct taxonomy
    Static(&'static str),
    Quoted { label: &'static str, name: SourceSpan<L::SourceOrigin> },
    Callable { spec: Arc<dyn CallableSpec<L>>, role: FrameRole },   // exact shape: open
}
pub struct TraceFrame<O: SourceOrigin> {  // snapshot: L-free, or L re-enters Diagnostic
    title: String,
    span: SourceSpan<O>,
}
```

- `Vec<Frame<L>>` lives on `ParserSession<L>`; pushed/popped by closure-scoped
  `cx.with_frame(frame, |cx| …)` (RAII guard impossible: it would hold `&mut cx` against
  the body).
- Push sites: invocation dispatch, argument parsing (argument #N), group interior,
  environment body.
- New defaulted, dyn-compatible hook `CallableSpec::stack_frame_title(&self, role) -> String`
  produces callable titles at snapshot time.
- `format_traceback` is reshaped to take `&[TraceFrame<O>]` — it finally has a producer.
- Primary diagnostic spans keep today's positions; the traceback supplies the second
  location (this resolves the original report's caret-in-the-wrong-place item; the
  `related: Vec<(SourceSpan, String)>` proposal is subsumed).

### `Lang` hooks

```rust
fn refine_diagnostic(
    data: Box<dyn DiagnosticData>,
    state: &ParsingState<Self>,
) -> Box<dyn DiagnosticData> { data }
```

(`diagnostic_catalog()` was considered and **dropped** — see finalized decisions below.)

### Token layer

- `EndOfStreamAfterEscape { escape_char: char }` and `ForbiddenChar { ch: char }` become
  standalone condition structs (each `DiagnosticInfo`), wrapped by the `TokenErrorKind`
  variants; the enum gains `Custom(Box<dyn DiagnosticData>)` for `Lang::scan_specials`,
  loses `Copy`, and `TokenError::kind()` returns `&TokenErrorKind`.
- Lift into diagnostics: built-ins boxed, `Custom` unwrapped.
- `ParseError::from_token_error(…)` named constructor replaces the lift duplicated at
  `constructs/mod.rs` (`try_peek`) and `nodes_parser.rs` (content-loop recovery arm).

## Condition-type inventory (initial; names/identifiers tentative)

Library-proper producer sites — the original report's inventory, corrected:
`environment_parser.rs:512/539/661` sit in `#[cfg(test)]` fixture code (`EnvLang`) and become
demonstrations of *third-party* `DiagnosticInfo` impls, not library conditions.

| Type | identifier | fields | producer sites |
|---|---|---|---|
| `UnclosedGroup` | `core.group_parser.unclosed-group` | `expected_close: String`, `found: EndOfInput \| StrayClose` | group_parser.rs:123, 137 |
| `EnvironmentTerminatorMismatch` | `core.environment_parser.terminator-mismatch` | `expected: String`, `found: String` | environment_parser.rs:253 |
| `MalformedEnvironmentTerminator` | `core.environment_parser.malformed-terminator` | `environment: String` | environment_parser.rs:272 |
| `MissingEnvironmentTerminator` | `core.environment_parser.missing-terminator` | `environment: String`, `found: …` | environment_parser.rs:323, 338 |
| `UnresolvableCommand` | `core.nodes_parser.unresolvable-command` | `name: String`, `escape_char: char` | nodes_parser.rs:430, 646; argument_parsers.rs:235 |
| `ExpressionCallableTakesArguments` | `core.nodes_parser.expression-takes-arguments` | `callable: String` | nodes_parser.rs:2418; argument_parsers.rs:276 |
| `MissingMandatoryArgument` | `core.argument_parsers.missing-mandatory-argument` | `argument_name: Option<String>` | argument_parsers.rs:452 |
| `ExpectedExpressionArgument` | `core.argument_parsers.expected-expression-argument` | `argument_name: Option<String>` | argument_parsers.rs:361 |
| `UnusableRecoveryToken` (name TBD) | `core.nodes_parser.unusable-recovery-token` | TBD | nodes_parser.rs:659, 686 (recovery placeholders) |
| `EndOfStreamAfterEscape` | `core.token.end-of-stream-after-escape` | `escape_char: char` | token layer |
| `ForbiddenChar` | `core.token.forbidden-char` | `ch: char` | token layer |

Notes:

- Identifiers follow the provisional scheme (user, July 2026): `core.<area>.*` for library
  conditions (areas mirror today's modules), `<preset-name>.<namespaced-name>` for presets
  and downstreams. The strings are frozen independently of future code moves — if the
  environment helper later moves to a preset crate, its identifiers stay.
- No `callable_name` field on the argument conditions: the frame stack renders
  "argument #N of ‘\abc’", so threading the name into the payload is unnecessary.
- All type names and identifiers get a NAMING_STRATEGY pass before landing — identifiers
  are semver surface from the moment they ship.

## Implementation steps

1. **Core types + all producer sites (one change — cannot be split).** New traits,
   `DiagnosticValue`, reshaped `Diagnostic`/`ParseError`/`Diagnostics`, the recover funnel,
   and the condition types from the inventory replacing every
   `ParseErrorKind::Syntax { message: format!(…) }` site. Atomic because the string
   constructors are removed. Includes `render()` rework, dropped `PartialEq`, test-idiom
   migration (identifier + downcast), and the `#[cfg(test)]` fixture parsers rewritten as
   third-party-style `DiagnosticInfo` demonstrations.
2. **Frame stack.** Session field, `with_frame`, threading through the descent points,
   snapshot in the funnel, `CallableSpec::stack_frame_title`, `format_traceback` reshape.
3. **`Lang::refine_diagnostic`** hook (default identity).
4. **Token layer.** Struct-per-variant, `Custom`, `Copy` loss, `from_token_error` dedup;
   plus the `try_peek` recoverability one-liner from the original report (mirror the content
   loop: an unrecoverable token error must abort even under `Tolerant`).
5. **Deferred (non-breaking to add later):** `serializable_data()` impls (the method is
   defaulted), a JSON emission helper over `DiagnosticValue`, and the
   `#[derive(DiagnosticInfo)]` proc-macro (generates identifier/message/serialization keys
   from the struct; build-dep only).

Steps 2–4 are independent of each other; all depend on step 1.

## Superseded / dropped from the original report

- `ParseErrorKind` variant promotion and `Option<ParseErrorKind>` on `Diagnostic` →
  superseded by dyn condition payloads.
- `related: Vec<(SourceSpan, String)>` on both error types → subsumed by the frame stack.
- `PartialEq`/`Eq` derives → dropped (dyn payloads; tests compare identifier/downcast).
- `Error::source()` chain via `TokenErrorKind` → moot (payloads are not `Error`s;
  `ParseError` remains the `core::error::Error`).
- Carried over unchanged: `from_token_error` dedup, `try_peek` recoverability fix,
  `@ char pos` opacity parenthetical.

## Finalized decisions (user, July 2026)

1. **MSRV: bump `rust-version` to 1.86** (dyn trait upcasting to `dyn Any`); update the
   Cargo.toml comment (1.81 was pinned for `core::error::Error`). No `as_any()` method.
2. `FrameTitle` variants as sketched above.
3. `DiagnosticValue`: barebones set as sketched, **no float variant** — serialize as a
   string if ever needed.
4. Identifier scheme (provisional): `core.<area>.*` for library conditions,
   `<preset-name>.<namespaced-name>` for presets/downstreams — see inventory.
5. **`diagnostic_catalog()` dropped** — maintenance work to keep in sync; namespaced
   identifiers already prevent collisions. Can be added later without breakage.

Still open: a final naming/identifier pass (NAMING_STRATEGY) before a public release makes
the identifier strings semver surface.
