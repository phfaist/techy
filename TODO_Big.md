# TODO List of Items to Discuss

## Big chunks of things to settle

- SERIALIZATION - ??

- STACK GUARD

- PY binding's wishlist analysis.


## More targeted items

- Ability to swap-out default parser types for different core primitives (group; nodes_parser; callable?).  Route getting instances of node parsers, group parsers (maybe even callable parsers) through L::nodes_parser(cx, ...), L::group_parser(cx, ...) etc.   ### still up-to-date?

- Have driver/lang be able to specify what expression parser to use when we ask for mandatory args?  E.g. mandatory arg, embellishment arg, + other places we seek an expression? Study this possibility.  ### still up-to-date?

- Restage/recompose - sub-walk subtree with custom visitor should be able to provide a symbolic argument (`None`) to mean "use same visitor", so that utilities/helpers can recurse down into children with custom logic while still invoking the original visitor callback/trait object.

- `techy::core::node::NodeKind::Comment` should hold a `CommentData` struct, mirroring `GroupData` and `CallableData`.

- Need a significant clean-up of Claude-generated docs & guides.  [CHECK: STILL THE CASE?]
  - Ban word list in user and developer guides: "door", "funnel", "mint", "trigger token", "vocabulary"
  - Very careful with the use of "contract" -- contract must be explicitly stated at that point exactly to justify the use of that word
  - References to dev-docs stages in API docs (e.g. "phase 7.8", "7.8 checkpoint") !!! Ban that.

- TokenRulesOverrides - disable_all() does not disable forbidden chars???  Rationale & revisit if necessary.


## Smaller todo

- Stack frame traceback in techy code/frames: accumulate/sort in the other order - innermost scope last.  Also, either (i) reduce the number of declared frame entries (e.g.: command->macro->argument-N->group  --> macro-arg-N ) or (ii) give "visibility" or "priority" tag/flag on frames so we can only report the meaningful frames to humans while keeping the other frames for diagnostic traceback, more refined error reporting/...

- DESIGN_RATIONALE/ARCHITECTURE pass - clean up, remove history ("Amended..." pollution)

**Spotted in API docs:**

- `callable_type == latexlike::CT_ENVIRONMENT` in `src/node/kind.rs`.
- `techy::core::node` module doc should also mention transform/restage, recompose, visit.  And link to the user guide.

**Misc:**

- `Add .markdownlint.yaml` with content

  ```yaml
  # MD012/no-multiple-blanks : Multiple consecutive blank lines : https://github.com/DavidAnson/markdownlint/blob/v0.41.1/doc/md012.md
  MD012:
    # Consecutive blank lines
    maximum: 3
  ```
