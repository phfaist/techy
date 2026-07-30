# FRAMEWORK-ANALYSIS — can a framework be built on techy's public API today?

Persona: developer building an extensive framework on techy, exposed through Python
(PyO3). Evidence: runnable probes in `probes/` and the working PyO3 module in
`techy-py/` (see FRICTION.md for raw results).

---

## (A) latex2text-class: LaTeX → readable text with overridable per-macro handlers

**What techy gives you today.**
- The complete input side: parse with a spec database (`Package` + `MacroSpec` /
  `EnvironmentSpec` / `SpecialsSpec`), tolerant recovery with diagnostics, spans on
  everything.
- A closed structural `NodeKind` (chars/group/callable/comment/list) that makes the
  render dispatch total: five cases, compiler-checked; callables then dispatch on
  `callable_type()` + `name()`.
- Both handler-attachment models work *today* (probed):
  1. pylatexenc-shape: a framework-side `HashMap<(CallableType, name), Handler>` —
     the direct rewrite target for `MacroTextSpec`/`EnvironmentTextSpec`/
     `SpecialsTextSpec` (l2t's `simplify_repl` database);
  2. spec-side: a custom struct implementing `CallableSpec<Latexlike>` (all methods
     defaulted) that *carries* its textify rule, recovered at render time via the
     documented `Any`-downcast contract. One database for parse + render — this is
     structurally *better* than pylatexenc, where latexwalker specs and l2t specs are
     two parallel databases that drift.
- Recursion is fully served: `argument_content_nodes(i)` (content view, delimiters
  stripped), `body()` for environments, math groups with `math_style()`, and
  `parsing_state().mode()` on every node for mode-sensitive rendering.
  Absent optional arguments are honest (`None`), so handlers need no sentinel logic.

**What you must build.** The renderer itself (trivial — the probe is ~80 lines), the
default handler database (the real work, as in pylatexenc's `_defaultspecs.py` — a
techy "standard macros" database does not exist yet and l2t needs it), text-layout
postprocessing (paragraph reflow, `strict_latex_spaces` options), and unicode tables
(`\alpha` → α).

**What is awkward.**
- `post_space` gotcha (probed): `$\alpha + x$` renders "α+ x" unless the handler
  re-emits `node.post_space()`. pylatexenc has the same underlying model but its l2t
  hides it centrally; a techy-based l2t must decide the policy once, centrally —
  needs a documented recipe.
- No `parent()`: context-sensitive handlers ("am I inside a heading?") must carry
  context down the recursion or precompute a parent map. Recursive renderers mostly
  don't care; pass-style ones do.
- pylatexenc's l2t receives the *whole document context* (`l2tobj`) in callables;
  the equivalent here (a `&Renderer` argument) works fine — no API obstacle.

**Verdict: SUFFICIENT today.** No blockers; one doc-level gotcha; the missing piece is
content (a standard spec database), not API.

---

## (B) FLM-class: a custom LaTeX-like language + semantic layer + multiple render targets

**What techy gives you today.**
- Runtime extensibility on the preset is genuinely strong: packages with custom
  spec types (spec-side data + `Any` downcast), custom `ArgumentSpec`s with
  parsing-state deltas (`\text{…}` probe from the guide), `EnvironmentSpec` with body
  deltas, and — importantly — `EnvironmentBehavior` as an *open trait* on the preset
  (custom body parsing, e.g. verbatim-like or tabular-like bodies) plus
  `CallableSpec::make_invocation_parser` as the full-takeover escape hatch. A large
  fraction of "FLM = LaTeX-like language with its own constructs" is expressible as
  *data on the Latexlike preset*.
- Multi-pass semantic processing over the immutable tree works cleanly with
  `HashMap<NodeId, T>` side tables (probed): NodeId is `Copy+Eq+Ord+Hash`, trees are
  frozen, passes are independent walks. For a Python-exposed framework this is also
  the natural shape (Python dicts keyed by node handles).
- The two-tier node-ext system **is** reachable from the public API — but only by
  implementing your own `Lang`: probed end-to-end (custom `NodeExtTypes` bundle,
  tier-1 ext written in `Lang::finalize_node` at the single mutation boundary, read
  via `NodeRef::ext()`).

**The fork in the road (the central B finding).** `Latexlike` hard-codes
`NodeExts = ()`, `StateExt = ()`, `Mode = {Text, Math}`. A language needing its own
node exts, state flags, or modes must implement `Lang` — and then *nothing from the
preset is reusable* (compile-probed: `LatexlikeDriver`, `MacroSpec`,
`default_token_rules()`, `base_package()` are all `Latexlike`-monomorphic). The
migration path from "FLM as preset data" to "FLM as its own Lang" is a cliff: you
re-implement the driver, the spec types, the token-rule defaults, and the base
package. The lang.rs docs name FLM as an intended *full implementor* — consistent —
but then the preset should either be generic over a Lang family, or its parts
liftable (e.g. `default_token_rules::<L>()` given `L::GroupTypeId: From<GroupType>`
or a re-usable driver core). Today the ext system is effectively unreachable *for
preset users*, and side tables are the only semantic-layer option without the cliff.

**What you must build.** The semantic pass framework, render targets (they are
consumers, no API needed beyond A's), the FLM spec database; if custom Lang: the
whole preset fork.

**What is awkward / prevented.**
- The cliff above (fork-the-preset) — the one structural obstacle.
- Side tables keyed by NodeId die at a transform boundary (new tree = new ids, and
  there is no old↔new id mapping — see cross-cutting below); an FLM pipeline that
  parses → transforms → renders must re-derive its semantic tables per stage.
- Per-argument/`ArgumentExt`-style caching (the lang.rs motivating example: parsed
  `{domain,key}` next to a `\ref` argument) is unreachable on the preset for the same
  `NodeExts = ()` reason.

**Verdict: SUFFICIENT for the "FLM on the preset + side tables" architecture;
RESTRUCTURING NEEDED (preset genericization or liftable parts) before the "FLM as its
own Lang with node exts" architecture is practical.**

---

## (C) latexpp-class: byte-faithful preprocessing/rewriting

**What techy gives you today.**
- **Byte-faithful reconstruction works** (probed on a document with optional args,
  environments with args, verbatim env, `\verb`, math both styles, comments,
  paragraph breaks, specials, and *tolerant-recovery* nodes for `\foo` and a stray
  `}`): every byte of the input is covered by the node structure — recurse children,
  copy inter-child gaps from the source. Tolerant recovery staging things as chars
  nodes (rather than dropping them) is what makes this hold even on erroneous input;
  that matters enormously for latexpp, which must not eat bytes on documents it
  half-understands.
- **Targeted rewrite works**: replace one node's subtree, emit all other bytes
  verbatim via span gap-filling — first try, exact.
- Node data alone (structure-only, no raw-source gap reads) reconstructs everything
  *except the trigger spellings* (`\emph`, `\begin{itemize}`…): group delimiters,
  comment parts, post-spaces are all `TextContent` data with pinned positions
  (the invariants doc), argument brackets are group delimiters. So a latexpp that
  works span-wise needs the source (always reachable: `span().source()`); one that
  works tree-wise needs preset knowledge only for trigger spellings.

**What you must build.** The emit/patch engine (trivial, ~40 lines probed); the rule
framework (latexpp's "fixes"); multi-file orchestration via `SourceResolver` (I/O is
the embedder's by design — fine for a Python framework).

**What is awkward.**
- The gap-filling walk re-derives what invariants.rs already knows (partition,
  contiguity). A public `recompose(node) -> Cow<str>` / "verbatim emit with
  replacements" helper would make level-1 recomposition a one-liner and make the
  invariant a *promise* instead of an emergent property the framework re-checks.
- No parent links / no positional index: a rule that wants "replace this node" has to
  drive the walk from the root (fine) or build maps (fine); minor.
- Multi-source documents (`\input` via resolver): each node's span knows its source,
  so per-file reconstruction still works; the probe covered single-source only —
  noting the untested edge.

**Verdict: SUFFICIENT today — this is the strongest archetype.** The span partition
invariant + tolerant recovery staging + `TextContent` positional payloads are exactly
what latexpp needs; only convenience is missing.

---

## Cross-cutting: node-tree transformation infrastructure

The user is considering a general tree→tree transformation framework (latex2text as
transforms-to-string-nodes + concatenation; FLM passes). What the public API offers
today (all probed):

**Works:**
- `NodeTreeBuilder` is public and complete: `new/add/add_with_ext/staged_nodes/finish`
  with validated staging (child claiming, region tiling, `TextContent` residency) —
  building synthesized trees from outside a parse works.
- `Language::initial_state()` supplies a legitimate `Arc<ParsingState>` for
  synthesized nodes; `Source::synthesized(content, description, triggered_at)` gives
  replacement content real provenance (chain preserved: `provenance_chain()`).
- All node-data types are publicly constructible (`NodeKind::chars/group/callable/…`,
  `GroupData::new`, `CallableData` struct literal, `ChildRegion::new/single`,
  `ContentNodes`, `ParsedArgument::provided/absent`, `From<Vec<ParsedArgument>>`).
- `Lang::finalize_node` runs on every re-staged node (idempotence is part of its
  contract — the hook is transform-aware by design).
- Extract helpers (`Split::into_tree`, `KeyVals::into_tree`) show the intended
  owned-subtree outputs exist in-crate.

**Missing / blocking for a transformation framework:**
1. **No public subtree copy.** node/copy.rs does resolved→staged region translation
   internally but is `pub(crate)` ("a public transform surface is a later phase's
   design"). The DIY reimplementation (probed, ~60 lines) requires understanding the
   two-phase region contract, `InRegion` vs `InChildrenOf` recovery via
   `content_parent() == node.id()`, and offset arithmetic — the exact kind of subtle
   bookkeeping a library should own. Every naive attempt fails fast
   (`RegionAlreadyResolved`) — good error, but the fix shouldn't be "reimplement
   copy.rs".
2. **No BuildId→NodeId correlation from `finish()`.** Old→new node maps (which FLM
   semantic tables and latexpp patch logs need) dead-end at `BuildId`; after `finish`
   you re-find nodes heuristically (probe used span equality). `finish` returning
   `(NodeTree, impl Fn(BuildId) -> NodeId)` or a map would close this.
3. **No transform-tree validation tier.** The builder accepts mixed-origin trees;
   `check_tree_invariants` rejects them (parse-tree law, explicitly deferring
   transform trees). Nothing in between checks what *should* hold for a spliced tree
   (structure + region tiling + per-node residency, minus cross-source span
   partition).
4. **Slice accessors can lie on spliced trees**: `NodeSlice::span()/source_text()`
   detect mixed sources only via first/last nodes; a replaced *middle* node yields the
   original source text, silently including replaced material. Fine under parse-tree
   law, a footgun the moment transforms exist.
5. `NodeRef::tree()` being `pub(crate)` forces `&NodeTree` to be threaded alongside
   every `NodeRef` in transform code that resolves region ids.

**Capabilities a transformation framework needs from techy** (beyond fixes to 1–5):
- a `TreeTransformer`/rebuild-visitor seam: walk old tree, default = copy, override =
  replace/drop/splice, returning new tree + id correspondence;
- a documented "synthesized node" recipe (which parsing_state to use, when
  `Source::synthesized` vs `Source::new`, provenance conventions) — everything exists,
  nothing says how to combine it;
- optionally a string-concatenation finisher (transform-to-chars + concatenate =
  latex2text), which is just (A)'s renderer expressed as a transform.

---

## Ranked list of API changes/additions for a framework builder

Legend: [blocker] = a framework architecture is impractical without it;
[friction] = costs real code/knowledge at every consumer; [nice] = convenience.
(A) additive, (R) restructuring.

1. **[blocker] (A) Public subtree-copy / rebuild-visitor** (expose copy.rs's
   translation via a transform surface on `NodeTreeBuilder` or a `TreeTransformer`).
   Unlocks the whole transformation-framework plan; today's DIY requires crate-innards
   knowledge. (Explicitly planned "later phase" — this walkthrough confirms it is the
   #1 need.)
2. **[blocker for custom-Lang FLM] (R) Make the latexlike preset reusable across
   `Lang`s** — genericize driver/specs/rules/base package over a Lang family (or
   provide lifting adapters). Without it, `NodeExts`/`StateExt`/modes are unreachable
   for any language that is 95% latexlike, and the ext system serves only full forks.
3. **[friction] (A) `finish()` → old/new id correspondence** (BuildId→NodeId map or
   remap handle). Needed by FLM semantic tables and latexpp patch logs across
   transform boundaries; today only heuristic re-finding.
4. **[friction] (A) Parent navigation**: either store parent indices in `NodeTree`
   (flat `Vec<u32>`, cheap) or ship a public `ParentMap` helper; plus
   `index_in_parent()`. Every binding and every pass-style consumer rebuilds this.
5. **[friction] (A) Recomposition helper**: `recompose(node)`/`emit_with(replacements)`
   codifying the gap-filling walk (and its guarantees) that (C) proved correct — turns
   40 lines + an invariant assumption into a supported one-liner.
6. **[friction] (A) Transform-tree validator + honest spliced-slice accessors**
   (`check_transform_tree_invariants`; make `NodeSlice::span()` scan for source
   uniformity or document the middle-node case loudly).
7. **[nice] (A) Stable kind-name strings** (`NodeKind::name() -> &'static str`) for
   bindings/debuggers.
8. **[nice] (A) `NodeRef::tree()` made public** (or region-resolution helpers taking
   `NodeRef` only) to unclutter transform code.
9. **[nice] (A) Binding-oriented doc page**: the Arc+NodeId handle pattern, the
   debug-tag caveat on cross-tree ids, `post_space` re-emission recipe, severity
   exhaustiveness — all learned the hard way here, all one page of docs.
