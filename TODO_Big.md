# TODO List of Items to Discuss

## Big chunks of things to settle

- SERIALIZATION - ??


## More targeted items

- Have driver/lang be able to specify what expression parser to use when we ask
  for mandatory args?  E.g. mandatory arg, embellishment arg, + other places we
  seek an expression? Study this possibility.  ### still up-to-date?

- API doc walk-through; 

  - Check for banned words in user and developer guides: "door", "funnel", "mint",
    "trigger token", "vocabulary", "facts"
  - Very careful with the use of "contract" -- contract must be explicitly
    stated at that point exactly to justify the use of that word
  - References to dev-docs stages in API docs (e.g. "phase 7.8",
    "7.8 checkpoint") !!! Ban that.


## Smaller todo

- Stack frame traceback in techy code/frames: accumulate/sort in the other order - innermost scope last.  Also, either (i) reduce the number of declared frame entries (e.g.: command->macro->argument-N->group  --> macro-arg-N ) or (ii) give "visibility" or "priority" tag/flag on frames so we can only report the meaningful frames to humans while keeping the other frames for diagnostic traceback, more refined error reporting/...

- DESIGN_RATIONALE/ARCHITECTURE pass - clean up, remove history ("Amended..." pollution)
  --> SIMPLIFY GREATLY ARCHITECTURE FILE.

