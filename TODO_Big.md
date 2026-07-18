# TODO List of Items to Discuss

## Big chunks of things to settle

- Centralized helpers for accessing parse methods (like parser::parse())

- Read/extraction API for content: callable arguments by name, callable argument contents nodes, helpers like get_content_as_chars(), parse_keyval(), etc. etc. [cf also pylatexenc/latexnodes/nodes.py]

- Clean up DOCUMENTATION and create good USAGE-ORIENTED-DOCUMENTATION --> DESIGN_RATIONALE.md  has grown too much, IMO.

- Fully gated language features (specials, callables, groups, temporary groups ...) with corresponding memory saving in ParsingState/TokenRules fields??


## More targeted items

- Perhaps a compile-time optimization of languages that don't want to implement libraries and keep a parsing_state with a zero-sized libraries field. Lang should provide a trait for Libraries implementing the relevant lookup functions?

- SimpleLang's role.  Does it have a use?  Maybe rename "SimpleLang" to "TrivialLang" ? because there's no behavior like command resolution.  Document that it's designed mainly for use in tests. Make it private or crate-internal even?

- Ability to swap-out default parser types for different core primitives (group; nodes_parser; callable?).  Route getting instances of node parsers, group parsers (maybe even callable parsers) through L::nodes_parser(cx, ...), L::group_parser(cx, ...) etc.

- `debug_assert!(delta.is_none(), "NodesParser returns no pass-through delta");` -- might need to do something about this for a general, custom NodesParser.  In general, NodesParser could return a delta (merged encountered deltas, say \newcommands in content).  In group parser, the interior delta should be dropped (silently? with warning?)

- Do we have an easy way to "resume nodes processing" in a NodesParser? E.g. stop condition triggers (say '\end'), check name, name doesn't match, report diagnostic, then continue parsing until a new end condition matches?

- Delimited Group Parser Helper Utility! (Expand GroupParser [src/constructs/group_parser.rs] or new class? Unclear to me) Optional argument, auto delimiter detection, group/content/child parsing states, not necessarily group token types, ...

- Parsers that are worth implementing: verbatim, trailing-macro-information (TackOnMacro...), [maybe not CommaChars... -> parse argument content instead]
