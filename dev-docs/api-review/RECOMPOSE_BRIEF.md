# Recompose Design-Session Brief (dedicated session, after 2b T5)

Prepared 2026-07-31. Inputs: PLAN.md decision log (P1–P5 + T1/T2 + T3 + T4 + T5 all
binding; the "NEXT: recompose design session" bullet is the charter), P4_RULING.md §7
(recompose ratification) + §8 (slot roles) + §9 (`\input`) + §10 (navigation),
T5_RULINGS.md H (per-node doctrine — BINDING), A9 (role edges), E (`ParsingStateStack`),
F (validator, slices), G (no input caching), B (`stage_invocation`), T4_RULINGS.md F-29
(walker routing) + E (navigation names) + B/C (error-Clone principle, no_std), T1T2
E4 (state stack + pillar functions), DESIGN_RATIONALE entries as cited,
walkthroughs/framework/ (FRICTION.md, FRAMEWORK-ANALYSIS.md), pylatexenc
`latexnodes/_latex_recomposer.py` (the node-data precedent). Every code claim
verified against the working tree at commit **3ae9c67** (file:line; paths relative to
`techy/src/` unless noted). The brief recommends; all rulings are the user's.

**Reading key — unapplied rulings.** The code at 3ae9c67 predates the Phase 3
application of *everything* ruled since P2. Verified for this brief: no module or item
named `recompose` exists anywhere in `techy/src/`; `ParsingStateStack`,
`exit_math_context_delta`, `VisitFlow`, `RestageVisitor`, `validate_tree`, `node_at`,
`covering_slice`, and `SourcePos` do not exist (crate-wide grep); `NodeTree` is
`{ nodes, debug-only tag }` with no annotations, no parent table, no single-source
flag (node/tree.rs:126–134); `finish()` computes a parent `Vec<u32>` and discards it
(node/builder.rs:234, 237, 245 → :270–274); `Descendants` is a flat preorder iterator
with no depth/enter-exit/skip control (node/node_ref.rs:349–371); `ParsedSlot` has no
`role` field (node/arguments.rs:325–334); the preset still ships `MathStyle` +
the `MATH_DELIMITERS` table (latexlike/node_ref.rs:16–36; P3's `MathGroupForm` is
unapplied); `NodeKind` still carries tier-2 exts (node/kind.rs:29–70). So every
"current code" fact below is the pre-application state, and every ruled-but-unapplied
structure is cited to its ruling, never to code.

**Method note.** Two prior-document claims were re-checked and hold: (1) the Phase-1b
"byte-faithful gap-filling" probe result (FRICTION.md:137–151) is a walkthrough-era
finding about a *technique* T5-H has since banned as API — it stays true as an
experimental fact and is used below only as evidence about information content;
(2) FRICTION.md:142–149's "the only node-data hole is the trigger spelling" claim was
re-verified against the node payloads at 3ae9c67 and is confirmed, with the precise
per-construct inventory in point 5 (one refinement: the `ParagraphBreakStyle::Specials`
name-normalization edge, latexlike/driver.rs:38–41, which the probe never exercised).

---

## Glossary (session jargon, defined before use)

Plain-language definitions of every review-coined or internal term this brief uses.
Rulings referenced in parentheses.

- **Recompose / recomposition** — turning a node tree back into output *text* (LaTeX
  re-emission, plain-text conversion, any render target). Ratified as a top-level
  module `techy::recompose` ([§dd-dr:recompose]); its detailed design is THIS session.
- **Restage** — the ruled tree→*tree* transformation machinery in `techy::transform`:
  a visitor walks the frozen input tree top-down while replacement nodes are staged
  bottom-up into a builder ([§dd-dr:restage], [§dd-dr:restage-ops]). "Restage" because
  every output node passes through the builder's staging door again.
- **Slot** — a content region of a callable invocation beyond its declared arguments
  (e.g. an environment's body). A record on the node (`ParsedSlot`), minted by the
  invocation parser; no spec-side declaration exists.
- **Slot roles** — the ruled `SlotRole { Content, Attached, Hidden }` field on
  `ParsedSlot` ([§dd-dr:slot-roles]): `Content` = constitutive material (environment
  body); `Attached` = derived/redundant material reconstructible from the invocation
  itself (`\input`'s parsed file content — the includer's own text contains only
  `\input{file}`); `Hidden` = callable/framework-defined attachments that generic
  techy machinery ignores, with semantics carried by the slot's *name* + spec.
- **Trigger spelling** — the literal source text that invoked a callable: escape char
  + written name (`\emph`), or the `\begin{name}`/`\end{name}` environment
  scaffolding. Today mostly *not* stored in node data (point 5).
- **Scaffolding** — the rigid syntax bytes around an environment's arguments/body:
  `\begin{name}` and `\end{name}` (plus tolerated whitespace). Currently
  *reconstructed, not recorded* ([§dd-dr:environment-scaffolding]).
- **Per-node recomposition doctrine** (T5-H, BINDING) — recomposition rebuilds output
  from each node's **own recorded data**; it never does *inter-node* span arithmetic
  ("apparent gaps" between siblings) and never reads source text beyond a node's own
  recorded content. Spans are provenance, not output location.
- **Parse-law / parse-tree law** — the byte-accounting invariant of *parse-produced*
  trees: sibling spans partition the parent's content interior exactly; regions tile;
  unrecorded scaffolding is the reconstructible complement
  ([§dd-dr:span-invariants]). T5-H demoted it to an **in-crate acceptance oracle**
  (techy's own test suite proves lossless parsing by reassembling input); it is NOT a
  framework-facing guarantee.
- **All-trees law** — the weaker law every finished tree satisfies regardless of
  origin (structure, region tiling, `TextContent` residency); checked by the ruled
  `core::node::validate_tree` ([§dd-dr:tree-validation]).
- **Honest slices** — internal shorthand for the ruled `NodeSlice::span()` /
  `source_text()` contract: they answer only when the whole run lies in a single
  source (T5-F1). The word "honest" is banned from rustdoc.
- **Annotations** — the ruled second generic on trees, `NodeTree<L, A = ()>`:
  consumer-owned per-node data in a parallel `Vec<A>` over an `Arc`-shared node core;
  `annotate()` re-types them zero-copy ([§dd-dr:node-annotations]).
- **Tree tags** — the ruled always-on `TreeTag` (u32) in `NodeId` identity, a
  cross-tree misuse detector ([§dd-dr:tree-tags]).
- **ParsingStateStack** — the ruled owning stack of enclosing parse states
  (`Vec<Arc<ParsingState<L>>>`), lent to driver hooks during a parse and
  constructible post-parse via `from_states` / `from_node_ancestors(node)`
  ([§dd-dr:enclosing-state-stack], T5-E).
- **Pillar functions** — public `LLL`-generic free functions carrying the preset's
  hook behavior (`math_group_interior_delta`, `exit_math_context_delta`,
  `make_paragraph_break_node`); the canned `LatexlikeDriver<LLL>` delegates to them
  one line per hook ([§dd-dr:preset-driver-pillars]).
- **VisitFlow** — the sketched control enum of the read-only structural walker:
  `{ Descend, SkipChildren, Stop }` (T4-F29 routing; designed in this session).
- **Level-1 / level-2 recomposition** — the pre-existing ARCHITECTURE vocabulary
  ([§dd-arch:nodes]): level 1 = a node's own `SourceSpan` → exact original text;
  level 2 = Lang-aware reproduction from recorded facts.
- **Span-verbatim / node-data strategies** — the two recompose strategies named in
  [§dd-dr:recompose]: exact bytes via spans (+ the now-banned gap-filling), vs
  reconstruction from recorded node facts (pylatexenc's
  `LatexNodesLatexRecomposer` precedent).
- **Gap-filling** — the Phase-1b probe technique: recurse children, copy the bytes
  *between* child spans from the source. Byte-faithful on unmodified parse trees
  (verified); BANNED as mechanism/guarantee by T5-H (inter-node span arithmetic).
- **`materialize()`** — `NodeTree::materialize` (node/tree.rs:226–246): a copy with
  every `TextContent` owned, making node data source-independent.
- **Archetypes** — the three framework consumers (PLAN scope): *latexpp*-class
  (source-faithful rewriting), *latex2text*-class (text conversion; content lives in
  the scheduled `techy-totext` crate), *FLM* (custom language + semantic layer +
  multiple render targets).
- **`LLL`** — the conventional generic parameter for a preset-family language
  (`LLL: LatexlikeLang`, [§dd-dr:latexlike-generalization]).
- **BuildId / staging** — builder-side node handles before `finish()` freezes the
  tree; staged regions use them (node/builder.rs:40, node/arguments.rs:102–123).

---

## Substrate: what each node kind records today (verified once, used by points 4–6)

The doctrine makes one question central: *can every node re-emit its own bytes from
recorded data alone?* Verified per kind at 3ae9c67:

| Kind | Recorded (payload) | Self-re-emission status |
|---|---|---|
| `Chars` | `content: TextContent` (exact span slice for parsed content; invariant 1, [§dd-dr:span-invariants]) | **Complete** (node/kind.rs:32–37) |
| `Group` | `group_type: Option<…>`, `open`, `close` `TextContent`, children (node/kind.rs:143–157); parser stores the *actual* delimiter spans (constructs/group_parser.rs:197–202); `close` empty on unclosed-recovery | **Complete** — includes math delimiter spelling (`$` vs `\(` …), optional-argument brackets (they are argument-region `Group` nodes, node/arguments.rs:14–36), and `\verb`-style delimited verbatim (constructs/verbatim_parser.rs:27–33) |
| `Comment` | `content`, `start` delimiter, `post_space` (terminating newline + indentation) (node/kind.rs:52–63) | **Complete** |
| `List` | children only (node/kind.rs:64–69) | **Complete** (no own syntax) |
| `Callable`, macro-shaped | `callable_type`, `name` (as written, owned), `spec`, `arguments`, `slots`, `post_space` (exactly the trigger token's own), `ext` (node/kind.rs:199–226; constructs/invocation_parser.rs:184–193) | **One gap: the escape character.** The token records it (`TokenKind::Command { name, escape_char, post_space }`, token/token.rs:64–76) but the node does not — it is dropped at staging. All argument noise/delimiters are region *nodes* (node/arguments.rs:16–36) |
| `Callable`, specials-shaped | `name` = the full trigger spelling (`~`, `` `` ``; the token carries name + spec, constructs/nodes_parser.rs:878) | **Complete** — no escape char exists |
| `Callable`, environment-shaped | environment `name`, arguments, one slot `named("body", …)`, `post_space` **empty by design** (latexlike/environments.rs:559–575) | **Largest gap: the entire scaffolding.** Not recorded: the `\begin` trigger's escape char AND command word (a *registration key*, `"begin"`, latexlike/mod.rs:298 — a framework can register the dispatcher under another name); whitespace after `\begin` (the token's post-space is consumed and unrecorded — environments.rs:477, 571–574); the name group's delimiters (`read_rigid_name_group` consumes without staging, environments.rs:478 + 495 — and accepts *any* content-class group, so `{itemize}` vs custom delimiters is real information); the whole `\end{name}` terminator — "consumed terminator bytes appear in no node; they are the reconstructible complement between the body `List`'s end and the callable's span end" (constructs/environment_parser.rs:21–22; the body `List`'s span is the content interior only, :233–241). The terminator command word is itself per-composition data (`stop_command_name`, environment_parser.rs:259–276) |
| Verbatim environment | body `List` + raw `Chars` (gobbled newline kept as a node designated out of content, verbatim_parser.rs:33–38) | Same scaffolding gap as environments |
| Paragraph break | default `ParagraphBreakStyle::Chars`: whitespace-only `Chars` over the full token — complete. `ParagraphBreakStyle::Specials`: node *name* is canonical `"\n\n"`, span covers the actual run (latexlike/driver.rs:29–46) | **Normalizing** under node-data emission (matches pylatexenc's documented paragraph normalization, `_latex_recomposer.py:22–28`); exact under own-span emission |
| Tolerant-recovery artifacts | span-backed `Chars` nodes over the consumed extent (stray close; malformed begin, environments.rs:482–492; orphan `\end`, :619–628) | **Complete** (chars content = exact bytes) |

Summary: **the residue is exactly the command-trigger spelling** — the macro escape
char, and the environment begin/end scaffolding (escape chars + command words +
post-spaces + name-group delimiters). This confirms FRICTION.md:142–149
independently. Everything else needed for re-emission is per-node recorded data.

One further verified fact used by point 6: the builder validates callable regions in
the **fixed order "provided arguments, then slots"** — the region chain must tile the
child list starting at 0 in exactly that iteration order (node/builder.rs:149–170,
full-coverage check :195–197). A scaffolding slot whose nodes *precede* the argument
regions in the child list is unrepresentable under the current check.

---

## 1. Core architecture: direct fold vs transform-to-chars (P4 open Q)

**Background.** [§dd-dr:recompose] ratified `techy::recompose` as "a generic tree
fold assembling output text; the consumer supplies per-node logic; a typed
recomposition state threads downward into children". The recorded alternative (PLAN.md
companion bullet, :155–158): implement to-text as *restage* transformations ending in
`Chars` nodes, plus a trivial concatenation. The P4 ruling deferred the choice
(P4_RULING.md §7: "direct fold vs transform-to-chars-then-concatenate").

**Verified facts.** (a) Restage's ruled contract is *read frozen / write staged*: a
`Descend` parent **never sees its children's results**, and the staged side is
write-only by design — "verified there is no meaningful staged-side read need"
([§dd-dr:restage], read-frozen bullet; T5-A confirms the op surface). (b) Text
composition is exactly a children-results fold: a latex2text handler for
`\emph{X}` needs X's *rendered text* to produce its own output (pylatexenc's
recomposer methods receive recomposed-children strings, `_latex_recomposer.py:99–139`).
(c) A chars-tree intermediate costs one staged node per output fragment with
span/state/provenance bookkeeping (`NodeTreeBuilder::add` demands span + state per
node, node/builder.rs:199–204) and forbids streaming — the whole output materializes
as a tree first. (d) The two mechanisms already have a ruled composition point:
latexpp's verbatim output path "needs no splicing at all — recompose emits
`\input{file}` per source" (T5-G rider), and restage-then-recompose is the
targeted-replacement pipeline of point 4.

**Options.**

- **1A — direct fold** (own driver in `techy::recompose`): per-node consumer logic
  receives the node, the downward state, and a recursion/emission context; children's
  output composes into the parent's (either streamed in order or returned as values).
  Pros: matches the P4 phrase; streaming possible; no intermediate tree; the natural
  home for the role-keyed strategies (points 4/8); Lang-generic mechanism with
  preset-side spelling strategies, as ruled. Cons: a second traversal driver
  (mitigated by shared walk vocabulary, point 7).
- **1B — transform-to-chars as THE mechanism**: recompose = a restage pass that
  replaces every node with `Chars` + a concatenation helper. Pros: one driver for
  everything; output-as-tree is inspectable/validatable. Cons: **structurally at odds
  with two T5 rulings** — parents need children's results (denied by read-frozen/
  write-staged) so every node becomes an `Emit` takeover, and the visitor would
  re-implement the fold *inside* restage anyway; O(document) synthesized nodes +
  provenance ceremony for an ephemeral result; no streaming; forces `String`.
- **1C — both as peers** (fold for to-text, chars-transform for "rewrite then
  re-emit"): the honest reading of the archetypes. Not a dual-path violation:
  tree→text and tree→tree are different operations, not two spellings of one.

**Recommendation: 1A as the mechanism, with 1C's composition documented.** The fold
is the recompose driver; "transform ending in chars nodes, then recompose the result"
remains a *pipeline* (restage does the tree work, recompose does the text work — each
inside its own ruled contract). If ruled, the PLAN companion sentence "to-text then =
transformations ending in string nodes + concatenation" (PLAN.md:156–158) should be
marked superseded in the PLAN update. What FLM needs is 1A: its render targets are
per-node logic + downward render context + a sink, run over a custom `Lang` — so the
core driver must be `L: Lang`-generic, with only the trigger-spelling strategies
`LLL`-bound (the ruled core-walk/preset-spellings split, [§dd-dr:recompose]).

**Cost.** Two traversal drivers exist (restage, recompose) plus the read walker —
point 7 exists precisely to keep them one idiom.

---

## 2. State-threading model

**Background.** P4 §7: "a typed **recomposition state** threads downward into
children". Binding neighbors: T1/T2-E4 + T5-E built `ParsingStateStack` (owning;
`from_states` + `from_node_ancestors`) so *parse-state* synthesis works post-parse,
and the E4 pillar functions make preset event logic callable outside a session. The
session must answer: what is the recompose state parameter concretely — a
`ParsingState`, the stack, or a consumer-defined fold state?

**Verified facts.** (a) Every node records its parse-time state
(`NodeRef::parsing_state()`, node/node_ref.rs:82–84) — enclosing *parse* context is
per-node recorded data, so reading it inside per-node logic is doctrine-clean; the
pylatexenc recomposer does exactly this (escape char from
`n.parsing_state.macro_escape_char`, `_latex_recomposer.py:111`). (b) What parse
states do NOT give you: output-side context — "am I inside a heading", current
indentation, math-rendering flags, replacement maps. The T5 walkthrough logged this
as the context-sensitive-handler friction (FRAMEWORK-ANALYSIS.md:43–45). (c) The
ruled `ParsingStateStack::from_node_ancestors(node)` needs the stored parent table
([§dd-dr:enclosing-state-stack] T5 amendment) — post-application it lets a consumer
recomposing a *detached subtree* reconstruct enclosing parse context; it has no role
in threading *recomposition* state.

**Options.**

- **2A — the state is `ParsingState`/`ParsingStateStack`.** Rejected on function:
  parse states are already per-node data (no threading needed to read them), and
  they cannot carry consumer render context. Threading them would duplicate recorded
  facts and add nothing.
- **2B — consumer-defined fold-state type parameter `S`**, threaded downward by the
  driver: per-node logic receives `&S` and may descend into a subtree under a
  *derived* value (scoped — restored after the subtree), mirroring the E4 session
  stack's push/pop shape and `ChildStateSpec`'s one-level-deep philosophy. `S` is
  whatever the consumer means by rendering context.
- **2C — no driver-owned state at all**: per-node logic controls recursion
  explicitly and passes state as an ordinary argument (`recurse(child, &new_state)`)
  — the pylatexenc recomposer's shape, where "threading" is just the consumer's own
  call stack.

**Recommendation: 2B and 2C are the same design at two altitudes — adopt the
2C-shaped core with 2B's vocabulary.** Concretely: the per-node callback receives
`(node, &state, cx)` where `cx` exposes emission (point 3) and
`cx.recompose_children(node, &child_state)?` / `cx.recompose_node(child, &state)?`
recursion ops (the driver owns traversal order and role-keyed defaults; the callback
picks the state each descent runs under). This keeps P4's "typed state threads
downward" literally true, needs no driver-held stack (the call stack is the stack,
zero residue — the E4 lesson), and gives takeover handlers full control (reorder,
repeat, or drop children output — pylatexenc's `descend_into_nodelist` parity).
`ParsingState` stays what per-node logic *reads*; `ParsingStateStack` stays
restage/synthesis vocabulary, mentioned in recompose docs only for the
detached-subtree recipe. Bounds rider: like restage visitors and `annotate`
callbacks, recompose callbacks get **no `Send`/`Sync` bounds** (T5-H(b) precedent —
GIL-bound FFI handlers are the primary consumer).

**Cost.** A generic `S` on the entry points (defaultable to `()` for stateless
strategies); one more parameter in the callback signature.

---

## 3. Output sink

**Background.** P4 §7 deferred "output sink type". T4 ruled the error principle:
techy error types uniformly `Clone`, out-of-crate information behind `Arc`
(T4_RULINGS round 1c); the panic policy demands `Result` everywhere
([§dd-dr:panic-policy]).

**Verified facts.** techy is `no_std` — core + alloc only, "the library performs no
I/O of its own" (lib.rs:13–18). Consequences: `std::io::Write` **cannot appear** in
the sink signature; `core::fmt::Write` and `alloc::string::String` are available.
`RestageError<E>` is the ruled error precedent — generic consumer error rides
through typed, `Clone where E: Clone` ([§dd-dr:restage-ops]). `fmt::Error` is a
`Copy` unit-like type, so it satisfies the Clone principle trivially.

**Options.**

- **3A — return `String` only.** Simplest; kills streaming; every large-document
  consumer pays full materialization. As the *only* API, rejected by the streaming
  consideration P4 explicitly routed here; as a convenience wrapper, free.
- **3B — generic over `W: core::fmt::Write`.** The std-idiomatic chunk sink that
  already exists (String implements it; `&str` chunks pass through without copying
  — span-backed content emits zero-copy). Sink failure = `fmt::Error`
  (information-free by design); std consumers wanting typed `io::Error`s use the
  standard adapter pattern (stash the real error in the adapter, re-extract after
  — the same pattern `std::fmt` itself uses), which the docs show once.
- **3C — a bespoke chunk-sink trait** (`write_str(&mut self, &str) -> Result<(), Self::Error>`
  with an associated error). Pros: typed sink errors without the side-channel.
  Cons: a second abstraction over what `fmt::Write` already is — the exact shape
  T4-C rejected for the FS trait ("a separate open/read trait: second abstraction,
  no techy-side consumer"); every adapter (String, Vec, io wrapper) must be written
  by techy or the consumer.
- **3D — callback sink** (`FnMut(&str) -> Result<(), E>`): 3C minus the trait name;
  same trade-offs, less discoverable.

**Recommendation: 3B + a `…_to_string` convenience entry (3A).** Signature shape:
`recompose(node, state, &mut logic, &mut sink) -> Result<(), RecomposeError<E>>`
with `RecomposeError<E> { Sink(fmt::Error), Logic(E), … }` (variant naming open),
`Clone where E: Clone` — the exact `RestageError<E>` symmetry. The consumer's
per-node logic keeps its own typed error channel `E` (panic policy: handlers fail
with `Err`, never panic); the streaming property comes free (emission happens in
document order for the re-emission strategies).

**Cost.** One generic parameter (`W`) on entry points; the io-adapter recipe is a
doc obligation (Phase 4, binding-guide chapter already carries the sink-adjacent
FFI notes).

---

## 4. Targeted-replacement integration (the latexpp archetype)

**Background.** latexpp: replace a targeted set of nodes, re-emit everything else
verbatim. The Phase-1b probe did this with span **gap-filling** (recurse children,
copy inter-child gaps; byte-faithful incl. tolerant-recovery nodes, targeted rewrite
"works first try" — FRICTION.md:137–151). T5-H has since **banned inter-node span
arithmetic**; the session must say what replaces the technique. P4 §7 lists
"targeted replacements" as a deferred item.

**Verified facts.** (a) The information the gap-filling walk extracted from
*between* nodes is, on parse trees, exactly the scaffolding complement — everything
else is inside per-node data (substrate table). (b) A node's **own** span is per-node
recorded data: `span_content()` is "level-1 verbatim recomposition — never needs an
external lookup" (node/node_ref.rs:74–79, [§dd-arch:nodes] recomposition levels).
Emitting an *untouched subtree* via its root's own `span_content()` involves no
inter-node arithmetic and no read-back beyond the node's own recorded content — it
is doctrine-clean, and on parse trees it is byte-exact by construction. (c) What is
NOT doctrine-clean: emitting a parent's scaffolding as the *complement* of its
children's spans (parent-minus-children is span arithmetic across nodes — it
resurrects deleted content the moment children were restaged; this is precisely the
hazard T5-H names). (d) Restage-then-recompose already works as a pipeline: replace
targets with synthesized `Chars` nodes (owned `TextContent`, `Source::synthesized`
provenance, source/source.rs:66–78), then re-emit per-node — the replacement text
flows out as ordinary chars content.

**Options.**

- **4A — per-node override in the recompose driver**: the consumer's per-node logic
  *is* the override point. Shipped shape: the preset's re-emission strategy is a
  public handler the consumer wraps — "if this node is a target, emit my
  replacement and skip the subtree; else delegate". The delegate's default for an
  untouched node: emit `span_content()` for the whole subtree and skip descent
  (fast path, byte-exact on parse trees); descend per-node only along paths that
  contain targets, emitting recorded scaffolding around the children (which
  requires point 6's residue design for the descend case).
- **4B — restage-then-recompose pipeline only**: no override machinery; latexpp
  passes are restages (targets → chars nodes), output is the plain re-emission of
  the result. Pros: one concept; the modified tree is a durable, validatable
  artifact (multi-pass latexpp composes). Cons: a tree rebuild per output run even
  for read-only emission; and the re-emission of the *restaged* tree needs the
  per-node residue anyway (the restaged environment's scaffolding must come from
  node data — spans of restaged neighbors are provenance, not layout).
- **4C — both (4A the recompose-owned surface, 4B the documented pipeline).**

**Recommendation: 4C.** They serve different phases of the same framework: 4A is
"emit with substitutions" (one pass, streaming, no new tree); 4B is "transform then
emit" (durable intermediate). Neither is a redundant spelling of the other
(different outputs), so one-canonical-path is not implicated. The decidable core:
**ratify subtree-own-span emission as the sanctioned replacement for gap-filling**
(4A's fast path — spans used per-node, all-or-nothing per subtree), with the
explicit contract that it is byte-exact only where the subtree is an unmodified
parse subtree — on restaged trees the node-data strategy (point 6) is the
re-emission path. This keeps latexpp's verified capability without re-promising the
withdrawn guarantee (the parse-law stays an in-crate oracle; the acceptance suite —
not consumer API — asserts `reemit(parse(s)) == s`, which is exactly T5-H(2)).

**Cost.** The wrap-a-strategy pattern must be genuinely ergonomic (a handler that
delegates needs the shipped strategy to be callable as a value, not a sealed
driver); this constrains point 1's callback shape (recommendation 2B/2C already
provides it).

---

## 5. The per-node recomposition doctrine, made operational (T5-H, BINDING)

**Background.** T5-H (T5_RULINGS.md H(c), amended into [§dd-dr:recompose]): spans
give provenance, not output location; recomposition is per-node; the parse-law is an
in-crate oracle; there is NO byte-reconstruction guarantee; `validate_parse_tree` was
withdrawn ([§dd-dr:tree-validation]). This point spells out what the doctrine
permits and forbids at implementation level, re-examines the span-verbatim strategy,
and delivers the verified gap inventory (substrate table above).

**Permits (implementation-level):**
1. Reading any field of the node's own payload, including resolving `Spanned`
   `TextContent` against the node's own source — that IS "a node's own recorded
   content" (`TextContent::resolve`, the accessors in node/node_ref.rs:179–263).
2. Emitting the node's own `span_content()` — its span is its own recorded datum
   (level-1; node/node_ref.rs:74–79). Corollary: whole-subtree verbatim emission at
   an untouched subtree root (point 4).
3. Reading the node's recorded `parsing_state()` — per-node data (pylatexenc
   precedent). *Flag for the session:* deriving trigger *spellings* from state (e.g.
   "the escape char is the state's command rule") is recorded-data-adjacent but
   ambiguous — `TokenRules.commands` is a `Vec<Arc<CommandRule>>` (token/rules.rs:159),
   so several escape chars can coexist in one state and the state cannot say which
   one fired. Recommend treating state-derived spellings as a *fallback*, not the
   design (point 6 records the datum instead).
4. Reading argument/slot records and recursing into children — structure is per-node
   data.

**Forbids:**
1. Computing or emitting the byte gap between two siblings' spans (the gap-filling
   walk).
2. Emitting a parent's span minus its children's spans (parent-complement
   scaffolding recovery — the technique [§dd-dr:environment-scaffolding] describes
   is *available to parse-tree analysis*, but recomposition must not build on it).
3. Reading source bytes outside the node's own span / recorded `TextContent` ranges.
4. Trusting a *slice's* covering span for emission (`NodeSlice::span()` is a query
   convenience under the T5-F1 contract, not an emission primitive).

**Span-verbatim strategy re-examined.** [§dd-dr:recompose] named span-verbatim as a
shipped strategy ("exact bytes via spans + gap-filling — the latexpp path"); the T5
amendment already narrows it ("its sound domain is unmodified parse trees"). Under
the doctrine, what survives of it is exactly permit #2: **own-span emission per
node/subtree**. As a standalone whole-tree strategy it is trivial
(`root.span_content()` — one call, no walk at all) and as a mixed-tree mechanism its
gap-filling core is banned. Recommendation: **retire "span-verbatim" as a named
shipped strategy**; re-express its value as (i) the trivial whole-tree case
(documented one-liner), and (ii) the untouched-subtree fast path inside the
targeted-replacement handler (point 4A). The node-data strategy becomes *the*
shipped re-emission strategy, completed by point 6. This is a conscious amendment to
[§dd-dr:recompose]'s "two shipped strategies prove the mechanism" sentence — the
mechanism is instead proven by the node-data strategy + the to-text mechanism split
(techy-totext). If the user prefers keeping two named strategies, the alternative is
to ship "verbatim" as the point-4A handler (span fast path + node-data descend) —
same code, marketed as a strategy; flag as a presentation choice.

**The oracle, precisely.** In-crate acceptance tests assert, for parse output of
each supported construct matrix: per-node re-emission == input bytes (strict + the
tolerant-recovery matrix — recovery artifacts are span-backed chars, substrate
table). This is the demoted parse-law's new job (T5-H(2)); it also carries the T5
I-18 multi-source obligation (recompose emits `\input{file}` per source; the
per-source acceptance test rides the input-wiring application). No consumer-facing
checker exists (`validate_parse_tree` withdrawn; `validate_tree` checks the
all-trees law only — [§dd-dr:tree-validation]).

**The gap inventory** is the substrate table above; its one-line summary for the
ruling: *chars, groups (incl. math + verbatim delimiters + argument brackets),
comments, lists, specials, and recovery artifacts are complete; macro-shaped
callables lack only the escape char; environment-shaped callables lack the entire
begin/end scaffolding; `ParagraphBreakStyle::Specials` nodes normalize.*

---

## 6. The trigger-spelling residue: precise Hidden-slot form

**Background.** T5-H's binding rider: environment scaffolding "could be stored by
the environment parser as Hidden slots (e.g. `"begin_tokens"`/`"end_tokens"`,
precise form TBD)" — turning scaffolding spelling into node data; explicitly this
session's item. Constraints in force: `SlotRole` is exhaustive with `Hidden` =
"framework/callable-defined attachments techy core ignores … semantics via slot
name + spec" ([§dd-dr:slot-roles] + T5-A9(iii)); readers/extract are role-blind
everywhere except recompose (T5-A9(ii)); restage descends into `Hidden` uniformly
(T5-A9(v)); the preset claims `SlotExt` for body marking ([§dd-dr:slot-roles]).

**Verified current-code facts.** [§dd-dr:environment-scaffolding] is the standing
*reconstruct-don't-record* decision, with recorded rejection reasons: per-environment
storage cost, and "a `Chars` node holding markup would violate chars-are-content".
Its revisit clause fires on per-instance-variable closing syntax; T5-H fires it for
a different reason (the doctrine needs the bytes as node data). The scaffolding
bytes at stake (substrate table): begin side = trigger escape char + command word +
token post-space + name-group delimiters + name; end side = the same for the
terminator. The builder's region check enforces arguments-then-slots tiling order
(node/builder.rs:149–170), which a source-leading `begin_tokens` slot violates as
coded. `env.body()` is `slot_content_nodes(0)` — "slot 0" sugar
(node/node_ref.rs:342–344); P4 already replaces it with the `BodySlotExt` ext axis,
so adding slots before the body does not break the ruled `body()` (it breaks only
the unruled positional habit).

### 6a. Which constructs get scaffolding storage?

- **Environments (incl. verbatim-form): YES** — the sketch's target; the only
  construct whose residue is multi-token and structurally variable (dispatcher word
  is a registration key; terminator word is composition config; name-group class is
  open).
- **Macros: NO Hidden slots — record the missing char on the node instead.** The
  residue is exactly one `char` (substrate table); a slot record + a `Chars` node
  per macro invocation (macros dominate real documents) to store 4 bytes is the
  wrong weight class. Recommend: `CallableData` gains an escape-spelling field —
  the division-of-labor rule puts invocation spelling on the node ("name … as
  written", node/kind.rs:202–204; the escape char is the missing half of "as
  written"). Shape options: `escape_char: Option<char>` (`None` for specials-formed
  and environment-shaped nodes) vs a `trigger: TextContent` full-spelling field
  (generalizes to the anticipated non-escape command syntaxes, token/rules.rs:72–73,
  but duplicates `name`). Recommend the `Option<char>` field now, revisit-if a
  non-escape command syntax actually lands.
- **Math delimiters, groups, comments, specials, `\verb`-delimited verbatim: NO** —
  already complete per-node (substrate table).
- **`\input`/Attached: NO** — the parent's own text is the trigger; recompose skips
  `Attached` by definition (point 8).

### 6b. What exactly sits in the two environment slots?

- **Option S1 — one span-backed `Chars` node per slot** (`begin_tokens` region =
  one chars node covering `\begin␣{itemize}` exactly as written, i.e. node-span
  start → first argument/body node; `end_tokens` = terminator start → node-span
  end), staged by the environment composition, `role: Hidden`,
  `SlotExt = make_scaffolding()`-style non-body value. Pros: two nodes + two slot
  records per environment, `materialize()` makes them source-independent for free
  (TextContent), recomposition emits the recorded bytes verbatim — pathological
  spacing (`\begin  {name}`) and exotic dispatcher words round-trip exactly. Cons:
  chars-are-content is breached for declared syntax residue — the old
  [§dd-dr:environment-scaffolding] objection, retired *here* by the Hidden role's
  own definition (declared, name-keyed, core-ignored); needs the honesty note that
  `display_tree`/walkers will show these nodes (role-blind reads, T5-A9(ii) — a
  feature: debug honesty).
- **Option S2 — structured slots** (`Chars("\begin")` + `Group{open,"{",name,"}"}`
  …): recomposition-grade structure with the name group as a real `Group`. Pros:
  navigable. Cons: the environment name then exists twice (CallableData.name + the
  group's chars) — a drift invariant to police; more nodes; no consumer identified
  for the extra structure.
- **Option S3 — payload fields instead of slots** (e.g. `begin_text`/`end_text`
  `TextContent` on an environment-specific record): rejected structurally —
  `CallableData` is core vocabulary and environments are a preset concept
  (no-privileged-concepts); slots exist precisely as the parser-minted, record-level
  channel.
- **Option S4 — preset stashes spans in `NodeExt`**: rejected — the preset's ext
  budget is `()` except `SlotExt` (P3/P4, [§dd-dr:latexlike-generalization] +
  [§dd-dr:slot-roles]); this would re-claim it.

**Recommendation: S1.**

### 6c. The Hidden-emission reconciliation (with point 8)

[§dd-dr:recompose] (P4 text) says "`Hidden` never participates"; the sketch stores
the very bytes re-emission must produce in Hidden slots. Resolution options:

- **R-a — semantics-via-name, as already ruled**: *generic* recompose machinery
  skips `Hidden` (it cannot know what a framework's hidden slots mean — the ruled
  definition); the **preset's own strategy consults its own declared slot names**
  (`begin_tokens`/`end_tokens`), exactly the "semantics via slot name + spec"
  mechanism [§dd-dr:slot-roles] assigns to `Hidden`. "Never participates" is
  amended to "generic strategies skip `Hidden`; a vocabulary-aware strategy may
  consult the hidden slots *it* defines".
- **R-b — a fourth `SlotRole` (e.g. `Scaffolding`)**: honest semantics
  (emit-as-syntax, in-source, in-byte-accounting) but re-opens the ruled exhaustive
  enum (T5-A9(iii) made adding a role a conscious breaking change — cheap *now*
  pre-application, but it un-rules a fresh decision and every role consumer grows an
  arm).

**Recommendation: R-a** — it is the mechanism `Hidden` was ruled to have; R-b
recorded as considered.

### 6d. Knock-ons to rule with it

1. **Builder tiling order**: relax node/builder.rs:149–170 from
   "arguments-then-slots chain from 0" to *order-free exact tiling* (collect all
   regions, verify they tile the child list — same O(n), order-insensitive), so
   `begin_tokens` can precede argument regions in the child list while remaining a
   slot. Child-list source order stays natural (begin, args…, body, end).
2. **Parse-law checker**: scaffolding bytes stop being "the reconstructible
   complement" and become tiled nodes — `check_tree_invariants`' callable arm
   (invariants.rs:38–45) simplifies at application; the in-crate oracle (point 5)
   gets *stronger* (fewer unrepresented bytes). The [§dd-dr:slot-roles] "Hidden = no
   byte accounting" clause needs a scoping amendment: in-span Hidden nodes account
   like any node; the exclusion is about *out-of-band* attachments.
3. **[§dd-dr:environment-scaffolding]** gets a supersession amendment
   (reconstruct→record reversal, with the doctrine as the trigger), and
   [§dd-arch:nodes]' "scaffolding is deliberately rigid and *reconstructed*, not
   recorded" sentence follows.
4. **post_space stays empty** on environment nodes (the begin-token's post-space now
   lives inside the `begin_tokens` bytes — no second home; kind.rs invariant-3
   wording gains a clause).
5. **Cost accepted**: +2 nodes, +2 slot records per environment (and the same for
   verbatim environments). Macro-shaped nodes grow one `Option<char>`.
6. **Naming flag** (see Naming section): "…_tokens" uses token-level vocabulary for
   node-level material — the terminology stack ([§dd-arch:naming]) treats that as a
   naming bug; alternatives below. The user's sketch names are the default if the
   collision is judged acceptable.

---

## 7. The read-only structural walker (T4 routing)

**Background.** T4-F29 REJECTED `Descendants::with_depth()` ("patched flat
iteration's structure loss"; superseded-names) and routed the honest structural read
here: `enter(node, depth) -> VisitFlow { Descend, SkipChildren, Stop }` + `exit(node)`
— "the skeleton of recompose … so the walk vocabulary is designed once"
([§dd-dr:recompose] T4 amendment). `descendants()` stays for structure-free queries.

**Verified facts.** `Descendants` yields nodes preorder off an explicit `u32` stack
with no depth, no exit events, no skip, no early stop (node/node_ref.rs:349–371).
The ruled traversal siblings: `RestageVisitor` = trait + closure blanket, reentrant,
`Result`-returning, variant `Restage::Descend(B)` ([§dd-dr:restage-ops]);
`annotate()` callback = storage-order, infallible. Three mechanisms must read as one
idiom (the session charter).

**Design questions + options.**

1. **Trait vs closure pair.** A trait `{ fn enter(node, depth) -> VisitFlow;
   fn exit(node, depth) {} }` with a blanket impl for `FnMut(NodeRef, usize) ->
   VisitFlow` (enter-only closures) mirrors `RestageVisitor`'s trait+blanket shape
   without its reentrancy motive. A bare closure *pair* parameter is the
   alternative — lighter but asymmetric (most walks need no exit) and blanket-less.
   Recommend the trait + enter-only blanket.
2. **Fallible?** Options: (a) infallible (`Stop` is the early exit; consumer errors
   ride captured state — the `annotate()` precedent); (b) `Result<VisitFlow, E>`
   (the `RestageVisitor` precedent). If recompose is *built on* the walker it must
   be (b); under point 1/2's recommendation recompose owns its recursion (value
   composition + state threading do not fit enter/exit without a shadow stack), so
   the walker serves read-only consumers and (a) suffices. Recommend (a), recorded
   with the reasoning; flip to (b) only if the user rules recompose-on-walker.
3. **Item + self-inclusion.** Item = `NodeRef` (gains the `A` default parameter at
   application per [§dd-dr:node-annotations]); `walk` visits the start node itself
   at depth 0 (unlike `descendants()`, which excludes self — the contrast sentence
   is a doc obligation). Depth = `usize`, relative to the walk root.
4. **Home + entry points.** `core::node`, beside `descendants`/`validate_tree`
   (T5-F3's placement logic: it walks *any* tree; recompose is a client). Entry:
   `NodeRef::walk(&mut v)` + `NodeTree::walk(&mut v)` sugar.
5. **Vocabulary unification.** `VisitFlow::Descend` deliberately matches
   `Restage::Descend` (ruled name, T5-A7) — "descend" = continue into children in
   both; `SkipChildren` is the read-side analog of a handled subtree (`Emit`);
   `Stop` has no restage analog (restage completes or errors). One doc table in the
   module shows the three mechanisms side by side (descendants = flat stream, walk =
   structural read, restage = rebuild) — that table is the "not three unrelated
   idioms" deliverable.
6. **Relationship to recompose.** Recommend: recompose *shares the vocabulary*
   (enter/exit phrasing, `Descend`/skip semantics in its role defaults) but runs its
   own recursion (point 2's `cx.recompose_children`), because the fold composes
   values and threads state — capabilities the read walker deliberately lacks.
   Alternative (recompose literally on the walker + internal emit stack) recorded:
   it buys one shared driver at the cost of making the walker fallible and
   stack-carrying for everyone.

**Recommendation:** the shapes above (trait + blanket, infallible, NodeRef + depth,
`core::node`, unified variant names, sibling-not-substrate to recompose).

**Cost.** One new public trait + enum + two entry points; a permanently stable
iterator type is *avoided* (the T4-E `ancestors()` lesson — a walker fn is smaller
API surface than an iterator adapter zoo).

---

## 8. The verbatim `Attached`-exclusion rule

**Background.** [§dd-dr:slot-roles]: `Attached` = derived/redundant, excluded from
the parent's byte-tiling; [§dd-dr:recompose]: "verbatim skips `Attached` by
definition (the invocation text IS the recomposition; descending is the explicit
expansion option)"; [§dd-dr:input-attachment]: recompose is per-source (emits
`\input{file}`, not the content). T5-A9(ii): readers role-blind *except recompose* —
recompose is the one sanctioned role-sensitive site. T5-A9(v): restage descends into
`Attached`/`Hidden` uniformly (contrast to record). The session spells the rule out
and reconciles `Hidden` (point 6c).

**Verified facts.** No role field exists in code yet (node/arguments.rs:325–334);
`\input` attachment itself is unapplied (no `parse_attached_source` in code). The
rule is therefore designed entirely against ruled structures.

**The rule (proposed formulation, decidable):** In the shipped re-emission
strategies, per-slot emission is role-driven —

- **`Content`: emit** (recurse into the slot's content region; region noise nodes
  are children like any others).
- **`Attached`: skip.** The parent's own recorded data (trigger + arguments +
  scaffolding) *is* the recomposition of the construct; the attached subtree lives
  in a different source by construction ([§dd-dr:input-attachment]) and emitting it
  would splice another source's text into this source's output. Descending is the
  **explicit expansion option**: the consumer's handler (which sees the node, its
  slots, and their roles) chooses to recurse into an `Attached` slot — a
  latex2text-style conversion typically will; a source-faithful re-emission never
  does. Expansion is a handler decision, not a driver flag (keeps the driver
  role-simple; the wrap-a-strategy pattern of point 4 is the override mechanism).
- **`Hidden`: skip in generic strategies; a vocabulary-aware strategy consults the
  hidden slots it itself declares** (point 6c's R-a — the preset's strategy emits
  `begin_tokens`, arguments, body, `end_tokens` in child-list order).

Plus two recorded contrasts: restage stays role-uniform (T5-A9(v) — transformation
must see everything); reads stay role-blind (T5-A9(ii) — `display_tree`, walkers,
and extract show attached/hidden reality; only recompose keys behavior on roles).
Multi-source note for the docs: per-source emission composes with per-file pipelines
(the T5-G rider — latexpp needs no tree merging).

**Recommendation:** adopt the formulation above as the ruled rule; it contains no
new mechanism, only the precise statement the agenda asked for, and its `Hidden`
clause is point 6c's R-a.

---

## Naming questions (checked against [§dd-arch:naming] + [§dd-dr:superseded-names])

None of the proposals below reintroduces a superseded name (checked against the full
register, incl. the T4/T5 blocks). Open questions for the session:

1. **The scaffolding slot names.** The sketch says `"begin_tokens"`/`"end_tokens"`.
   Terminology-stack check ([§dd-arch:naming]): *token* is a token-level term, and
   these slots hold *nodes* whose content is source text — "using a term at the
   wrong level is a naming bug". Alternatives: `"begin_scaffolding"`/
   `"end_scaffolding"` (matches the established design vocabulary in
   [§dd-dr:environment-scaffolding]), `"begin_syntax"`/`"end_syntax"`. The sketch
   name stands if the user judges the token echo acceptable (the slots do record
   what the begin/end *tokens* spelled). Also decidable: the strings live as preset
   `const`s (e.g. `latexlike::environments::BEGIN_SCAFFOLDING_SLOT`).
2. **The `CallableData` escape field**: `escape_char` (matches
   `TokenKind::Command`'s field, token/token.rs:72, and `CommandRule.escape_char`,
   token/rules.rs:77 — same fact, same name at every level) vs `trigger`-shaped
   names (rejected lean: *trigger* is token-level vocabulary and the field holds
   only the escape half).
3. **Entry/driver names**: module `techy::recompose` is ruled; the free fn
   `recompose(…)` + `recompose_to_string(…)` follow the module (no stutter concern —
   free fns read as `recompose::recompose`? If judged a stutter, `run`/`emit`
   alternatives exist; recommend flagging, leaning `recompose` as the bare verb at
   the *root* re-export level per the P1 facade rules).
4. **Error/context types**: `RecomposeError<E>` (the `RestageError<E>` symmetry;
   specificity rule — error of *what*); context type name for the callback's second
   parameter (`RecomposeContext<'…>`? mirrors `RestageContext` from
   [§dd-dr:restage-ops]); variant naming inside the error (`Sink` vs `Fmt`;
   `Logic` vs `Handler` vs `Visitor` — note `Visitor` is restage vocabulary; the
   recompose callback is not a visitor if point 7's recommendation holds, so a
   distinct noun avoids blurring the mechanisms; candidates: **handler** or
   **logic**; pick one and use it for the trait/callback name too).
5. **The per-node callback name**: P4 says "consumer supplies per-node logic".
   Candidate trait name if a trait is wanted for wrap-ability (point 4):
   `RecomposeLogic` / `RecomposeHandler` / `Recomposer` (pylatexenc precedent uses
   *recomposer*; bare `Recomposer` in the recompose module reads well and the
   context-determines-names rule applies — no sibling vocabulary competes).
6. **Walker names**: trait `NodeVisitor` (visitor of *nodes* — specificity; the
   `RestageVisitor` sibling makes `…Visitor` the family suffix) vs `TreeVisitor`
   vs `WalkVisitor`; control enum `VisitFlow` (T4's sketch name; no superseded
   conflict) with variants `Descend` / `SkipChildren` / `Stop` (Descend fixed by
   the Restage::Descend symmetry argument — confirm); entry `walk`. `LatexWalker`
   remains banned (superseded list) — not proposed.
7. **Strategy naming**: the preset re-emission strategy needs a name that does NOT
   collide with the verbatim construct family (`VerbatimBehavior`,
   `verbatim_state_delta` — same crate, sibling vocabulary competes, so "verbatim"
   alone is unavailable per naming principle 4). Candidates: `recompose_source` /
   `SourceSpelling` / `reemit_latexlike` — genuinely open; the latex2text-side
   "strategy" ships in techy-totext and needs no techy name.

---

## Resolved by prior rulings — do not re-litigate

- `techy::recompose` as a top-level module; mechanism-in-techy /
  content-in-techy-totext; strategies key on `SlotRole` (P4 §7,
  [§dd-dr:recompose]).
- The per-node doctrine itself, the withdrawn byte-reconstruction guarantee, the
  parse-law's demotion to in-crate oracle, the `validate_parse_tree` withdrawal
  (T5-H; [§dd-dr:tree-validation]) — this session *applies* the doctrine, it does
  not reopen it.
- `SlotRole` exhaustive; `Attached` over `Derived`; readers role-blind except
  recompose; restage descends uniformly (T5-A9). Body marking on the ext axis
  (`BodySlotExt`), preset claims `SlotExt` ([§dd-dr:slot-roles]).
- Walker routed here; `Descendants::with_depth()` rejected; `descendants()` stays;
  `ancestors()` rejected (T4-F29/E).
- `ParsingStateStack` name + owning shape + `from_node_ancestors`; pillar
  signatures ([§dd-dr:enclosing-state-stack] T5 amendment).
- No `Send`/`Sync` bounds on consumer callbacks (T5-H(b)); techy error types
  uniformly `Clone`, out-of-crate info behind `Arc` (T4); `Result` everywhere
  ([§dd-dr:panic-policy]); "honest" banned from rustdoc (T5-F1).
- No input caching; recompose emits `\input{file}` per source (T5-G,
  [§dd-dr:input-attachment]).
- One canonical path; shorthand-of-same-op is not a second path (P1, T1/T2-E1).

---

## Recommended rulings (numbered, decidable)

R1. **Architecture**: recompose is a **direct fold** with its own driver in
    `techy::recompose`; transform-to-chars + concatenate is demoted to a documented
    restage→recompose pipeline pattern, not the to-text mechanism. PLAN's companion
    sentence updated. (Point 1.)

R2. **Genericity split**: the driver and generic machinery are `L: Lang`-generic;
    only the trigger-spelling strategy is `LLL`-preset-side. (Point 1, ruled
    direction restated for confirmation.)

R3. **State**: the recomposition state is a **consumer-defined type parameter**,
    threaded by explicit descent (`cx.recompose_children(node, &state)`-shaped);
    not `ParsingState`, not `ParsingStateStack`. Per-node logic reads
    `node.parsing_state()` as recorded data; `from_node_ancestors` is the
    detached-subtree recipe pointer only. (Point 2.)

R4. **Callback bounds**: recompose per-node logic gets no `Send`/`Sync` bounds
    (T5-H(b) extension). (Point 2.)

R5. **Sink**: entry points generic over `W: core::fmt::Write` + a `…_to_string`
    convenience; error `RecomposeError<E> { sink-error, logic-error, … }`,
    `Clone where E: Clone`. `std::io::Write` structurally excluded (no_std);
    bespoke sink trait rejected. (Point 3.)

R6. **Targeted replacement**: both routes — (a) handler-level override with the
    **subtree-own-span fast path** ratified as the doctrine-clean replacement for
    gap-filling (byte-exact only on unmodified parse subtrees, stated); (b) the
    restage-then-recompose pipeline documented. (Point 4.)

R7. **Doctrine operationalized**: adopt point 5's permits/forbids list as the
    implementation contract (incl. permit #2 own-span emission; forbid #2
    parent-minus-children complements). State-derived spellings are fallback-only
    (the multi-escape-char ambiguity). (Point 5.)

R8. **Strategy roster**: retire "span-verbatim" as a named shipped strategy
    (its content = the one-liner + the R6(a) fast path); the node-data strategy is
    the shipped re-emission strategy. Amendment note on [§dd-dr:recompose].
    (Point 5; presentation alternative recorded.)

R9. **Scaffolding storage**: environments (incl. verbatim-form) store begin/end
    scaffolding as **two `Hidden` slots, each one span-backed `Chars` node**
    (option S1), minted by the environment composition; slot-name strings fixed as
    preset consts (name per naming Q1). (Point 6a/6b.)

R10. **Macro escape char**: `CallableData` gains `escape_char: Option<char>`
     (`None` for specials-formed and environment-shaped nodes); no Hidden slots for
     macros. (Point 6a.)

R11. **Hidden emission semantics**: generic strategies skip `Hidden`; a
     vocabulary-aware strategy consults the hidden slots it itself declares
     (amendment to "`Hidden` never participates"). No fourth `SlotRole`.
     (Point 6c.)

R12. **Knock-ons**: builder region-tiling check relaxed to order-free exact tiling;
     parse-law checker updated (scaffolding now tiled); supersession amendments on
     [§dd-dr:environment-scaffolding] + [§dd-arch:nodes] + the [§dd-dr:slot-roles]
     byte-accounting clause scoping. (Point 6d.)

R13. **Walker**: trait (enter/exit, defaulted exit) + enter-only closure blanket;
     **infallible**, `VisitFlow { Descend, SkipChildren, Stop }` exhaustive; item
     `NodeRef` + `usize` depth, self included at depth 0; home `core::node`,
     entries `NodeRef::walk`/`NodeTree::walk`; recompose shares vocabulary but owns
     its recursion. (Point 7.)

R14. **Attached-exclusion rule**: adopt point 8's role-driven emission formulation
     (Content emit / Attached skip with handler-level expansion / Hidden per R11);
     restage-uniform and reads-role-blind contrasts recorded in the docs.
     (Point 8.)

R15. **Oracle tests**: the in-crate acceptance suite asserts per-node re-emission
     round-trips (strict + tolerant matrices; multi-source rides the T5 I-18
     obligation). Phase 3 checklist item. (Point 5.)

---

## Session logistics (proposed order)

Ground truth first, then the structures that depend on it:

1. **Point 5** (doctrine operationalized + the gap inventory — everything else
   stands on it) → R7, R8, R15.
2. **Point 6** (the residue design; the largest new mechanism) → R9–R12.
3. **Point 1** (architecture) → R1, R2.
4. **Point 7** (walker — locks the traversal vocabulary before recompose signatures
   use it) → R13.
5. **Point 2** (state) → R3, R4.
6. **Point 3** (sink) → R5.
7. **Point 4** (targeted replacement — now expressible in the decided vocabulary)
   → R6.
8. **Point 8** (the exclusion rule, mostly confirmation) → R14.
9. **Naming sweep** (Qs 1–7).

Interim rulings file: `RECOMPOSE_RULINGS.md`, updated every round (session
pattern); durable records at close: amendments on [§dd-dr:recompose],
[§dd-dr:slot-roles], [§dd-dr:environment-scaffolding], [§dd-dr:span-invariants]/
[§dd-arch:nodes], plus new entries for the walker and the recompose surface, and
superseded-names additions per the naming rulings.
