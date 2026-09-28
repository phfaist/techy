# Plan — Frames appended after the parse: `Diagnostic::with_outer_frames`

**Status: EXECUTED (2026-09-28) on branch `outer-frames`; archived.** The record is [§dd-dr:diagnostic-outer-frames].

## 0. What is being added, in one paragraph

`techy::error::Diagnostic` gains one public method, `with_outer_frames`, which appends
`TraceFrame`s to the diagnostic's existing traceback as new *outer* frames (the list
stays innermost first). This is the one gap the flm-rs request identified: outside code
can build a `TraceFrame` (`TraceFrame::new` is public) and a `ParseError` can take
frames (`ParseError::with_frames` is public), but a `Diagnostic` could only receive
frames from the parse's recovery entry point. A framework that processes the parsed
tree after the parse — FLM lowering a copy of a heading's title where a reference shows
it — uses the method to say where it was when a condition was reported, so a reader can
tell "this came from a reuse of copied content, shown at line 40" from an error at the
original place. Alongside, the traceback heading `Open blocks:` (parse wording, mirrored
from pylatexenc's `Open LaTeX blocks:`) becomes the neutral `Inside:`. No second frame
list, no wire-format change, no change to `ParseError`.

## 1. Rulings (settled — do not re-litigate during execution)

1. **One frame list, not two.** The frames are, per [§dd-dr:parse-traceback], the nested
   activities open at detection, innermost first, in the shape of the Language Server
   Protocol's `relatedInformation`. A copy shown elsewhere is such an activity; copies
   nest as frames nest. A second list ("context") was the request's sketch and is
   rejected (see the record entry, stage 5, for the full list of alternatives).
2. **Append semantics.** Frames already stored are kept and stay first; the appended
   frames follow, innermost first. An empty argument changes nothing. (Replace
   semantics, mirroring `ParseError::with_frames`, was considered and rejected: it
   would clobber parse frames when a framework re-parses copied text.)
3. **Name: `with_outer_frames`.** "Outer" pairs with the "innermost first" wording
   already on `TraceFrame` and on `Diagnostic::frames`, and states the append direction
   so it cannot be mistaken for the parse-error method's replace semantics. "Context"
   is out ([§dd-dr:parse-traceback]: `ParseContext` owns the word). Checked against
   [§dd-arch:naming] principles 2–4 (specific, clear, no competing sibling term in scope).
4. **Parameter: `Vec<TraceFrame<O>>`**, the same shape as `ParseError::with_frames`; the
   public API has no `impl IntoIterator` precedent. Return type `Self`, as the other
   `Diagnostic` builders.
5. **No kind tag on `TraceFrame`.** A parse frame and an appended frame are
   distinguishable only by title. This follows the ruling that frames are the
   human-facing projection ([§dd-dr:parse-traceback], rejected alternative "structured
   machine fields on frames"). The requesting agent accepted this limit.
6. **Heading wording: `Inside:`** (user preference, 2026-09-28, over the proposed
   `While processing:`). The user also offered `Frames:`; `Inside:` is chosen because
   the rendered report is user-facing text and "frame" is a library term
   (the docs-clarity rule: no undefined terms in user-facing text). If the user
   prefers `Frames:` at kickoff, it is a one-string change plus the same test edits.
7. **Line format unchanged:** `  @ (line L, col C) [origin]: <title>`. Report structure
   unchanged: headline, `at:` position, traceback, provenance lines.
8. **`ParseError` is unchanged** in this round. A twin can follow when a need appears.
9. **No wire change.** Appended frames travel in the existing `frames` field of the
   serialized diagnostic and round-trip as parse frames already do.
10. **Attach at creation, on the framework side.** Not techy's concern to enforce, but
    the intended use (and the reason no mutable iteration over `Diagnostics` is added):
    the framework keeps its own stack of "shown at" frames and appends a snapshot of it
    in the method through which all its diagnostics pass, exactly as the parse does.

## 2. Execution stages

Every stage lists exact locations as of `main` at 24b7743; re-locate by content if
lines have moved. Work in a git worktree on a branch off `main` (the user runs
concurrent agents in the primary checkout).

### Stage 1 — The method and the heading (`techy/src/error.rs`)

**1a. Add the method** to `impl<O: SourceOrigin> Diagnostic<O>`, directly after
`from_parts` (mirroring the `from_parts` → `with_frames` order of `ParseError`):

```rust
    /// Appends frames that enclose the ones already stored.
    ///
    /// The traceback grows outward: `frames` are the activities the diagnostic's own
    /// frames were nested in, innermost first, and they are stored after the frames
    /// already present, which stay first. Code that processes a parsed tree after the
    /// parse — a framework lowering or transforming it — uses this to record where it
    /// was when the condition was reported, for instance "while lowering the copy of a
    /// heading's title shown by a reference". The diagnostic's own
    /// [`span`](Diagnostic::span) is unchanged.
    ///
    /// An empty `frames` changes nothing. The parse itself never calls this; its
    /// frames arrive through the recovery entry point,
    /// [`ParseContext::recover`](crate::core::constructs::ParseContext::recover).
    pub fn with_outer_frames(mut self, frames: Vec<TraceFrame<O>>) -> Self {
        self.frames.extend(frames);
        self
    }
```

**1b. Heading.** In `format_traceback_with` (currently line 1432) change
`String::from("Open blocks:")` to `String::from("Inside:")`. Update the `# Example
output` block in the `format_traceback` rustdoc (line ~1413) accordingly, and reword
its first sentence "as one line per open block" → "as one line per frame".

**1c. Rustdoc that says "parse frames" or "open blocks".** Run
`grep -in "open block\|parse frames\|parse traceback" techy/src/error.rs` and adjust
each hit so the text covers appended frames:

- `TraceFrame` type doc (line ~461): first sentence becomes "One frame of a traceback:
  what the parse, or processing after it, had descended into, and where." Keep the
  title/span sentences. In the paragraph on the snapshot, add: "Processing after the
  parse appends frames of its own with
  [`Diagnostic::with_outer_frames`](Diagnostic::with_outer_frames)."
- `Diagnostic` type doc (line ~553, the `frames` paragraph): "… the recovery entry
  point … attaches it; processing after the parse may append outer frames with
  [`with_outer_frames`](Diagnostic::with_outer_frames)."
- `Diagnostic::frames` doc (line ~650): "The traceback frames, innermost first: the
  parse frames that were open when the condition was recorded, followed by any outer
  frames appended afterwards with [`with_outer_frames`](Diagnostic::with_outer_frames).
  Empty when the diagnostic was recorded outside a parse descent and nothing was
  appended. Format them with [`format_traceback`]."
- `Diagnostic::render` doc (line ~661): "the traceback of open blocks" → "the
  traceback".
- `format_traceback` doc (line ~1398): "the [`TraceFrame`]s of [`Diagnostic::frames`]
  or [`ParseError::frames`], innermost first — as one line per frame."
- `ParseError::frames` doc says "the parse frames that were open at the moment of the
  abort" — still accurate (no appending on `ParseError`); leave it.

**1d. Existing tests** in the `error.rs` test module — replace `Open blocks:` with
`Inside:` in the four assertions (`format_traceback_single_frame`,
`format_traceback_multiple_frames_innermost_first`, `format_traceback_with_origin`,
`render_appends_the_traceback`; currently lines 1910, 1926, 1948, 1963).

**1e. New tests** in the same module (use the module's existing `TestCondition` and
`Source`/`SourceSpan` helpers):

- `with_outer_frames_on_an_empty_traceback_stores_them_in_order`: a diagnostic built
  with `Diagnostic::error`, two frames appended in one call; `frames()` has length 2
  with the titles in the given order.
- `with_outer_frames_keeps_existing_frames_first`: a diagnostic assembled with
  `from_parts` carrying one parse frame, then one frame appended; `frames()` is
  `[parse frame, appended frame]`. A second `with_outer_frames` call appends after
  both (three frames, in call order).
- `with_outer_frames_with_no_frames_changes_nothing`: `Vec::new()` leaves `frames()`
  and `render()` byte-identical to before.
- `render_shows_outer_frames_after_parse_frames`: `render()` of the two-frame
  diagnostic contains
  `"Inside:\n  @ (line …): <parse title>\n  @ (line …): <appended title>"`, and the
  `at:` line still shows the diagnostic's own position.
- `with_outer_frames_accepts_frames_from_another_source`: the appended frame's span is
  in a second `Source` with an origin label; the rendered line carries that label
  (this pins that the formatter resolves each frame against its own source).

### Stage 2 — Other assertions on the heading

- `techy/src/engine/mod.rs:951` — `assert!(err.render().contains("Open blocks:"))` →
  `"Inside:"`.
- `techy/src/constructs/nodes_parser.rs:3468` — same replacement, keeping the rest of
  the asserted string.
- Then `grep -rn "Open blocks" techy/ docs/ dev-docs/ARCHITECTURE.md dev-docs/DESIGN_RATIONALE.md README.md`
  must return nothing (the archive is left alone).

### Stage 3 — Serialization: test only

No change to `techy/src/serialize/wire/diagnostic.rs` or the driver. Add one test to
`techy/src/serialize/drivers/diagnostic_tests.rs`, next to the existing frame
round-trip tests, using the file's `source`, `round_trip_diagnostic` helpers:

- `outer_frames_round_trip`: `Diagnostic::error(<condition>, span).with_outer_frames(vec![two frames])`
  round-trips with both frames, in order, titles and spans equal.

Check `dev-docs/serialize_schema.md`'s description of the diagnostic `frames` field: if
it says "parse frames", widen the wording to "traceback frames (parse frames, then any
appended after the parse)"; if it says "traceback snapshot, innermost first", leave it.

### Stage 4 — Guides

- `docs/parsing.md` (line ~131): "a traceback of parse frames" → "a traceback of
  frames — the parse frames open when it was recorded, plus any that processing after
  the parse appended with
  [`with_outer_frames`](crate::error::Diagnostic::with_outer_frames)".
- `docs/concepts-overview.md` (line ~201): after "plus span and traceback frames", add
  "(which code processing the tree after the parse can extend with
  [`Diagnostic::with_outer_frames`](crate::error::Diagnostic::with_outer_frames))".
- `docs/panics.md`: untouched (nothing panics).
- Check the new text against the banned-word list in `TODO_Big.md` (no "door",
  "funnel", "mint", "trigger token", "vocabulary", "facts", "load-bearing",
  "straggler"; no unjustified "contract").

### Stage 5 — Records

**5a. DESIGN_RATIONALE.md** — new entry in the Errors topic, immediately after
[§dd-dr:parse-traceback] (before [§dd-dr:refine-diagnostic-hook]), following the entry
template (Status / body / Rejected alternatives / Accepted costs / Revisit if; no dates
in the status line):

```
#### Frames appended after the parse: `Diagnostic::with_outer_frames` [§dd-dr:diagnostic-outer-frames]

Status: DECIDED (user + design session, on a request from the FLM framework).
```

Body, in the document's concise style:

- The addition: one append-only builder on `Diagnostic`, `with_outer_frames(Vec<TraceFrame<O>>)`,
  outer frames after the existing ones, innermost first preserved; the accessor
  unchanged; the traceback heading made neutral (`Inside:`) because frames are no
  longer only the parse's. Closes the asymmetry with `ParseError::with_frames`.
- Why one list: frames are already the nested activities open at detection in
  `relatedInformation` shape ([§dd-dr:parse-traceback]); a copy shown elsewhere is such
  an activity and copies nest; nesting order stays right when parse frames and appended
  frames coexist (a framework re-parsing copied text); serialization needs nothing.
- Why the diagnostic's own span stays put: the framework copies *nodes*, which keep the
  spans of the place the content was written; the report must keep pointing there.
- The framework-side discipline: attach at creation through the framework's own
  reporting method, from its own stack — the parse's model, not wrapping on the way
  out (the tolerant path never bubbles; `Diagnostics` has no mutable iteration, and
  none is added).
- Rejected alternatives: (1) a second frame list ("context") with its own heading — a
  second mechanism for the same concept, a new wire field, inverted nesting in the
  rendered report when both lists are present, and a name `ParseContext` already owns;
  (2) synthesized-source provenance (`Source::synthesized`) — per source, and positions
  become relative to the new content, which loses the original place; the revisit
  clause of [§dd-dr:provenance-on-source] concerns per-node provenance, this need is
  per diagnostic; (3) a wrapping condition type carrying the shown-at span — changes
  the identifier every consumer matches on, and condition payloads carry no spans;
  (4) a framework-side wrapper struct around `Diagnostic` — cannot live in the shared
  `Diagnostics` collection; (5) replace semantics mirroring `ParseError::with_frames` —
  clobbers parse frames; (6) a kind tag on `TraceFrame` — the frames stay the
  human-facing projection.
- Accepted costs: appended frames are distinguishable from parse frames only by
  title; the rendered heading loses its pylatexenc echo (`Open LaTeX blocks:`).
- Revisit if: a consumer needs to filter or style appended frames apart from parse
  frames (then a kind on `TraceFrame`, still free of `L`), or a framework needs the
  same builder on `ParseError`.

**5b. ARCHITECTURE.md** [§dd-arch:errors], bullet "Tracebacks come from an explicit
frame stack": append one sentence — "Processing after the parse appends outer frames
of its own through `Diagnostic::with_outer_frames`; one list, innermost first, neutral
`Inside:` heading ([§dd-dr:diagnostic-outer-frames])." Add
`[§dd-dr:diagnostic-outer-frames]` to the section's "Decisions behind this section"
list.

**5c. Forward pointer** in [§dd-dr:parse-traceback]: after "innermost first", add
"(frames appended after the parse: [§dd-dr:diagnostic-outer-frames])". No label is
renamed or retired.

### Stage 6 — Verification

```sh
cargo build
cargo test --all-features
rm -rf target/doc && cargo docs            # no broken intra-doc links from the new anchors
scripts/check_semver.sh                    # expect: additive only (minor); no breaking change
grep -rn "Open blocks" techy/ docs/ dev-docs/ARCHITECTURE.md dev-docs/DESIGN_RATIONALE.md  # empty
git grep -n 'dd-dr:diagnostic-outer-frames' dev-docs/ARCHITECTURE.md                       # at least one hit
```

If `cargo-semver-checks` is unavailable, say so in the report instead of skipping
silently.

## 3. Execution protocol

1. `git worktree add ../techy-outer-frames -b outer-frames main` (or the user's
   preferred worktree location); all edits happen there.
2. Stages 1–4 in one commit (`feat(error): Diagnostic::with_outer_frames; neutral
   traceback heading`), stage 5 in a second (`docs: record [§dd-dr:diagnostic-outer-frames]`),
   or one commit if the executor prefers; commit messages end with the attribution
   lines in force for the session.
3. Stage 6 green, then hand the user the merge one-liner
   (`git -C <primary checkout> merge --ff-only outer-frames` after a rebase onto
   `main` if needed). No PR.
4. After the merge lands on `main`: `git mv dev-docs/PlanDiagnosticOuterFrames.md dev-docs/archive/`
   in a follow-up commit, and delete the branch and worktree.
5. Tell the user the final method name and heading so they can relay them to the
   flm-rs agent, whose side (a stack of shown-at frames, appended in its `diagnose`
   method) is out of techy's scope.

## 4. Out of scope (recorded so nobody adds them by reflex)

- A `with_outer_frames` twin on `ParseError`.
- A kind tag on `TraceFrame`, or any structured field beyond title and span.
- Mutable iteration over `Diagnostics` (`iter_mut`) — deliberately absent.
- Any change to the FLM crate.
