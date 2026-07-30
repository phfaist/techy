# API-SURFACE — techy items touched by the T5 framework walkthrough

Fully-qualified public items actually used by the probes (`probes/src/bin/*.rs`) and
the PyO3 module (`techy-py/src/lib.rs`). Grouped by module; re-exports at crate root
exist for core items (used via `techy::<module>::…` paths here).

## engine
- `techy::engine::Language<L>` — `::default()`, `::new(driver)`, `.with_provider(Arc<dyn SpecsProvider>)`,
  `.parse(&str)`, `.initial_state()`
- `techy::engine::ParseResult<L>` — `.tree`, `.diagnostics` (public fields)
- `techy::engine::StdParseDriver` (as `Lang::Driver` for the custom Lang)
- (type-probed only: `techy::engine::ParserSession<L>`)

## node
- `techy::node::NodeTree<L>` — `.root()`, `.node(NodeId)`, `.get(NodeId)`,
  `.descendants()`, `.node_count()`, `.iter_storage_order()`, `.materialize()`,
  `Clone`
- `techy::node::NodeRef<'t, L>` — `.id()`, `.kind()`, `.ext()`, `.span()`,
  `.span_content()`, `.parsing_state()`, `.summary()`, `.child_count()`, `.child(i)`,
  `.children()`, `.descendants()`, `.is_chars/.is_group/.is_callable/.is_comment/.is_list`,
  `.chars()`, `.comment()`, `.comment_start()`, `.comment_post_space()`,
  `.group_delimiters()`, `.callable_type()`, `.name()`, `.spec()`, `.post_space()`,
  `.arguments()`, `.argument_content_nodes(i)`, `.body()`
- `techy::node::NodeSlice<'t, L>` — `.iter()`, `.get(i)`, `.range()`, `.span()`,
  `.source_text()`, `.len()`
- `techy::node::NodeId` — `.index()`, `Copy/Eq/Ord/Hash`
- `techy::node::BuildId`
- `techy::node::NodeTreeBuilder<L>` — `::new()`, `.add(...)`, `.add_with_ext(...)`,
  `.finish(root)`
- `techy::node::NodeBuildError` (matched `::RegionAlreadyResolved`)
- `techy::node::NodeKind<L>` — `::chars()`, `::list()`, `::Callable` (matched/rebuilt),
  `Clone`
- `techy::node::CallableData<L>` — public fields `.arguments`, `.slots` (mutated in
  the DIY copy), `Clone`
- `techy::node::ParsedArguments<L>` — `.arguments` field, `.iter()`
- `techy::node::ParsedSlots<L>` — `.slots` field (via iter_mut in copy)
- `techy::node::ChildRegion` — `::new(range, ContentNodes)`, `.children()`,
  `.content_range()`, `.content_parent()`
- `techy::node::ContentNodes` — `::InRegion`, `::InChildrenOf`
- `techy::node::check_tree_invariants` (and its documented parse-tree-only scope)
- `techy::node::NodeExt<L>` type alias (custom Lang finalize_node signature)
- `techy::node::StagedNodes<'_, L>` (finalize_node signature)
- (extract module exercised earlier via guide doctests pattern, not in probes:
  `techy::node::extract::{content_as_chars, split_at_chars, parse_keyval}`)

## error
- `techy::error::Diagnostic<O>` — `.severity()`, `.identifier()`, `.message()`,
  `.span()`, `.render()`
- `techy::error::Diagnostics<O>` — `.len()`, `.iter()`
- `techy::error::Severity` — exhaustive match Error/Warning/Note
- `techy::error::ParseError<O>` — `.identifier()`, `.render()`
- `techy::error::Recovery` — `::Strict`, `::Tolerant`

## source
- `techy::source::Source<O>` — `::new()`, `::synthesized(content, desc, triggered_at)`,
  `.content()`, `.origin()`, `.line_index()`
- `techy::source::SourceSpan<O>` — `::new(&Arc<Source>, range)`, `::entire()`,
  `.start()`, `.end()`, `.range()`, `.content()`, `.source()`, `.same_source()`, `Clone`, `PartialEq`
- `techy::source::LineIndex<'c>` — `.line_col(pos)`
- `techy::source::TextContent` — matched `::Owned` / `::Spanned`
- (type-probed: `techy::source::Span`, `SourceProvenance`)

## state
- `techy::state::Lang` — implemented (custom Flm): associated types, `initial_state_data`,
  `finalize_node`
- `techy::state::NodeExtTypes` — implemented (custom bundle)
- `techy::state::StateData<L>` — struct literal
- `techy::state::ParsingState<L>` — `.mode()` (via guide), Arc-shared
- (type-probed: `ParsingStateDelta<L>`)

## scopes
- `techy::scopes::Package<L>` — `::new(name)`, `.insert(type, name, Arc<spec>)`,
  `.insert_specials` (guide)
- `techy::scopes::ScopeStack<L>` — `::new()`, `.push(Arc<provider>)`
- (type-probed: `Scope<L>`; trait used implicitly: `SpecsProvider`)

## spec
- `techy::spec::CallableSpec<L>` — implemented by a framework type (defaulted methods;
  `Any` downcast used at render time)
- `techy::spec::ArgumentSpec<L>` — `Arc`-held in custom spec + probed with
  `.with_state_delta` (guide pattern)
- (type-probed: `StdCallableSpec<L>`)

## token
- `techy::token::TokenRules<L>` — struct literal (all 12 public fields)
- `techy::token::GroupRule<L>`, `techy::token::WhitespaceRules` — struct literals
- (type-probed: `Token<'s, L>`)

## latexlike (preset)
- `techy::latexlike::Latexlike`
- `techy::latexlike::LatexlikeDriver` — `::new(Recovery)`
- `techy::latexlike::MacroSpec` — `::new(argument_specs)`
- `techy::latexlike::EnvironmentSpec` — `::new()`, `::from_behavior(Arc<VerbatimBehavior>)`
- `techy::latexlike::SpecialsSpec` (guide pattern)
- `techy::latexlike::VerbatimBehavior` — `::default()`
- `techy::latexlike::CallableType` — `::Macro`, `::Environment`, `::Specials`
- `techy::latexlike::MathStyle` — `::Inline`, `::Display`
- `techy::latexlike::argument_specs(["o","m","v"])`
- NodeRef preset sugar: `.macro_name()`, `.environment_name()`, `.specials_name()`,
  `.is_math_group()`, `.math_style()`
- (compile-error-probed as *non*-reusable across Lang: `default_token_rules()`,
  `base_package()`)

---

## Wished-for items (ranked rationale in FRAMEWORK-ANALYSIS.md)

- `techy::node::copy_subtree_into` (or a `TreeTransformer` visitor) made public
- `NodeTreeBuilder::finish` variant returning BuildId→NodeId correspondence
- `NodeTree` parent table: `NodeRef::parent()`, `NodeRef::index_in_parent()`
- `recompose(node)` / `emit_with_replacements(...)` span-faithful emitter
- transform-tree validator (`check_transform_tree_invariants`)
- source-uniformity-honest `NodeSlice::span()/source_text()` (or documented caveat)
- `NodeKind::name() -> &'static str` stable kind strings
- `NodeRef::tree()` public
- Lang-generic latexlike parts: `default_token_rules::<L>()`, generic
  `MacroSpec<L>`/driver core, or adapter traits (the FLM cliff)
- `Diagnostics::into_vec()` (minor)
- binding/handler cookbook page (Arc+NodeId pattern, post_space recipe,
  synthesized-node recipe)
