# FRICTION — language-designer walkthrough (T3, "notely")

Persona: an advanced user building their own small markup language ("notely") on
techy's generic machinery, outside-in (README, docs/ guides, doc comments + public
signatures in techy/src/**; implementation bodies off-limits, reads logged below).

notely: commands `@name(...)`, running-text groups `[...]`, argument-only groups
`(...)` (minted per-argument), line comments `# ...`, a `-->` specials shorthand,
NO math mode. All five tasks plus the specials probe completed; everything
compiled on the FIRST `cargo build` and all assertions passed after one trivial
fix (guessed diagnostic-identifier strings). That first-compile success is itself
a headline datum: the contracts stated in doc comments were sufficient to write
~540 lines of working extension code without trial-and-error — the friction below
is almost entirely about *finding and assembling* the recipe, not about the API
lying or breaking.

## Task 1+2 — defining the language (lang.rs, 154 lines incl. specials wiring)

Minimal concept set a designer MUST hold before the first line compiles:

1. `Lang` — the 9-associated-type compile-time bundle (and why `SimpleLang` is
   not usable here, see below);
2. the `Lang` / `ParseDriver` split (static hooks vs. instance behavior) — needed
   just to know *where* command resolution goes;
3. `TokenRules` + its four rule types (`GroupRule`, `CommandRule`, `CommentRule`,
   `WhitespaceRules`) and the gate-vs-data "two spellings of off" doctrine;
4. `StateData` (rules + scopes + mode + ext) and `initial_state_data()` as the
   seed;
5. `ScopeStack` / `Package` / `SpecsProvider` for definitions;
6. `Language` (`new` / `with_provider` / `parse`) and `Recovery`;
7. `CommandResolution::resolve_via_scopes` — the single most load-bearing helper.

That is ~7 concept clusters across 5 modules for "hello world in my own syntax".
Each individual item is well documented where it lives; the *recipe ordering*
(Lang → seed rules → driver with resolve_command → package → Language) is
documented nowhere outside-in.

Specific friction:

- **F1 — No language-designer guide chapter (the worst gap).** README and both
  guide pages are 100% latexlike-preset; `concepts-overview.md` is a labeled
  skeleton; `parsing-model.md` is a 57-byte stub. The only end-to-end custom-Lang
  examples in the repo live in `#[cfg(test)]` modules (engine/mod.rs `PlainLang`
  / `ObserverLang`, engine/language.rs `DocLang` / `MacroLang`) — invisible in
  rendered rustdoc. "Custom LaTeX-like languages are defined with the same
  machinery" is the crate's opening promise; no public page demonstrates it.
- **F2 — `SimpleLang` dead-ends exactly at this persona.** Its defaulted driver
  (`StdParseDriver`) resolves no commands, so a `SimpleLang` language that
  enables command syntax can only ever produce unresolvable-command conditions.
  The blanket impl makes `SimpleLang` and a custom driver mutually exclusive, so
  the upgrade path is discontinuous: from `impl SimpleLang for X {}` (1 line) you
  jump to 9 associated types + a driver type + `initial_state_data` (~80 lines).
  Mitigation that worked: the default `resolve_command`'s detail string names the
  fix ("implement ParseDriver::resolve_command or use a preset") — good failure
  design. Still, a mid-tier (e.g. SimpleLang with overridable Driver type only)
  or at least a guide note "SimpleLang is for command-less/test languages" would
  save the false start.
- **F3 — Seed-state boilerplate.** `TokenRules` has 13 mandatory fields, no
  `Default` (deliberate and documented — no privileged LaTeX values), so every
  language spells all 13 plus `StateData`'s 4. The `Lang::initial_state_data`
  *default body* literally constructs the all-off neutral value, but that value
  is not reachable as data — you cannot call-and-tweak; you copy it from the doc.
  A `StateData::neutral()` / `TokenRules::disabled()` constructor would cut ~20
  lines of every language and remove a transcription-error class. (Everything
  else about rules-as-data was pleasant: `@` as escape char, `#` comments, `[...]`
  groups, and "no math" were pure data choices — zero code, exactly as promised.)
- The `Copy + Ord + Hash + …` bound sets on the id vocabularies are stated on
  each associated type's docs — good — but you only discover a missing `Ord` on
  `CallableTypeId` at the compile error (mine had it from reading the doc; a
  guide snippet would make the derive lists copy-pasteable).

## Task 3 — two declarative callables (specs.rs, 52 lines)

Pure configuration, as intended: `Package::new` + `insert` +
`StdCallableSpec::new(vec![ArgumentSpec::new(...)])` +
`GroupArgumentParser::with_rule(paren_rule())`. Named arguments and by-name
access (`argument_content_nodes_named("url")`) worked first try.

- The generic layer has no equivalent of latexlike's `argument_specs(["o","m"])`
  code factory — expected (codes are preset vocabulary), and the long form is
  ~4 lines/argument; acceptable, worth a guide example.
- `GroupArgumentParser::with_rule` vs `new(class)`: the `with_` prefix reads as
  an augmenting builder but is an alternative *constructor*; found only by
  scanning the impl block. Naming nit.
- The class-form/rule-form + expression-fallback matrix on `GroupArgumentParser`
  is genuinely well documented (defaults per form, pylatexenc parity notes) —
  once found. Discoverability path was: ArgumentSpec docs → "standard
  implementations live in constructs" → the four parser types. Two hops, fine.

## Task 3b — the `-->` specials shorthand (+19 lines in lang.rs, +5 in specs.rs)

- **F4 — Standard specials behavior needs paired `Lang`-hook wiring.** Both
  `Lang::scan_specials` and `Lang::specials_trigger_chars` must be overridden,
  each delegating to a `ScopeStack` method whose own docs call it "the standard
  body of a preset's Lang::…" — the standard body exists but every language
  retypes the two delegating wrappers, and there are three touch points total
  (two hooks + the `enable_specials` gate). The failure mode of forgetting the
  trigger-chars hook is *silent* (documented as such: an omitted char "silently
  never fires"). The hook docs' obligation lists are excellent; the wiring is
  still copy-paste ceremony a `ScopeStackSpecials` mixin/derive or guide snippet
  should own.
- Conceptual asymmetry to internalize: specials resolution is token-time and
  lives on `Lang`; command resolution is parse-time and lives on the driver.
  It is stated (driver docs, "that asymmetry is decided") but a designer feels
  it as "why do my two lookups go in two places".

## Task 4 — diagnostics (in main.rs)

Displaying diagnostics was the smoothest task: `ParseError::render()`,
`Diagnostics::render_all()`, spans, and the "Open blocks" traceback all worked
untouched, and tolerant vs. strict behaved exactly as the guide describes.

- **F5 — Identifier strings are unguessable; use the typed consts.** I asserted
  `"core.constructs.unresolvable-command"`; actual is
  `"core.nodes_parser.unresolvable-command"` (namespaced by defining *file*, not
  by public module). Fixed properly with `UnresolvableCommand::IDENTIFIER` /
  `MissingMandatoryArgument::IDENTIFIER` (requires importing the
  `DiagnosticInfo` trait). The condition types being root-re-exported with good
  names made the fix fast. Recommendation: document "match on
  `Condition::IDENTIFIER` / `diagnostic.is::<T>()`, never on literal strings".
- Cosmetic: the position line renders as `at: @ (line 1, col 5)` for an
  anonymous source — the `@` placeholder collides visually with notely's escape
  character and initially read as part of the message.
- Observation (unverified, body off-limits): the strict unclosed-group abort
  printed *no* "Open blocks" traceback while the missing-argument diagnostic
  printed the full frame stack; felt inconsistent.

## Task 5 — custom ConstructParser (`@title` rest-of-line, title.rs, 132 lines)

Reached, wired, and green — the strongest evidence that the extension seams
compose. But it is the task with the widest concept surface:
`CallableSpec::make_invocation_parser` takeover contract, `Invocation`,
`ConstructParser`'s output+after-effect-delta pair, `ParseContext`
(`derived_state`, `tokens.peek/move_past`, `implementation_error`),
`TokenRulesOverrides` for the raw state, and — the heavy part — hand-staging
nodes: `NodeTreeBuilder::add`, `NodeKind::chars/callable`, `CallableData`'s 7
fields, `ParsedArguments::empty()`, `ParsedSlot::named` + `ChildRegion::new` +
`ContentNodes::InRegion` staging coordinates, `TextContent` from `Span`.

- **F6 — no staging helper for takeover callables.** ~40 of the 132 lines are
  the `CallableData` literal + builder calls + span bookkeeping that every
  takeover parser will repeat. A `stage_callable(cx, &invocation, children,
  slots, end)`-style helper (or a worked guide example) would halve the task.
- **F7 — the "raw state" idiom lacks a terminator-less helper.**
  `verbatim_state_delta` is exactly the needed all-gates-off delta but requires
  a terminator `GroupRule`; rest-of-line has none, so I hand-built the 8-line
  override block. (Log: I read the 10-line body of `verbatim_state_delta` to
  confirm which gates it flips — see implementation-body reads.)
- `ParsedArguments`/`ParsedSlots` are built from `Vec` only via `From` impls —
  found by grepping `impl From`; `::new(vec)` constructors would be
  discoverable from the type's own docs page.
- What *prevented* mistakes: the `StdInvocationParser` module-doc contract
  (trigger already consumed; post-space rule), the builder's staged/claimed
  error enum, and `implementation_error` as the sanctioned abort path. The
  contract prose is genuinely at production grade here.

## Implementation-body reads (each = a documentation gap by definition)

1. `engine/mod.rs` + `engine/language.rs` `#[cfg(test)]` modules — read
   *incidentally* (same files as the module docs I was entitled to). They
   contain the repo's only end-to-end custom-Lang/driver examples
   (`DocLang`, `MacroLang` + after-effect spec). Most individual facts were
   recoverable from doc comments, but these tests are the de-facto missing
   guide chapter; I cannot claim I was uninfluenced by them. → Gap: a
   "define your own language" guide page.
2. `constructs/verbatim_parser.rs::verbatim_state_delta` body (10 lines) —
   read deliberately to copy the all-gates-off field list for task 5's raw
   state. → Gap: a documented terminator-less raw-state helper/idiom.
3. `node/builder.rs::add_with_ext` body (~40 lines) — skimmed for assurance
   about the staging checks (claimed children, callable region tiling) before
   hand-staging in task 5. The type-level docs state the contract; I read the
   body because a takeover author cannot afford to guess. → Gap: a staging
   walkthrough (or the F6 helper making it moot).
4. `scopes/mod.rs::ScopeStack::scan_specials` body — incidental (the fold sits
   directly under its doc comment); the doc comment alone sufficed.

## Verdict against the bar

"The detailed API should be a logical reach for this level of task, possibly
organized in submodules" — **the organization meets the bar; the entry path does
not yet.** The module split (state / token / engine / spec / scopes /
constructs / node) maps one-to-one onto the decisions a language designer makes,
every needed extension point was public, and contracts were stated precisely
enough that 540 lines compiled and ran first try — for an API of this ambition
that is exceptional. What is missing is entirely at the threshold: a guide
chapter with the recipe (F1), a non-dead-end quick-start tier (F2), a neutral
seed constructor (F3), packaged specials wiring (F4), and the two task-5 helpers
(F6, F7). None of these require design changes — they are docs plus ~4 small
conveniences.
