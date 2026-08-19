# `core::token` and the scan helpers — implementation plan

Status: APPROVED by the user (design session 2026-08-19); executed by Opus implementer
and reviewer agents per stage, in worktrees, ff-merged to `main`. Section 1 is
normative: reviewers check every diff against it. PROGRESS.md (same directory) is the
stage log; a fresh session resumes from PLAN.md + PROGRESS.md + `git log`.

---

## 0. Why: the problem in one page

Third-party crates want to write custom token readers that reuse the *pieces* of
`StdTokenReader`'s scanning without re-implementing them: a reader whose token type
wraps standard tokens drawn from several sources (a macro expander), and a reader with
its own token kinds that still wants LaTeX-style whitespace, paragraph breaks, group
delimiters, commands, comments and specials recognized exactly the way the standard
reader does. Today that logic is spread over `pub(crate)`/private methods of
`StdTokenReader` (`techy/src/token/reader.rs`: `scan_token_at`, `token_kind_of`,
`detect_paragraph_break`, `detect_group_delimiter`, `read_command`, `read_comment`,
the inline specials step, `lift_specials_scan_error`), every one of which takes `&self`
only for the content it scans. Only `skip_whitespace` is public.

Two reuse cases, two tiers:

- **Tier 1 — a reader over standard tokens from several sources.** It keeps one inner
  `StdTokenReader` per source and needs two things made public: the scanning core
  (`scan_token_at`, renamed `scan_std_token_at`) and the interpretation of a standard
  token under any `L` (`token_kind_of`, renamed `token_kind_of_std_token`). The in-crate
  scripted test reader (`token/scripted_reader.rs`) is the proof that these two suffice.
- **Tier 2 — a reader with its own token kinds.** It composes free *scan helpers*, one
  per construct, each returning a source-free *match value* (byte spans plus the
  matched rule or spec) — never a token — and builds its own tokens from them.
  `StdTokenReader`'s scanning core is rewritten as a composition of the same helpers,
  so each construct's recognition rule has exactly one implementation.

Placing six more free functions and three types in the flat `techy::core` hub (81
items today, 33 of them token-topic) was rejected by the user. The hub's token subset
is extracted into a new facade **`techy::core::token`** — the mirror of
`core::constructs` (contract + shipped implementation + helpers + conditions), which
[§dd-dr:public-namespace-topology]'s own revisit clause ("the hub grows uncomfortably
large") calls for. The straddle that kept the token subset in the hub ("token data vs
runtime") is resolved by the placement rule in §1.2.

---

## 1. Target design (normative — reviewers check diffs against this section)

### 1.1 Vocabulary (define on first use in every doc that uses the term)

- **scan helper** — a free function `fn(content: &str, pos: usize, …) -> …` that
  recognizes one construct at `pos` in `content` and returns a match value or `None`;
  it never advances anything and never builds a token.
- **match value** — what a scan helper returns on success: byte spans into `content`
  (plain `Span`s, never `SourceSpan`s — the helper knows no source) plus the matched
  rule or spec. Types: `GroupDelimiterMatch`, `CommandMatch`, `CommentMatch`, and the
  existing `SpecialsMatch`.
- **the standard dispatcher** — `StdTokenReader::scan_std_token_at`: the composition of
  the scan helpers in the standard priority order (paragraph break → expected group
  close → longest delimiter → command → comment → specials → forbidden character →
  `Char`), producing `StdToken`s.
- **placement rule of `core::token`** — see §1.2.
- Plan shorthand only (never in rustdoc/guides): "Tier 1", "Tier 2", "the hub".

### 1.2 The `core::token` facade

**Public path.** `techy::core::token`, a facade module file `techy/src/core/token.rs`
(`pub mod token;` in `techy/src/core/mod.rs`, beside `constructs`, `node`, `specs`).
It re-exports from the internal `crate::token` and `crate::state` modules; internal
file layout stays as is and stays invisible.

**Placement rule (record verbatim in the facade's module docs, ARCHITECTURE and the
DESIGN_RATIONALE entry):** *`core::token` holds what a token reader produces, consumes
and answers with — the token and stream-position types, the `TokenReader` trait and
the standard reader, the scan helpers, the token rules the reader reads together with
the overrides that change them mid-parse and the caches derived from them, the types
the specials-scan hooks answer with, and the token conditions and errors. The hub
keeps the `Lang` trait (its associated types and hooks), the parsing state and its
deltas, and the engine.*

**Items that move from `techy::core` to `techy::core::token`** (41; every one keeps
its name):

- Tokenization declaration: `Tokenization`, `StdTokenization`, `Token`,
  `StreamPosition`.
- Token values and views: `StdToken`, `StdStreamPosition`, `TokenKind`, `TokenEdge`.
- Reader: `TokenReader`, `StdTokenReader`, `skip_whitespace`.
- Rules data: `TokenRules`, `WhitespaceRules`, `ParagraphRules`, `GroupRules`,
  `GroupRule`, `CommandRules`, `CommandRule`, `CommentRules`, `CommentRule`,
  `SpecialsRules`, `ForbiddenCharsRules`.
- Rules overrides (from `crate::state`): `TokenRulesOverrides`, `WhitespaceOverrides`,
  `ParagraphOverrides`, `GroupOverrides`, `CommandOverrides`, `CommentOverrides`,
  `SpecialsOverrides`, `ForbiddenCharsOverrides`.
- Caches: `PrefixTable`, `PrefixEntry`, `TriggerChars`.
- Specials-scan hook types: `SpecialsMatch`, `SpecialsScanError`.
- Errors and conditions: `TokenError`, `TokenErrorKind`, `TokenRecovery`,
  `TokenResult`, `EndOfStreamAfterEscape`, `ForbiddenChar`.

**Items that stay in the hub:** everything else it exports today — `Lang`,
`TrivialLang`, `LangFeatures`/`AllLangFeatures`/`NoLangFeatures`, the `LangHas*`
markers, `FeaturePresence`/`FeaturePresent`/`FeatureAbsent`, `ClosedVocabulary`,
`InvocationSyntax`, `NodeExtTypes`, `ParsingState`, `ParsingStateDelta`,
`ParsingStateStack`, `StateData`, `DeriveError`, `FinalizeError`, and the engine
family. `ParsingStateDelta::rules(TokenRulesOverrides)` and `Lang::Tokenization` /
`Lang::scan_specials` / `Lang::specials_trigger_chars` become cross-facade signature
references — the accepted kind (`Lang::make_node_ext` already names `core::node`
types).

**New items added by this plan (Stage 1) live in `core::token`, flat:**
`scan_paragraph_break`, `scan_group_delimiter`, `GroupDelimiterMatch`,
`command_rule_at`, `scan_command`, `CommandMatch`, `scan_comment`, `CommentMatch`,
`scan_specials_trigger`.

**Facade module docs (`core/token.rs`), required content, in this order:**
1. One-paragraph definition: what a token is (opaque; the reader answers what and
   where), pointing at `TokenReader` and `Tokenization`.
2. The placement rule, verbatim.
3. A tour by group (the six bullets above, as a list with links).
4. "Writing a token reader" — the three ways, each one paragraph with links, in this
   order: (a) *rules data only* — no reader at all: `TokenRules` (the preset's
   `default_token_rules` as the worked example); (b) *a reader over standard tokens* —
   either the delegating wrapper of the `TokenReader` docs (same tokens, different
   classification) or a token type of one's own that wraps standard tokens read from
   one or several sources: one inner `StdTokenReader` per source,
   `StdTokenReader::scan_std_token_at` to read, `StdTokenReader::token_kind_of_std_token`
   to interpret (link the `TokenReader` *Seams* section for the several-sources
   contract); (c) *a reader with its own token kinds* — the scan helpers, listed, with
   the sentence that `StdTokenReader` itself is written as their composition and that
   the standard priority order is documented on `scan_std_token_at`.
5. The `pos` precondition of the scan-helper family, stated once here and repeated per
   function (§1.4).

**Hub docs (`core/mod.rs`):** the "Tokens" bullet is replaced by a sentence pointing
at `token`; the "Three submodules hold the subsets with clear boundaries" list becomes
four, `token` first ("the tokenization library: …"). `techy/src/lib.rs`'s topology
list (line ~74) gains the `core::token` bullet. `CLAUDE.md`'s public-topology list
gains a `techy::core::token` bullet (between `techy::core` and `techy::core::specs`)
and its `techy::core` bullet drops the token items.

**Every explicit path** `crate::core::<Item>` / `techy::core::<Item>` for a moved
item, in rustdoc text, doctests, `docs/*.md`, `docs/panics.md`, tests, README, is
retargeted to `…::core::token::<Item>`. Grep list for the reviewer (moved-item regex):
`(CommandRule|CommandRules|CommentRule|CommentRules|EndOfStreamAfterEscape|ForbiddenChar|ForbiddenCharsRules|GroupRule|GroupRules|ParagraphRules|PrefixEntry|PrefixTable|skip_whitespace|SpecialsMatch|SpecialsRules|SpecialsScanError|StdStreamPosition|StdToken|StdTokenization|StdTokenReader|StreamPosition|Token|TokenEdge|TokenError|TokenErrorKind|Tokenization|TokenKind|TokenReader|TokenRecovery|TokenResult|TokenRules|TriggerChars|WhitespaceRules|TokenRulesOverrides|WhitespaceOverrides|ParagraphOverrides|GroupOverrides|CommandOverrides|CommentOverrides|SpecialsOverrides|ForbiddenCharsOverrides)`
— after Stage 0, `grep -rEn "core::(<regex>)\b"` over `techy/src techy/tests docs
README.md CLAUDE.md` matches only lines where the path continues with `::token::`
(i.e. zero matches for `core::<Item>` not preceded by `token::`). Multi-line
`use techy::core::{ … }` blocks are inspected by hand (75 such `use` lines exist).

**External projects are not touched** (user ruling): `techy-ext`, `techy-playground`,
`flm-rs` adapt on their own schedule.

**Version and semver baseline:** no version bump and no move of the `api-baseline`
branch in this plan; `scripts/check_semver.sh` is expected to report the path breaks
(soft freeze; recorded in PROGRESS.md and reported to the user, who moves the baseline
deliberately — [§dd-dr:stability-rubric]).

### 1.3 Tier 1 — the two promoted methods of `StdTokenReader`

Both already panic-free by construction; visibility and names change, docs are
rewritten for an external reader author.

```rust
impl<'s, O: SourceOrigin> StdTokenReader<'s, O> {
    /// (was `token_kind_of`, pub(crate))
    pub fn token_kind_of_std_token<'t, L: Lang>(&self, tok: &'t StdToken<L>) -> TokenKind<'t, L>
    where 's: 't;

    /// (was `scan_token_at`, pub(crate))
    pub fn scan_std_token_at<L>(
        &self,
        start: usize,
        state: &ParsingState<L>,
        recovery_for: impl FnOnce(StdToken<L>, usize) -> Option<TokenRecovery<L>>,
    ) -> TokenResult<L, StdToken<L>>
    where
        L: Lang<SourceOrigin = O>;
}
```

- `token_kind_of_std_token`: doc says what it is (the interpretation behind
  `TokenReader::token_kind`, requiring only `L: Lang`), who needs it (a reader of a
  language whose `Lang::Tokenization` declares its own token type and stores standard
  tokens read by inner `StdTokenReader`s — it cannot call the trait method, whose impl
  is bound to the standard tokenization), the foreign-token behavior (offsets outside
  this content read as empty text, never a panic — contract clause 4), and that the
  view borrows the token and this reader's content, never the reader.
- `scan_std_token_at`: doc says it is the standard dispatcher (state the priority order
  here — this is the canonical place; the `TokenRules` docs may keep their copy), that
  it does not move the reader, that `start` is validated (an out-of-bounds or
  mid-character `start` is reported as an unrecoverable implementation error anchored
  at the nearest valid offset at or before it — no panic), what `recovery_for` is for
  (the one step needing the language's token and position types; `None` = no recovery
  offered), and which failures it reports (dangling escape, forbidden character — both
  with the standard placeholder recovery when `recovery_for` answers; specials-hook
  failures and contract violations — no recovery). It is written as the composition of
  the §1.4 helpers (Stage 1 step 4).
- The internal callers (`peek`, the scripted reader) are renamed accordingly. The
  `TokenReader` trait docs' *Writing a reader over standard tokens* section gains one
  sentence pointing at the two methods for the several-sources case; the full
  description lives in the `core::token` module docs (§1.2 item 4b).

### 1.4 Tier 2 — the scan helpers (`techy/src/token/scan.rs`, new; public via `core::token`)

**Common contract, stated once in the module docs of `core::token` and repeated in
each function's `# Panics` section:** `content` is the text being scanned; `pos` is a
byte offset into it that must lie within the content (`pos <= content.len()`) and on
a `char` boundary. A `pos` that violates this is a caller-contract violation and
**panics, in all builds** — the user-approved exception for this family
([§dd-dr:panic-policy] rule 3; register line in `docs/panics.md`, §1.6). The reader
that calls a helper validates its position once, at its own consumption boundary
(what `scan_std_token_at` does), and passes derived offsets on. Helpers never allocate,
never build tokens, never see a `Source`, and are generic over `L: Lang` only through
the rules or state they read (feature gating included: a helper for a construct whose
feature the language declares absent returns `None` — its branch compiles away).

`skip_whitespace` (and its private companion `paragraph_continues`) **moves** into
`scan.rs` unchanged in signature, behavior and docs (its public path becomes
`core::token::skip_whitespace`; its `docs/panics.md` line is folded into the family
line).

```rust
/// A paragraph break — a whitespace run holding two or more newlines — beginning
/// exactly at `pos`: `Some(span)` from the first newline through the last newline of
/// the run (whitespace after the last newline is left for the next token's
/// pre-space); `None` when the paragraphs or whitespace feature is absent or
/// disabled, when `'\n'` is not a whitespace character, when the text at `pos` does
/// not start with `'\n'`, or when the run holds a single newline. Intended to be
/// called at the offset `skip_whitespace` answered, which stops right before the
/// first newline of such a run.
pub fn scan_paragraph_break<L: Lang>(content: &str, pos: usize, rules: &TokenRules<L>) -> Option<Span>;

/// A group delimiter at `pos`, with the rule it belongs to.
pub enum GroupDelimiterMatch<'r, L: Lang> {
    Open  { span: Span, rule: &'r Arc<GroupRule<L>> },
    Close { span: Span, rule: &'r Arc<GroupRule<L>> },
}
impl<'r, L: Lang> GroupDelimiterMatch<'r, L> {
    pub fn span(&self) -> Span;
    pub fn rule(&self) -> &'r Arc<GroupRule<L>>;
}
// manual Clone, Copy, Debug, PartialEq, Eq (no `L: …` bounds; rules compare
// structurally, as `TokenKind::GroupOpen` does)

/// The delimiter the standard reader recognizes at `pos`: the close delimiter of
/// `state.rules().expecting_group_close()` when non-empty and present at `pos`
/// (regardless of the groups gate — the expected close is what the parser inside the
/// group waits for); otherwise the longest entry of `state.prefix_table()`, read as an
/// opener when the string is both an open and a close delimiter. `None` when nothing
/// matches, when the groups feature is absent (no table), or when it is disabled (the
/// table is empty).
pub fn scan_group_delimiter<'r, L: Lang>(content: &str, pos: usize, state: &'r ParsingState<L>)
    -> Option<GroupDelimiterMatch<'r, L>>;

/// The first command rule (in `rules.command_rules()` order) whose escape character
/// stands at `pos`; `None` at the end of the content, when the commands feature is
/// absent or disabled, or when no rule's escape character stands there.
pub fn command_rule_at<'r, L: Lang>(content: &str, pos: usize, rules: &'r TokenRules<L>)
    -> Option<&'r Arc<CommandRule>>;

/// A command read at `pos` under `rule`. `span` runs from the escape character
/// through the post-space; `name` is what follows the escape character (a run of the
/// rule's name characters, or a single character); `post_space` is the syntactic
/// whitespace after a multi-character name (`skip_whitespace` from the name's end, so
/// it never crosses a paragraph break) and empty after a single-character name.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CommandMatch { pub escape_char: char, pub span: Span, pub name: Span, pub post_space: Span }

/// Read the command whose escape character (`rule.escape_char`) stands at `pos`.
/// `Err(EndOfStreamAfterEscape)` when the escape character is the last character of
/// the content — the condition the standard reader reports with a `Char(escape_char)`
/// placeholder over `pos..content.len()`, resuming at `content.len()`.
/// # Panics — on an invalid `pos` (family rule), and when `rule.escape_char` does
/// not stand at `pos` (see D3).
pub fn scan_command<L: Lang>(content: &str, pos: usize, rules: &TokenRules<L>, rule: &CommandRule)
    -> Result<CommandMatch, EndOfStreamAfterEscape>;

/// A whole comment at `pos`: `start` is the matched start delimiter (a leading
/// sub-range of `span`), `content` the text up to but excluding the terminating
/// newline (or the end of the content), `post_space` the newline plus following
/// indentation as `skip_whitespace` reads it — empty when that whitespace forms a
/// paragraph break, which then surfaces on its own. `span` = `pos..post_space.end()`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CommentMatch { pub span: Span, pub start: Span, pub content: Span, pub post_space: Span }

/// The comment starting at `pos` when a comment-start delimiter matches there (the
/// longest non-empty one across `rules.comment_rules()`); `None` when none does or when
/// the comments feature is absent or disabled. `'\n'` is the sole line terminator
/// (`'\r'` is ordinary content).
pub fn scan_comment<L: Lang>(content: &str, pos: usize, rules: &TokenRules<L>) -> Option<CommentMatch>;

/// The specials step of the standard reader at `pos`: `Ok(None)` when the specials
/// feature is absent, at the end of the content, when the character at `pos` is not
/// in `state.trigger_chars()` (`None` filter or `may_start` false), or when
/// `Lang::scan_specials` answers no match; `Ok(Some(m))` for a match whose `m.end`
/// satisfies the `SpecialsMatch` contract (`pos < end <= content.len()`, on a `char`
/// boundary); `Err(e)` for a hook failure — `e` is the hook's own error when its span
/// lies within the content on `char` boundaries, and otherwise, as for an `m.end`
/// violating the contract, a `SpecialsScanError { kind: Custom(ImplementationError…),
/// span: Span::empty(pos) }`. No `Err` carries a recovery: the caller reports it as
/// `TokenError::new(e.kind, SourceSpan::new(source, e.span), None)`.
pub fn scan_specials_trigger<L: Lang>(content: &str, pos: usize, state: &ParsingState<L>)
    -> Result<Option<SpecialsMatch<L>>, SpecialsScanError>;
```

Semantics are those of today's private methods, verbatim (Stage 1 step 3 moves the
bodies; the reviewer diffs the moved bodies against the originals). Not extracted (kept
inline in the dispatcher, said so in the module docs): the end-of-content test, the
forbidden-character test (`rules.forbidden_chars().contains(c)` — the accessor already
answers `""` for an absent feature), the `Char` fallback. `nearest_valid_offset` stays
a private method of the reader (the helpers' precondition makes `pos` itself the
anchor). The `ImplementationError` messages keep today's wording.

### 1.5 The standard dispatcher after Stage 1

`scan_std_token_at` is the composition — behavior byte-identical to today (the reader
test suite is the gate; the reviewer checks each arm):

```text
validate start (unchanged) → ws_end = skip_whitespace; pre_space = start..ws_end
→ scan_paragraph_break(ws_end) → paragraph_break token
→ ws_end >= len → end_of_stream(pre_space)
→ scan_group_delimiter(pos, state) → Open → group_open(rule.clone(), span, pre_space); Close → group_close(span, pre_space)
→ command_rule_at(pos, rules) → scan_command → Ok(m) → command(m.escape_char, m.span, pre_space, m.post_space)
                                            → Err(c) → TokenError(EndOfStreamAfterEscape(c), source-qualified pos..len, recovery_for(char(escape, pos..len, pre_space), len))
→ scan_comment(pos, rules) → comment(m.start, m.span, pre_space, m.post_space)
→ scan_specials_trigger(pos, state) → Err(e) → TokenError(e.kind, source-qualified e.span, None)
                                     → Ok(Some(m)) → specials(m.callable_type, m.spec, pos..m.end, pre_space)
→ forbidden char (unchanged) → char (unchanged)
```

`detect_paragraph_break`, `detect_group_delimiter`, `read_command`, `read_comment`,
`lift_specials_scan_error` are deleted (their bodies live in `scan.rs`).

### 1.6 Documentation

- **rustdoc**: every new/renamed item fully documented (`missing_docs` is `deny`);
  the `core::token` module docs per §1.2; the `core::token` "Writing a token reader"
  section is the canonical place for the three ways — `docs/custom-lang.md` (Stage 2)
  summarizes and links to it.
- **`docs/panics.md`**: the `skip_whitespace` bullet becomes the family bullet: "the
  scan helpers `skip_whitespace`, `scan_paragraph_break`, `scan_group_delimiter`,
  `command_rule_at`, `scan_command`, `scan_comment`, `scan_specials_trigger` (all in
  `core::token`) — each requires `pos` to lie within the content, on a `char`
  boundary; `scan_command` additionally requires `rule.escape_char` to stand at
  `pos`." Update the count in the "Precondition asserts" lead sentence ("Five value
  functions and the seven span-taking … constructors" → whatever the new count is; say
  it precisely). All `crate::core::…` links in the file retargeted (Stage 0).
- **`docs/custom-lang.md`** (Stage 2): the paragraph starting "A language whose
  tokenization *behavior* — not just data — differs" gains, after the "Keeping the
  standard token type…" sentence, the several-sources sentence (`scan_std_token_at` +
  `token_kind_of_std_token`, link the *Seams* section) and the own-token-kinds
  sentence (the scan helpers, link `core::token`'s "Writing a token reader").
- **`CLAUDE.md`, `techy/src/lib.rs`, `core/mod.rs`**: Stage 0 (§1.2).
- **ARCHITECTURE / DESIGN_RATIONALE** (Stage 2 only): see §5.
- **Banned words** (user, TODO_Big.md; applies to every doc line this plan writes or
  rewrites): "door", "funnel", "mint", "trigger token", "vocabulary", "facts",
  "load-bearing", "straggler"; "contract" only where the contract is stated right
  there; no dev-docs stage references in API docs. Also the docs clarity rules: define
  every term on first use, no metaphors, US English.

### 1.7 Naming register (reviewers enforce; [§dd-arch:naming])

| Item | Name | Notes |
|---|---|---|
| The facade | `techy::core::token` (file `core/token.rs`) | singular, like `core::node`; the internal `crate::token` module is unrelated to the public path |
| Promoted scan core | `StdTokenReader::scan_std_token_at` | user ruling; "std" says the result is a `StdToken<L>` for any `L` |
| Promoted interpretation | `StdTokenReader::token_kind_of_std_token` | user ruling; keeps the `token_kind` stem of the trait method |
| Helper family | `skip_whitespace`, `scan_paragraph_break`, `scan_group_delimiter`, `command_rule_at`, `scan_command`, `scan_comment`, `scan_specials_trigger` | `scan_` = recognize at a position without moving; the specials helper is deliberately not named like the hook `Lang::scan_specials` |
| Match types | `GroupDelimiterMatch<'r, L>`, `CommandMatch`, `CommentMatch` | parallel to `SpecialsMatch`; no "Shape", no "Info" |
| Internal file | `techy/src/token/scan.rs` | not public |
| Superseded names | `scan_token_at`, `token_kind_of`, `detect_paragraph_break`, `detect_group_delimiter`, `read_command`, `read_comment`, `lift_specials_scan_error` | Stage 2 adds them to [§dd-dr:superseded-names] |

### 1.8 Small decisions with defaults (do not ask; record in PROGRESS.md if deviated)

- D1 — `GroupDelimiterMatch::Close` carries the rule (the expected-close branch has it,
  and every prefix-table close entry has one) even though `StdToken::group_close` drops
  it: a match value is richer than the token, and losing information in a helper would
  be a choice a caller cannot undo.
- D2 — `CommandMatch`/`CommentMatch` carry the whole `span` alongside their parts
  (redundant by construction; the docs state the coherence: `span.start()` is the
  escape character / start delimiter, `span.end() == post_space.end()`).
- D3 — `scan_command` asserts, in all builds, that `rule.escape_char` stands at `pos`
  (a mismatch would otherwise slice mid-character and panic anyway, less clearly). It
  falls under the family's approved exception and is named in the `docs/panics.md`
  line. **Flag to the user in the Stage 1 report** (the exception as granted named
  only "invalid pos"; if the user objects, the fallback is
  `-> Option<Result<CommandMatch, EndOfStreamAfterEscape>>` with `None` for the
  mismatch).
- D4 — `command_rule_at`, `scan_specials_trigger` at `pos == content.len()` answer
  `None` / `Ok(None)` (no character there) rather than panicking: `pos == len` is a
  valid position by the family rule.
- D5 — `scan_paragraph_break`'s `Some` span is `pos..last_nl_end` exactly as
  `detect_paragraph_break` computes today; a `pos` at which the run does not start
  with `'\n'` answers `None` (no assertion that the caller stood at
  `skip_whitespace`'s answer).
- D6 — Tests for the helpers go in `scan.rs`'s own `mod tests` (moved
  `skip_whitespace` tests included, unchanged); the reader's tests stay in `reader.rs`
  and are not weakened or rewritten (they are the byte-identical-behavior gate).
- D7 — Doctests: `core::token`'s module docs carry no full reader example (the
  `TokenReader` and `Tokenization` docs already have compiling examples; the module
  docs link them). One short compiling doctest on `scan_command` or `scan_comment` is
  welcome if cheap; not required.
- D8 — The `TokenRules` docs' priority-order paragraph is kept; `scan_std_token_at`'s
  docs restate it (both list the same eight steps).

### 1.9 Rulings (user, 2026-08-19)

1. Both tiers, as recommended by the assistant's proposal.
2. Scan helpers panic on an invalid `pos` (out of bounds or mid-character), in all
   builds — exception granted under [§dd-dr:panic-policy] rule 3; document per function
   and in `docs/panics.md`.
3. Minimal inputs per helper (`&TokenRules<L>` where rules suffice; `&ParsingState<L>`
   only where a state cache or hook is involved).
4. Extract the token subset into `techy::core::token` with the placement rule of §1.2;
   the `*Overrides` go to `core::token`; record the rule.
5. Names: `token_kind_of_std_token`, `scan_std_token_at`; helper and match names as
   proposed (§1.7).
6. No public shape-returning dispatcher (it would duplicate the token's own kind data
   and blur token opacity; Tier 1 serves the mixed-source case, Tier 2 readers write
   their own dispatcher).
7. Do not touch external projects (`techy-ext`, `techy-playground`, `flm-rs`).
8. Execution: Opus implementer + reviewer agents per stage, worktrees, ff-merges to
   `main`, commit regularly.

---

## 2. Stage 0 — the `core::token` facade (`ct-0-facade` ← `main`)

Move-only, breaking public paths; no behavior change; no new items.

Files: `techy/src/core/token.rs` (new), `techy/src/core/mod.rs`, `techy/src/lib.rs`,
`CLAUDE.md`, `docs/*.md` (all with retargeted links; the reviewer's grep decides
which), `docs/panics.md`, `techy/src/serialize/wire/state.rs` (rustdoc paths), the
doctests in `techy/src/**` that `use techy::core::{…}` moved items (`token/reader.rs`
`TokenReader` example, `token/tokenization.rs` example, others the grep finds),
`techy/tests/*.rs`, `README.md` if it names a moved item.

Steps:
1. Create `core/token.rs` with the module docs of §1.2 (items 1–4; item 5's
   precondition sentence and item 4c's helper list are written in Stage 1 — leave a
   one-line placeholder sentence "The scan helpers are described with the functions
   themselves." that Stage 1 replaces) and the `pub use` blocks: the 33 token items
   from `crate::token`, the 8 overrides from `crate::state`. Remove the same names from
   `core/mod.rs`'s `pub use` blocks. Add `pub mod token;`.
2. Rewrite the hub docs per §1.2 ("Hub docs"); update `lib.rs` and `CLAUDE.md`.
3. Retarget every explicit path (§1.2 grep) in rustdoc, doctests, `docs/*.md`,
   `docs/panics.md`, tests. Where a doctest's `use techy::core::{A, B, C}` mixes moved
   and unmoved items, split it into two `use` lines.
4. Write PROGRESS.md (this stage's section) — and commit PLAN.md/PROGRESS.md as the
   branch's first commit if not yet committed.

Gates: `cargo build`, `cargo test --workspace`, `cargo test --workspace --all-features`,
`cargo clippy --workspace --all-targets -- -D warnings`, the same with `--all-features`,
`cargo docs --all-features` after `rm -rf target/doc` (no broken intra-doc links; the
alias is `doc --workspace --no-deps`); the §1.2 grep is empty; `git diff --stat`
shows no change under `techy/src/token/` other than doctest paths (no behavior edits
in Stage 0).

Reviewer checklist: (a) the 41 items are exported from `core::token` and from nowhere
else (grep each name in `core/mod.rs` — absent; in `core/token.rs` — present once);
(b) nothing else moved (diff of `core/mod.rs`'s export lists against §1.2's "stays"
list); (c) module docs contain the placement rule verbatim and the four items of
§1.2 in order; hub docs, `lib.rs`, `CLAUDE.md` updated; (d) the §1.2 grep is empty
over `techy/src techy/tests docs README.md CLAUDE.md`; (e) `docs/panics.md` links
resolve; (f) no source-behavior change (`git diff` under `techy/src/token/`,
`techy/src/state/` is doc/doctest-only); (g) banned words absent from all new/edited
doc lines (grep the list); (h) gates verbatim.

---

## 3. Stage 1 — the two promoted methods and the scan helpers (`ct-1-scan` ← `ct-0-facade`)

Files: `techy/src/token/scan.rs` (new), `techy/src/token/reader.rs`,
`techy/src/token/mod.rs`, `techy/src/token/scripted_reader.rs`,
`techy/src/core/token.rs`, `docs/panics.md`.

Steps:
1. Rename and publish the two methods (§1.3) with their docs; rename the callers
   (`peek`, the scripted reader). Add the one-sentence pointer in the `TokenReader`
   trait docs' *Writing a reader over standard tokens* section.
2. Create `scan.rs`: move `skip_whitespace` + `paragraph_continues` (+ their tests)
   there; export from `token/mod.rs` and `core/token.rs` (path unchanged for
   `skip_whitespace` relative to Stage 0).
3. Add the helpers and match types of §1.4, moving the bodies of the private reader
   methods (keep the comments that explain a rule — the pylatexenc notes, the
   `'\r'` note, the paragraph-break corner); write each function's docs and
   `# Panics` section; write the family precondition sentence in `core/token.rs`'s
   module docs (§1.2 item 5) and complete item 4c's helper list.
4. Rewrite `scan_std_token_at` as the composition of §1.5; delete the five private
   methods.
5. `docs/panics.md`: the family bullet and the lead-sentence count (§1.6).
6. Tests in `scan.rs` (one `mod tests`, a small `TrivialLang`-style test language or
   the reader tests' `latex_rules` pattern): per helper — the positive case, the
   `None`/feature-disabled case, and the corners: `scan_paragraph_break` lone newline
   → `None`, two newlines with inner whitespace → span through the last newline, run
   not starting with `'\n'` → `None`; `scan_group_delimiter` expected close wins,
   longest wins, ambiguous → `Open`, close-only entry → `Close`, groups disabled →
   `None` for table entries but the expected close still matches; `command_rule_at`
   at end of content → `None`, commands disabled → `None`; `scan_command` named vs
   single-character (post-space only for named), post-space stops before a paragraph
   break, dangling escape → `Err`, multi-byte escape character; `scan_comment` longest
   start, no trailing newline, `'\r'` kept in content, newline opening a paragraph
   break → empty post-space; `scan_specials_trigger` match, miss, non-trigger char,
   feature disabled (empty filter), invalid `end` → `Err` with the implementation
   error at `Span::empty(pos)`, hook error with valid span passed through, hook error
   with invalid span → `Err` anchored at `Span::empty(pos)`; the panics: one
   `#[should_panic]` per helper for an out-of-bounds `pos` and one for a mid-character
   `pos` (mirroring the two `skip_whitespace` panic tests), plus `scan_command` with a
   rule whose escape character is not at `pos` (D3).
7. PROGRESS.md.

Gates: as Stage 0, plus: the reader test suite in `reader.rs` passes **unchanged**
(`git diff ct-0-facade..ct-1-scan -- techy/src/token/reader.rs` shows no edits inside
`mod tests` other than the two method renames if the tests call them);
`grep -n "detect_paragraph_break\|detect_group_delimiter\|read_command\|read_comment\|lift_specials_scan_error\|scan_token_at\|token_kind_of\b" techy/src` is empty
(the last pattern must not match `token_kind_of_std_token` — use `-w`).

Reviewer checklist: (a) signatures and docs of §1.3 and §1.4 exactly (names, bounds,
return types, `# Panics` sections, the "not extracted" sentence in the module docs);
(b) each moved body diffed against its original — same logic, same messages, same
comments preserved; (c) the dispatcher matches §1.5 arm by arm, and the reader tests
are untouched; (d) `docs/panics.md` bullet + count; (e) tests of step 6 present and
meaningful (no test that only calls the function); (f) no new panic outside the family
rule and D3; (g) naming register §1.7; (h) banned words; (i) `core::token` module docs
complete per §1.2 (items 1–5); (j) gates verbatim.

---

## 4. Stage 2 — guide and record (`ct-2-record` ← `ct-1-scan`)

Files: `docs/custom-lang.md`, `dev-docs/ARCHITECTURE.md`,
`dev-docs/DESIGN_RATIONALE.md`, `TODO_Big.md` (Claude section only, if anything is
deferred), `dev-docs/coretoken/PROGRESS.md`.

Steps:
1. `docs/custom-lang.md` per §1.6 (two sentences with links; keep the paragraph's
   flow; banned words).
2. ARCHITECTURE:
   - [§dd-arch:arch] "Public export topology" paragraph: `core::{token, constructs,
     specs, node}`; one sentence with the placement rule of `core::token`.
   - [§dd-arch:token]: a bullet "**The scanning primitives are public**: the scan
     helpers (list) are free functions returning source-free match values;
     `StdTokenReader::scan_std_token_at` is their composition and, with
     `token_kind_of_std_token`, what a reader over standard tokens from several sources
     reuses" — with references to the new DR entries; the "custom reader over standard
     tokens" bullet gains the several-sources sentence.
   - The `[§dd-arch:token]` decisions list gains the two new labels.
3. DESIGN_RATIONALE:
   - New entry `[§dd-dr:core-token-facade]` (title: "`core::token`: the token subset
     extracted from the hub; the placement rule"): Status DECIDED (user); the rule
     verbatim; why now (hub size, growth of the token topic after
     [§dd-dr:public-namespace-topology], the `constructs` mirror); the item-by-item
     resolution of the four straddling families (table of §1.2 of this plan, condensed);
     accepted cost (breaking paths under the soft freeze; external projects adapt);
     rejected: a helper-only namespace (`core::tokenscan`) beside token items left in
     the hub (contract in the hub, its implementation library elsewhere — the
     asymmetry `constructs` avoids), keeping the straddle uncut; Revisit if: a token
     item genuinely belongs to two facades.
   - Amend [§dd-dr:public-namespace-topology] with a reversal note (dated — the one
     place dates are allowed): the token subset is extracted; the layout list gains
     `techy::core::token`; the "token data vs runtime" edge is resolved by
     [§dd-dr:core-token-facade].
   - New entry `[§dd-dr:scan-helpers]` (title: "The scan helpers: public,
     token-agnostic recognition primitives; the standard reader as their
     composition"): Status DECIDED (user); the two tiers and who each serves; match
     values are source-free `Span`s (why: mixed sources); minimal inputs; the family
     precondition and the granted panic exception (+ D3); the promoted methods and
     their names; rejected: a public shape-returning dispatcher (ruling 6), `Result`
     on every helper for a bad `pos`, a validated cursor newtype (would change
     `skip_whitespace`'s signature); Revisit if: a third reuse case appears that
     neither tier serves, or a helper needs a source.
   - [§dd-dr:superseded-names]: the seven names of §1.7's last row.
   - [§dd-dr:panic-policy]: the register of exceptions gains the family line
     (wherever that entry lists them — find it, do not duplicate the list elsewhere).
4. Every new `[§dd-dr:…]` label referenced from ARCHITECTURE (maintenance rule);
   `git grep` each new label to confirm.
5. PROGRESS.md: final state, gate transcript, the semver note (§1.2), the D3 flag.

Gates: `cargo docs --all-features` (rm -rf target/doc first); `cargo test --workspace`
(nothing should change); grep: each new DR label appears in ARCHITECTURE; banned-word
grep over the edited files' diff.

Reviewer checklist: (a) guide sentences accurate against the code and links resolve;
(b) both DR entries follow the entry template (Status, decisive reasons, rejected
alternatives, Revisit if; no dates except in the reversal note); the placement rule
verbatim in three places (facade docs, ARCHITECTURE, DR); (c) ARCHITECTURE references
present for both labels; (d) superseded names added; panic register line present once;
(e) banned words; (f) gates.

---

## 5. Execution protocol (orchestrator)

**Roles.** The orchestrator never edits source itself; per stage it spawns one
*implementer* agent and one *reviewer* agent (fresh context), reads compact reports,
resolves flagged points (asks the user when a point is a design decision), merges.
**Every subagent runs with `model: "opus"`** (user directive).

**Worktrees and branches.** Worktrees live under
`/Users/philippe/projects/techy/.claude/worktrees/<branch>` (gitignored); never edit
the primary checkout. Chain: `ct-0-facade` ← `main`; `ct-1-scan` ← `ct-0-facade`;
`ct-2-record` ← `ct-1-scan`. Stages are sequential (each edits what the previous one
created). Merge each stage to `main` after its review passes (ff), or merge the whole
chain at the end — the orchestrator's call; either way `main` only ever receives
reviewed stages. Reviewer of stage N reviews `git diff <base>..<branch>`.

**Merging** (user's standing procedure, no PRs): after review passes, rebase the
branch onto current `main`, run `cargo test --workspace`, confirm the primary checkout
is clean (`git -C /Users/philippe/projects/techy status --porcelain` empty — if not,
wait/ask), then `git -C /Users/philippe/projects/techy merge --ff-only <branch>`
**with the sandbox bypassed** (git needs to write under `.git`). Never merge while the
checkout is dirty. Do not push unless the user says so. Remove worktrees after the
project; delete the `ct-*` branches only when the user says so.

**Agent prompts.** Implementer: "You are implementing Stage N of
`dev-docs/coretoken/PLAN.md` (read §0, §1 and §N+2 in full; also PROGRESS.md). Work
only in worktree `<path>` on branch `<name>` (already created; run cargo there). Do not
touch dev-docs/ARCHITECTURE.md or DESIGN_RATIONALE.md (Stage 2 does). Follow CLAUDE.md
(naming, panic policy — Result not panic except the granted family exception, tests for
new behavior, US English, docs clarity: define terms, no metaphors, banned words §1.6).
Run the stage's gates before reporting. Commit in small logical commits with the
configured trailers. Update PROGRESS.md (your stage's section) and commit it. Report:
(1) what changed per file, (2) gate results verbatim (test counts, clippy, docs),
(3) any deviation from §1 with reason, (4) any open question — do not decide design
questions yourself. Never end without a final report."
Reviewer: "You are reviewing Stage N of `dev-docs/coretoken/PLAN.md` in worktree
`<path>`, branch `<name>`, base `<base>`. Read §1 and §N+2; run every gate yourself;
read the full diff; check the reviewer checklist item by item; check naming against
§1.7 and dev-docs/ARCHITECTURE.md [§dd-arch:naming]; check the panic policy; run the
greps the plan names. Report PASS/FAIL per item with file:line evidence and a list of
required fixes. Do not fix things yourself."

**Fix loop.** Reviewer FAIL → send the required-fixes list to the implementer
(SendMessage, same agent) → re-review the delta. Two failed rounds on one point →
escalate to the user.

**State on disk.** `PROGRESS.md`: per stage — branch, worktree, status
(started/implemented/reviewed/merged), gate results, decisions under §1.8, open
questions and answers.

**Never end a turn without live children or a final report.**

---

## 6. Risks and fallbacks

| Risk | Detect | Fallback |
|---|---|---|
| A moved item is still reachable at its old path (a stray `pub use`) or reachable at two paths | Stage 0 checklist (a) | remove the duplicate; one canonical path |
| A doctest imports a moved item from `techy::core` and rustdoc reports the failure only under `cargo test --doc` | Stage 0 gates (`cargo test --workspace` runs doctests) | fix the `use` |
| Rewriting the dispatcher changes a corner (comment before paragraph break; escape-led delimiter; `$$`) | Stage 1 gate: `reader.rs` tests unchanged and green | restore the arm from the original body |
| A helper's precondition is weaker than the moved body assumed (e.g. `scan_command` with a mismatched rule slicing mid-character) | Stage 1 review (b) | D3's assert; report |
| The `GroupDelimiterMatch` manual impls need `L` bounds the rest of the crate does not have | Stage 1 | write them like `TokenKind`'s manual impls (no `L` bounds; compare rules structurally) |
| The family panic line in `docs/panics.md` conflicts with the register in [§dd-dr:panic-policy] | Stage 2 | one list in DR, the guide page mirrors it; do not paraphrase |
| Docs gate: broken intra-doc links from the many retargeted paths | every stage | fix links, never drop them |

---

## 7. Deferred (not part of this plan)

1. A public "nearest valid offset" utility for anchoring reports at an invalid
   position (custom readers may want it; today private to `StdTokenReader`).
2. `StdTokenReader::source()` visibility (open item in TODO_Big.md; untouched).
3. A `pos == content.len()` end-of-content helper or an `is_forbidden_char` helper —
   deliberately not provided (one-liners).
4. Moving the `api-baseline` branch / a version bump for the path breaks — the user's
   deliberate act after merge ([§dd-dr:stability-rubric]).
