# TODO List of Items to Discuss

## Big chunks of things to settle

- Centralized helpers for accessing parse methods (like parser::parse()) ??

- Create good USAGE-ORIENTED-DOCUMENTATION

- Fully gated language features (specials, callables, groups, temporary groups ...) with corresponding memory saving in ParsingState/TokenRules fields??  i.e.: Perhaps a compile-time optimization of languages that don't want to implement libraries and keep a parsing_state with a zero-sized libraries field. Lang should provide a trait for Libraries implementing the relevant lookup functions?

- Public API review.  Distill aggressively narrower public entry points; keep only as much as is really necessary.  Keep flexibility for the future!


## More targeted items

- SimpleLang's role.  Does it have a use?  Maybe rename "SimpleLang" to "TrivialLang" ? because there's no behavior like command resolution.  Document that it's designed mainly for use in tests. Make it private or crate-internal even?

- Ability to swap-out default parser types for different core primitives (group; nodes_parser; callable?).  Route getting instances of node parsers, group parsers (maybe even callable parsers) through L::nodes_parser(cx, ...), L::group_parser(cx, ...) etc.

- `debug_assert!(delta.is_none(), "NodesParser returns no pass-through delta");` -- might need to do something about this for a general, custom NodesParser.  In general, NodesParser could return a delta (merged encountered deltas, say \newcommands in content).  In group parser, the interior delta should be dropped (silently? with warning?)

- Delimited Group Parser Helper Utility! (Expand GroupParser [src/constructs/group_parser.rs] or new class? Unclear to me) Optional argument, auto delimiter detection, group/content/child parsing states, not necessarily group token types, ...

- Parsers that are worth implementing: verbatim, trailing-macro-information (TackOnMacro...), [maybe not CommaChars... -> parse argument content instead]


- Have driver/lang be able to specify what expression parser to use when we ask for mandatory args?  E.g. mandatory arg, embellishment arg, + other places we seek an expression? Study this possibility.
