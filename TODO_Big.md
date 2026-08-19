# TODO List of Items to Discuss

CLAUDE/AI AGENTS ARE ONLY ALLOWED TO EDIT THE SECTION BELOW MARKED
`[CLAUDE]`. Do not edit any other part of this document.


## Big chunks of things to settle

- Major doc walk-through, especially in API Doc.

  Check for banned words in user and developer guides:
  "door", "funnel", "mint", "trigger token", "vocabulary", "facts", "load-bearing",
  "straggler".
  
  Very careful with the use of "contract" -- contract must be explicitly stated
  at that point exactly to justify the use of that word. Ban references to dev-docs
  stages in API docs (e.g. "phase 7.8", "7.8 checkpoint").


- Major ARCHITECTURE/DESIGN_RATIONALE cleanup.


## More targeted items

- Have driver/lang be able to specify what expression parser to use when we ask
  for mandatory args?  E.g. mandatory arg, embellishment arg, + other places we
  seek an expression? Study this possibility.  ### still up-to-date?

- Describe "chars-only input" also as a "walker event" so that command specs like
  "\input", "\label" can be defined to take chars-only args but chars-only args
  can be refined at parse time (e.g. to include #-macro-definition-placeholder
  expansion, etc.)


## Specific things - from Claude [CLAUDE]

[CLAUDE IS ONLY ALLOWED TO EDIT THIS SECTION.]

- Better tokens — deferred follow-ups (the token/reader port; plan and stage log in
  dev-docs/bettertokens/):

  - ~~Gap-free chars-run contract: relax it for a reader that serves one parse from
    several sources — flush the run when the source changes, or let a reader declare
    that it may skip bytes.~~  **DONE**: a language declares whether its parse trees are
    span-tiled (`Lang::OBEYS_SPAN_TILING`), and the parsers of one that does not obey
    span tiling assume nothing about where tokens come from ([§dd-dr:span-tiling]).
  - `LatexlikeDriver::with_token_reader(...)`: a knob for installing a custom reader
    *instance* in the preset family.  Partly answered: a latexlike language now
    declares its reader as `Lang::Tokenization` ([§dd-dr:tokenization]), so only a
    reader configured per driver instance still needs the knob.  Same "only once such
    a reader exists".
  - `StdStreamPosition` public constructor: graduate on demonstrated need (a
    third-party reader over standard tokens that hands out positions of its own —
    such a language declares its own `Tokenization` with
    `StreamPosition = StdStreamPosition`).
  - The expanding reader itself (in-place macro expansion) lives in `techy-xp`, not
    here.
  - Naming polish over the port's new fields: `NameGroup::name` is a span, while
    `EnvironmentInvocation` splits `name` (text) from `name_span`; and
    `RawContentEnd::{content_end, end}` are two stream positions whose names do not
    say which is which.
  - `StdTokenReader::source()` is a public inherent accessor next to `content()`;
    decide whether it stays public or becomes `pub(crate)`.
  - The port's own text is clear of the banned words below, but older rustdoc around
    it still uses "mint", "facts", "vocabulary", "funnel" and "trigger token" — the
    walk-through below is where that gets settled.

- Span tiling — deliberately unfixed, recorded in [§dd-dr:span-tiling]:

  - A traceback frame's title renders a span as text, and the environment sites hand it a
    *multi-token* span, so under `OBEYS_SPAN_TILING = false` a frame can quote text that
    was never read.  Both variants are affected: `FrameTitle::Quoted { label, name }` (fed
    the name-group span by `environment_parser`'s `with_invocation_name_span` and
    `latexlike/environments`' `name_span`) and `FrameTitle::Callable { spec, role, name }`
    (fed the same span by `parse_declared_arguments`, which is where every declared
    argument's frame title comes from; it is exact at the macro sites, which pass one
    token's span).  Diagnostic decoration only — no lookup, no node data — and the fix
    changes the public `FrameTitle` (a text field beside the anchor span, or a
    `TextContent`), so it needs a decision.

  - ~~`\input` with a provided reference argument whose content is not plain characters
    (`\input{{chap.tex}}`): silent under `OBEYS_SPAN_TILING = false`, an unresolvable
    reference for the braces-included literal under a language that obeys span
    tiling.~~  **DONE** (user ruling): the reference argument must carry plain text, the
    reference is read off the argument's node data under every language, and content
    that is not plain characters raises `InvalidSourceReferenceArgument`
    (`core.sources.invalid-reference-argument`, reason `InvalidReferenceReason`) at the
    argument's span with nothing attached ([§dd-dr:span-tiling] amendment,
    [§dd-dr:input-wiring]).


## Smaller todo

- Stack frame traceback in techy code/frames: accumulate/sort in the other order - innermost scope last.  Also, either (i) reduce the number of declared frame entries (e.g.: command->macro->argument-N->group  --> macro-arg-N ) or (ii) give "visibility" or "priority" tag/flag on frames so we can only report the meaningful frames to humans while keeping the other frames for diagnostic traceback, more refined error reporting/...

- DESIGN_RATIONALE/ARCHITECTURE pass - clean up, remove history ("Amended..." pollution)
  --> SIMPLIFY GREATLY ARCHITECTURE FILE.

