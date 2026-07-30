# Friction log — extender persona (T2)

Persona: knows LaTeX, wants to teach the parser custom macros/environments, does not
want to learn parser internals. Bar: "for an extender it should be simple."

Sources used, outside-in: README.md, docs/guide.md, docs/learn-by-example.md,
docs/concepts-overview.md; then rustdoc-level material (doc comments + public
signatures) in `techy/src/**` where the guide ran out. No implementation bodies were
needed at any point (see "Doc gaps" for where signature-grepping *was* needed).

Headline: **the happy path is genuinely good.** All five tasks compiled on the first
`cargo build` and passed on the first run, written almost entirely by imitating
docs/learn-by-example.md. The friction below is real but mostly second-order:
footguns, missing conveniences, and doc gaps off the happy path.

---

## Task 1 — custom macro `\greet[greeting]{name}`

**What worked**: `Package::new` + `insert(CallableType::Macro, "greet",
Arc::new(MacroSpec::new(argument_specs(["o","m"])?)))` +
`Language::default().with_provider(...)`. Copy-adapted from learn-by-example;
compiled first try. Argument codes (`o`, `m`) are exactly the xparse vocabulary a
LaTeX user already knows — excellent choice.

**Friction**:

1. **[FOOTGUN — worst finding] Registering `"\greet"` (with backslash) fails
   silently.** Probe A: `package.insert(CallableType::Macro, r"\greet", ...)` is
   accepted without complaint; at parse time the user gets
   `cannot resolve command '\greet' (searched providers: p, base)` — actively
   misleading, since package `p` *does* contain `\greet` (under the wrong key).
   The rule "register the bare name, no backslash" is shown only by example in the
   guide; the `Package::insert` doc says `name` is the "normalized spelling"
   without defining it (the definition — "normalization is the caller's (preset's)
   business" — lives on `CallableQuery::name` in `scopes`, where no extender will
   look). Insert-time validation (or at least a loud latexlike-level doc line, or
   the resolve error mentioning near-miss keys) would kill this footgun.

2. **Registration ceremony is deep.** The everyday line nests four constructors and
   two failure modes:
   `Arc::new(MacroSpec::new(argument_specs(["o","m"]).unwrap()))` inside
   `insert(CallableType::Macro, "greet", ...)`. Vocabulary needed: `Package`,
   `CallableType`, `MacroSpec`, `argument_specs`, plus `Arc` (std). It is all
   learnable, but a one-liner for the dominant shape — something like
   `package.define_macro("greet", "om")?` — would cover ~90% of real extender
   definitions with two names instead of five and no `Arc` in sight.

3. **`Arc` is the extender's problem everywhere.** Every spec and every package must
   be hand-wrapped. Understandable design (sharing across languages/scopes), but it
   is parser-plumbing showing through in what should be declarative definition code.

4. **Absent optional vs. out-of-range are conflated.** Probe D:
   `argument_content_nodes(0)` (absent optional) and `argument_content_nodes(7)`
   (no such argument) both return `None`. To distinguish you must go through
   `arguments().get(i).is_provided()`. Nothing in the guide or the method name
   warns about this; a typo'd index silently reads as "argument absent".

5. **Zero-argument macros have no nice spelling.** `MacroSpec::new(Vec::new())`
   works; `argument_specs([])` needs a turbofish (`argument_specs::<[&str; 0]>([])`)
   to infer. `\qed`-style no-arg macros are the most common macro shape in the wild;
   a `MacroSpec::empty()` (or `Default`) would help. (Probe F.)

6. Minor: naming arguments for by-name access doesn't compose with the code factory.
   `ArgumentSpec::named` exists, and `argument_content_nodes_named("greeting")`
   works beautifully — but `argument_specs` returns `Vec<Arc<ArgumentSpec>>`, so
   attaching names means rebuilding each spec around its (fortunately public)
   `.parser` field (Probe I). A `(code, name)` variant of the factory, or codes like
   `argument_specs([("o", "greeting"), ("m", "name")])`, would make names the easy
   default the docs themselves recommend ("names stay valid, positions renumber").

## Task 2 — custom environment `\begin{theorem}[title]...\end{theorem}`

**What worked**: perfectly symmetric with macros —
`insert(CallableType::Environment, "theorem", Arc::new(EnvironmentSpec::new(...)))`;
`node.environment_name()`, `node.argument_content_nodes(0)`, `node.body()`. First
try. The symmetry macro/environment/specials is the API's best structural property.

**Friction**:

1. `body()` returning `Option<NodeSlice>` was easy to guess, but nothing in the
   guide says what `None` means for an environment node (can it be `None`?
   empty-body vs. not-a-body-carrier?). Small doc gap.
2. Three parallel name getters (`macro_name`, `environment_name`,
   `specials_name`) and no generic "give me the callable's name". Fine for
   task code, mildly annoying for generic tooling over mixed trees.
3. Probe B/C: the `MacroSpec`/`EnvironmentSpec` ↔ `CallableType` pairing is pure
   convention — a `MacroSpec` registered under `CallableType::Environment` parses
   fine and yields an `Environment(...)` node, and vice versa. Nothing checks, and
   the docs don't say what a mismatch means. Either document the interchangeability
   as a feature or catch it; today it reads as an accident waiting for a confused
   bug report.

## Task 3 — bundling definitions as a reusable package

**What worked**: `Package` *is* the reusable unit — build once, `Arc` it, share it
across any number of `Language`s via `with_provider`. A plain
`fn my_notation_package() -> Arc<Package<Latexlike>>` was all the "package
authoring" story I needed. Shadowing story ("innermost provider wins, that's the
whole model") is refreshingly simple.

**Friction**:

1. **Two documented ways to activate a package.** learn-by-example uses
   `Language::with_provider(...)`; the `argument_specs` rustdoc example uses
   `Language::with_seed_delta(ParsingStateDelta::new().push_provider(...))`. They
   are the same thing (`with_provider` is declared sugar), but an extender reading
   both pages sees two idioms and wonders which is canonical. The rustdoc examples
   should use the sugar everywhere.
2. `with_provider` returns `Result` that "cannot fail today" (its own words) — one
   more `.unwrap()`/`?` of pure ceremony on every setup. Defensible
   (transition machinery), but it is the kind of ceremony extenders notice.
3. Parameter-order inconsistency in the `Package` API: `insert(callable_type, name,
   spec)` vs `insert_specials(trigger, callable_type, spec)` — the key moves from
   second to first position between the two siblings. Trips up muscle memory.
4. No `with_providers([...])` for several packages; chained `with_provider` calls
   each with their own `?`. Minor.

## Task 4 — wrong usage: what do diagnostics look like?

**What worked**: this is a strong area. Strict mode: `parse` returns `Err` whose
`Display` is a clean one-liner, `identifier()` is a stable machine name
(`core.argument_parsers.missing-mandatory-argument`), and `render()` adds an
"Open blocks" pseudo-traceback (`argument #2 of macro '\greet'` → `macro '\greet'`)
that tells the end user exactly where they are. Tolerant mode: same condition as a
`Diagnostic` on `result.diagnostics`, with severity, span, and a recovered tree that
keeps the `\greet` node with the argument recorded absent. Missing `\end{theorem}`
(Probe H) renders equally well. An extender can hand these to their users nearly
as-is.

**Friction**:

1. **The single-expression fallback will ambush extenders.** `\greet word` is *not*
   an error: the mandatory `m` argument silently takes `w` (TeX-faithful,
   `\frac12`-style). The only warning is half a sentence in the `argument_specs`
   code table; learn-by-example never mentions it. Extenders defining
   `\greet{name}`-style macros will expect "missing braces" to be diagnosed. There
   is also **no argument code for "mandatory braced group, no fallback"** (`r` only
   comes delimiter-parameterized, and `r{}` mints custom-delimiter semantics rather
   than the standard group parser) — a missing convenience for exactly this, or at
   least a prominent doc callout.
2. Cosmetic: rendered positions read `at: @ (line 1, col 11)` — the `@` placeholder
   for an anonymous source origin looks like a formatting bug to an end user.
3. The span of "missing mandatory argument" is the empty range at end of input
   (`10..10`); fine, but `Diagnostic::span().content()` on an empty span gives `""`,
   so anyone printing "offending text" gets nothing — the render()'s
   open-blocks frames are the useful part. Worth one doc line.

## Task 5 (stretch) — a definition active only inside one environment

**What worked**: expressible, and short — the environment's spec pushes an inner
package for the body's extent:
`EnvironmentSpec::new(...).with_body_delta(ParsingStateDelta::new().push_provider(inner))`.
Six lines; `\qed` resolves inside `{proof}` and is `cannot resolve command` outside.
Genuinely impressive that lexical scoping of definitions falls out this cleanly.

**Friction**:

1. **State-machinery vocabulary leaks into extender code.** To say "these commands
   exist only inside `proof`" I had to import `techy::state::ParsingStateDelta` and
   compose a "parsing-state delta" with a "provider push" — parser-internals
   language for a packaging wish. `Language` got the sugar (`with_provider` over
   `with_seed_delta`); `EnvironmentSpec` did not. A symmetric
   `EnvironmentSpec::with_body_provider(Arc<Package>)` would keep `state::` out of
   extender code entirely.
2. **Discoverability**: nothing in the guide shows this pattern. learn-by-example
   demonstrates `with_body_delta` only for mode changes (`equation` → math). I
   found the combination by grepping public signatures of
   `latexlike/environments.rs` and `state/delta.rs` and *guessing* that
   `push_provider` composes with `with_body_delta`. It worked first try — the
   model is coherent — but the guide should show it (it is a classic LaTeX wish:
   `\item` inside lists, `&` inside tabular).

## Cross-cutting observations

- **Doc gaps (each cost trial-and-error or signature-grepping, never a body):**
  - the bare-name registration rule (Task 1.1);
  - absent-vs-out-of-range `None` (Task 1.4);
  - scoping definitions to an environment body (Task 5.2);
  - what a `MacroSpec`-under-`Environment` mismatch means (Task 2.3);
  - `concepts-overview.md` is a self-declared skeleton and `parsing-model` a stub —
    everything not in learn-by-example is rustdoc-only today.
- **Internals leakage observed in the docs** (not needed for my five tasks, but on
  the extender path): the guide's own `\text{...}` example — "argument parses in
  text mode", a top-five LaTeX macro shape — requires
  `constructs::GroupArgumentParser`, `state::TokenRulesOverrides`,
  `latexlike::default_token_rules`, and `GroupType`. That is four internal
  vocabularies for one everyday definition; a canned
  `text-mode argument` helper in `latexlike` would fit the preset's job
  description.
- **Redundancies** (mostly benign, all documented): `argument_specs` vs
  `argument_specs_from_str` (deliberate, fine); `with_provider` vs
  `with_seed_delta` + `push_provider` (sugar, but doc examples disagree);
  `insert` / `insert_in_modes` / `insert_specials` / `insert_specials_in_modes`
  (fine, but see the parameter-order nit).
- **Names**: mostly guessable. `Package`, `body()`, `arguments()`,
  `environment_name()` — found on first guess. `CallableType` needed the guide.
  `argument_content_nodes` vs `argument_nodes` — the content/delimiters
  distinction is real but the names alone don't teach it. `latexlike` items are
  *not* re-exported at the crate root while core items are — defensible layering,
  but the extender's most-used module is the one that needs full paths.
