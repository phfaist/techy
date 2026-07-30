# API-SURFACE — techy items touched by the final notely code

Legend: [R] = also re-exported at crate root (`techy::Item`); [M] = module path
only. All items below were imported via module paths (matching the docs' own
example style). Trait *methods implemented* are listed under their trait.

## state
- techy::state::Lang [R] — trait implemented for `Notely`
  - associated types set: GroupTypeId, CallableTypeId, ModeId, StateExt, Event,
    SessionExt, SourceOrigin, NodeExts, Driver
  - methods implemented: `initial_state_data`, `scan_specials`,
    `specials_trigger_chars`
- techy::state::StateData [R] — constructed (fields rules/scopes/mode/ext)
- techy::state::ParsingState [R] — `scopes()` used in the driver hook
- techy::state::ParsingStateDelta [R] — `new()`, `.rules(...)` (task 5)
- techy::state::TokenRulesOverrides [R] — 6 `enable_*` fields + `..Default`
- techy::state::ClosedVocabulary [R] — implemented for both id enums (optional)
- (read but rejected: techy::state::SimpleLang [R] — unusable with a custom
  driver, see FRICTION F2)

## token
- techy::token::TokenRules [R] — constructed (13 fields)
- techy::token::WhitespaceRules [R] — constructed
- techy::token::GroupRule [R] — constructed (square + paren rules)
- techy::token::CommandRule [R] — constructed (`@`, lowercase name_chars)
- techy::token::CommentRule [R] — constructed (`#`)
- techy::token::Token [R] — driver hook parameter; `.span`, `.post_space()`
- techy::token::TokenKind [R] — matched: `Char`, `EndOfStream` (task 5)
- techy::token::TokenResult [R] — `scan_specials` return alias
- techy::token::SpecialsMatch [R] — `scan_specials` return payload
- techy::token::TriggerChars [R] — `specials_trigger_chars` return
- techy::token::TokenReader [R] — used through `cx.tokens`: `peek`, `move_past`,
  `pos` (trait not implemented; StdTokenReader engaged implicitly via
  `Language::parse`)

## engine
- techy::engine::Language [R] — `new`, `with_provider`, `parse`
- techy::engine::ParseDriver [R] — trait implemented for `NotelyDriver`
  - methods implemented: `recovery`, `resolve_command`
- techy::engine::CommandResolution [R] — `resolve_via_scopes` (the key helper)
- (types touched indirectly: ParseResult [R] — `.tree`, `.diagnostics`;
  StdParseDriver [R] — read, rejected with SimpleLang)

## error
- techy::error::Recovery [R] — Strict/Tolerant
- techy::error::DiagnosticInfo [R] — trait imported for `IDENTIFIER` consts
- (via results: ParseError [R] — `render`, `identifier`; Diagnostics [R] —
  `render_all`, `len`, `iter`, `is_empty`; Diagnostic [R] — `identifier`,
  `message`)

## scopes
- techy::scopes::Package [R] — `new`, `insert`, `insert_specials`
- techy::scopes::ScopeStack [R] — `new` (seed), `scan_specials`,
  `specials_trigger_chars` (hook delegation)
- (trait techy::scopes::SpecsProvider [R] — satisfied by Package; the
  `Arc<dyn SpecsProvider>` coercion in `with_provider`)

## spec
- techy::spec::StdCallableSpec [R] — `new`, `default`
- techy::spec::ArgumentSpec [R] — `new`, `.named(...)`
- techy::spec::CallableSpec [R] — trait implemented for `TitleSpec` (task 5)
  - methods implemented: `requires_content`, `make_invocation_parser`

## constructs
- techy::constructs::GroupArgumentParser [R] — `with_rule`
- techy::constructs::ConstructParser [R] — trait implemented (task 5):
  `type Output`, `parse`
- techy::constructs::ConstructParserResult [R] — return alias
- techy::constructs::ParseContext [R] — fields/methods used: `tokens`, `source`,
  `state`, `session`, `derived_state`, `implementation_error`
- techy::constructs::Invocation [R] — fields used: `callable_type`, `name`,
  `spec`, `token`
- techy::constructs::UnresolvableCommand [R] — `IDENTIFIER`
- techy::constructs::MissingMandatoryArgument [R] — `IDENTIFIER`

## node
- techy::node::NodeRef [R] — `summary`, `span`, `children`, `child`,
  `child_count`, `is_chars`, `is_group`, `is_comment`, `is_callable`, `chars`,
  `comment`, `comment_start`, `group_type`, `group_delimiters`, `name`,
  `callable_type`, `span_content`, `argument_content_nodes`,
  `argument_content_nodes_named`, `slot_content_nodes_named`
- techy::node::NodeSlice [R] — `iter`, `source_text` (via the accessors above)
- techy::node::NodeKind [R] — `chars`, `callable` constructors (task 5)
- techy::node::CallableData [R] — constructed (7 fields, task 5)
- techy::node::ParsedArguments [R] — `empty`
- techy::node::ParsedSlot [R] — `named`
- techy::node::ChildRegion [R] — `new`
- techy::node::ContentNodes [R] — `InRegion`
- techy::node::BuildId [R] — parser Output type
- techy::node::NodeTreeBuilder [R] — `add` (via `cx.session.builder`)

## source
- techy::source::Span [R] — `new`, `empty` semantics via `end()`/`start()`
- techy::source::SourceSpan [R] — `new`, `.range()`
- techy::source::TextContent [R] — `From<Span>` (post_space)

## Tally
~55 distinct public items touched (~20 in the "define the language" core path
of tasks 1–4; the other ~35 arrive with the task-5 takeover parser and AST
inspection). Every touched item is root-re-exported except the latexlike preset
(unused, as intended).

## Wished it existed
1. `StateData::neutral()` / `TokenRules::disabled()` — the all-gates-off value
   the `Lang::initial_state_data` default body already spells out, as a callable
   starting point (FRICTION F3).
2. A quick-start tier that survives commands: `SimpleLang` with an overridable
   `Driver` type, or a `ScopeResolvingDriver<CT>` generic driver whose
   `resolve_command` = `resolve_via_scopes(state, token, CT)` (F2) — my whole
   `NotelyDriver` is that one expression plus a recovery knob.
3. Packaged specials wiring: a one-line way to say "my specials come from the
   scope stack" instead of two hand-written delegating hooks (F4).
4. `stage_callable(...)` helper for takeover invocation parsers (F6).
5. A terminator-less raw-state delta helper (rest-of-line / until-predicate
   verbatim), sibling of `verbatim_state_delta` (F7).
6. `ParsedArguments::new(Vec)` / `ParsedSlots::new(Vec)` constructors (the
   `From<Vec<_>>` impls exist but are undiscoverable from docs pages).
7. A generic argument-spec shorthand factory (the core-level cousin of
   latexlike's `argument_specs(["m","o"])`), even just covering
   mandatory-group/optional-group/marker.
8. Doc-page guessability of diagnostic identifiers, or a stated rule
   "always match via `T::IDENTIFIER` / `is::<T>()`" (F5).
