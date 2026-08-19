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

- (empty)


## Smaller todo

- Stack frame traceback in techy code/frames: accumulate/sort in the other order - innermost scope last.  Also, either (i) reduce the number of declared frame entries (e.g.: command->macro->argument-N->group  --> macro-arg-N ) or (ii) give "visibility" or "priority" tag/flag on frames so we can only report the meaningful frames to humans while keeping the other frames for diagnostic traceback, more refined error reporting/...

- DESIGN_RATIONALE/ARCHITECTURE pass - clean up, remove history ("Amended..." pollution)
  --> SIMPLIFY GREATLY ARCHITECTURE FILE.

