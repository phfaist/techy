# Claude Conversation Export

- Session: `bc1edce0-e1b0-4e04-a69e-1e016735de1f`
- Project: `/Users/philippe/projects/techy`
- Exported: 2026-07-09T21:48:11

---

**User:**

/clear

---

**User:**

/model opus

---

**User:**

Set model to [1mOpus 4.8[22m and saved as your default for new sessions

---

**User:**

Down the road, I'd like users to be able to quickly write their own construct parsers, possibly after having installed some parser/flm structure based on techy.  What's the best model for this?  I can imagine (1) Commit to a scripting language (python), maybe even distribute as a python project (with internal rust usage for parsing), and all custom parsing classes are written in Python with convenience API and syntax.  (2) Have users write custom parsers in rust and compile "plug-ins" or extension libraries (sounds complex!)  (3) Embed an interpreter in the project itself, picking a scripting language (python? js? lua? ...), and use that to have users write extensions.  (4) something else?  Analyze and provide a recommendation.

---

**Assistant:**

This is a great strategic question and exactly the kind of design decision worth thinking through carefully before committing. Let me ground my analysis in where techy actually is today — the extensibility story depends heavily on how your spec/argument-parser layer is currently shaped.

Now I understand the shape. Let me look at how a "language"/library is assembled and the construct-parser boundary — that's where the extensibility story lands.

I read enough of the architecture to ground this. Here's my analysis.

## First, the reframe

The four options treat "how users write custom parsers" as a single choice, but your codebase already answers most of it. You don't have *one* extension point — you have a **tier stack**, and each tier has a different right answer:

| Tier | Boundary in your code | What it expresses | Who needs it |
|---|---|---|---|
| 0 — declarative data | `StdCallableSpec` (arguments + slots lists) | "`\myfrac` takes 2 args", a new environment, optional args | ~90% of users |
| 1 — argument logic | `ArgumentParser` trait (standard impls from the preset) | delimited/optional/verbatim/comma-list variants | selected, not written |
| 2 — full takeover | `make_invocation_parser()` → `ConstructParser` | `\verb`-like novel token-level syntax | rare |
| 3 — whole language | `Lang` trait + `SpecLookup` | a new markup dialect | preset authors |

So "let users write their own construct parsers" is really **two very different questions**: (a) how do they *declare* macros/environments (Tier 0), and (b) how do the rare cases supply *custom logic* (Tier 2). Conflating them is what makes Options 1–3 each look like a whole-project commitment when they aren't.

## Two constraints in your code that decide this

Reading `callable.rs` and `constructs/mod.rs`, two things jump out that veto parts of the naive options:

1. **`Send + Sync`, `Arc`-shared, stored in the parsed tree.** Specs live inside nodes and the tree must be `Send + Sync`. Python/Lua/JS interpreter handles are the opposite — `!Send`/`!Sync` or gated behind a global lock (GIL, single Lua state). Embedding a scripting *callable inside a spec* (the heart of Options 1 and 3) fights this contract directly. Declarative data has no such problem.

2. **`no_std` + `alloc` core.** You're keeping WASM/embedded on the table. Baking a CPython (or any) runtime into the core crate throws that away. Whatever scripting exists must be an *optional crate on top*, never in core.

These two facts, more than taste, are why I'd steer away from Options 1 and 3 as **core** strategies.

## The options, scored against that

**Option 1 (commit to Python, ship as a Python project):** Conflates *packaging* with *extension model*. A `pip install techy` wheel (via maturin) is a fine **distribution target** — but making Python the *extension core* means Python callbacks on the parse path, FFI cost per invocation, and the `Send+Sync`/`no_std` violations above. It also contradicts your stated Rust-first/generic philosophy. Keep it as *a* binding, not *the* model.

**Option 2 (users write Rust plugins, compiled):** Right for Rust embedders, but be precise about *how*. Rust has no stable ABI, so **dynamic `dylib` plugins are a known trap** (must match compiler + dep versions exactly; `abi_stable` is a heavy workaround). The viable form is **static linking — extensions are crates that impl your traits and get compiled in.** That's already free in your design. It's the max-performance path but useless for non-programmer document authors.

**Option 3 (embed an interpreter):** The heaviest option and only justified by one specific need — **extensions traveling inside untrusted documents, run install-free and sandboxed.** If that need is real, don't embed CPython; use a small embeddable engine (`mlua` or `rquickjs`). But you'd still pay the `Send+Sync` bridge cost, so it stays a feature-flagged crate behind the Tier-2 boundary. Premature today.

**Option 4 (what I recommend): a layered hybrid.**

## Recommendation

Build the extension story as tiers, matching the boundaries you already have:

1. **Invest first in Tier 0 as a data format.** A macro/environment **signature mini-language** (xparse/pylatexenc-style: `m m O{default}`, `{{[`) plus a spec file format (RON/TOML) that the preset compiles into `StdCallableSpec`s. This is where the vast majority of "I want my own construct" lives, it needs **zero scripting**, it's `Send+Sync` and `no_std`-clean by construction, and it dodges the entire language-commitment question. **This is the highest-leverage thing to build.**

2. **Keep Rust traits as the native Tier-2 escape hatch, via static linking** (crates that impl `ConstructParser`/`ArgumentParser`), not dynamic plugins. Free, already designed, serves embedders and preset authors.

3. **Add first-class PyO3 Python bindings as a separate crate — as the primary *scripting* surface, not the core.** Your existing audience (pylatexenc/FLM = Python scientific writers) gets a convenient Python API to author specs and, for genuinely custom logic, Python callbacks at the Tier-2 boundary *only*. Because Tier 0 handles the common case in Rust, the FFI cost is paid rarely, off the hot path. This captures Option 1's audience benefit without Option 1's cost.

4. **Defer a general embedded interpreter (Option 3)** until a concrete "untrusted, document-bundled, install-free, sandboxed" requirement appears. If it does, `mlua`/`rquickjs` behind a feature flag — never CPython, never in core.

Net: **the answer is (4), and the surprising part is that "let users write custom parsers" is mostly solved by *not* making them write parsers** — a declarative data layer — with scripting reserved as a narrow, optional escape hatch delivered as a binding crate on top.

## The one question that pivots this

Everything above assumes a distribution of authors. The single fact that would sharpen priorities: **who is the primary extension author you're optimizing for, and how custom is their typical need?**

- Python-savvy scientific writers extending FLM, mostly *declaring* macros → prioritize the Tier-0 data format + PyO3 bindings; embedded interpreter never needed.
- Rust developers embedding techy in a larger tool → Tier-0 format + the trait/static-linking path; skip Python for a while.
- Non-programmer authors shipping self-contained documents with custom logic → this is the *only* profile that pulls the embedded-interpreter option forward.

Which of those is closest to the users you're picturing? That determines whether Python bindings or the declarative format comes first, and whether Option 3 ever needs to exist.

---

**User:**

Okay, perhaps I didn't express myself clearly.  It is clear that the core library will remain no_std and will not know of anything like python or lua.  I'm only talking about an additional layer on top, e.g., a reimplemented FLM project that uses techy as its core parser.  It could be based on python.  So none of these options affect core.  I was reassured (by you/Fable in an earlier conversation) that the Send+Sync+Arc structure would not interfere negatively with python/js extensions, I forget exactly why but IIRC in python the GIL enforces a sufficient degree of thread safety.  Then: Declaring macros by data is only possible for trivial replacement macros (perhaps with arguments), but cannot execute logic (e.g. "typeset this only if the argument #2 is empty").  So I want a convenient way of expressing such logic without replicating a TeX engine/LaTeX logic commands; here a programming language such as python is much more powerful.  I'm thinking a declarative-heavy way of expressing definitions but with the ability to set custom code/logic of the macro expansion.  Then I'm only talking about callable-spec-level extensions, lang-level and other such things are out of scope and are possibly by writing the suitable code in rust that uses techy in the desired way.  But an occasional user just might have to write a custom parser, which we just might as well alow if they're expressing their macros in python anyways.

---

**User:**

Okay, perhaps I didn't express myself clearly.  It is clear that the core library will remain no_std and will not know of anything like python or lua.  I'm only talking about an additional layer on top, e.g., a reimplemented FLM project that uses techy as its core parser.  It could be based on python.  So none of these options affect core.  I was reassured (by you/Fable in an earlier conversation) that the Send+Sync+Arc structure would not interfere negatively with python/js extensions, I forget exactly why but IIRC in python the GIL enforces a sufficient degree of thread safety.  Then: Declaring macros by data is only possible for trivial replacement macros (perhaps with arguments), but cannot execute logic (e.g. "typeset this only if the argument #2 is empty").  So I want a convenient way of expressing such logic without replicating a TeX engine/LaTeX logic commands; here a programming language such as python is much more powerful.  I'm thinking a declarative-heavy way of expressing definitions but with the ability to set custom code/logic of the macro expansion.  Then I'm only talking about callable-spec-level extensions, lang-level and other such things are out of scope and are possibly by writing the suitable code in rust that uses techy in the desired way.  But an occasional user just might have to write a custom parser, which we just might as well alow if they're expressing their macros in python anyways.  Revise your analysis with this scope in mind.

---

**Assistant:**

That clarification changes the shape of the answer a lot. With core off the table and the scope being "a layer on top (FLM-on-techy) where callables are declared largely declaratively but can carry real logic," the analysis collapses onto one distinction that I want to nail first, because the whole design hinges on it.

## Parse-time vs. render-time — your example is render-time

"Typeset this only if argument #2 is empty" is **not a parsing decision**. When techy sees `\mymacro{a}{}`, parsing's entire job is to build a `MacroNode` with two named arguments. Nothing about the arg signature depends on what #2 contains. The "only if #2 empty" logic runs later, when your FLM layer **walks the finished tree to produce output** (HTML/LaTeX/text).

This matters because techy — as I read the architecture (`token → constructs → node`, `ParserSession::finish → NodeTree`) — is a *parse-to-AST* engine with **no TeX-style expansion**. Like pylatexenc/FLM, it deliberately does not expand macros into re-fed tokens; it hands you nodes and you interpret them. `\newcommand` pushing a library via `ParsingStateDelta::push_library` is *definition registration*, not expansion.

So your "macro logic" splits onto two axes that have completely different answers:

- **Axis A — how args/slots are recognized (parse-side).** Declarative. A signature string. Runs in the hot Rust loop.
- **Axis B — what the callable *means* / produces (semantic-side).** This is where all your interesting logic lives. It is **not in techy at all** — it's your layer walking the NodeTree. It runs *once, at render time, off the parse path*.

The consequence is the good news: **the logic you care about never touches the parse hot loop.** Python here is not a performance concern the way it would be if it ran per-token.

## The model

A callable definition in your FLM layer = **declarative parse-signature + a host-language semantic hook**:

```python
@macro("emph", args="m")                 # A: declarative signature → compiled to a techy Std spec
class Emph:
    def render(self, node, ctx):         # B: arbitrary Python, runs at render time
        return ctx.emph(node.args["text"])

@macro("mymacro", args="m m")
class MyMacro:
    def render(self, node, ctx):
        if node.args[1].is_empty():      # "only if #2 empty" — pure render-time logic
            return ctx.render(node.args[0])
        return ...
```

Wiring:

- **The signature** (`"m m O{}"`) is passed to Rust and compiled into a `StdCallableSpec` using the preset's standard `ArgumentParser`s (exposed over PyO3). **No Python object enters techy's spec tree**, and no Python runs during parsing. The declarative part stays fully Rust-side.
- **The `render` hook** lives in *your layer's own registry*, keyed by name/callable-type, and is invoked when you walk techy's `NodeTree`. techy never sees it.

So for the common case, the `Send+Sync`/GIL question doesn't even arise — techy's `Arc<dyn CallableSpec>` holds pure data, and Python callables live entirely in the Python layer above the AST.

## On the Send+Sync/GIL point you remembered

Fable was right, and here's the mechanism so it's not just faith: PyO3's `Py<T>` handles **are** `Send + Sync`, precisely because you can't touch the wrapped object without a `Python<'py>` token (i.e. holding the GIL). The GIL serializes access, which is exactly what `Sync` promises. So a Python callable *can* legally sit inside a `Send+Sync` techy trait object.

But note: you mostly **won't** need that, because of the split above. The *only* time a Python object must live inside a techy `Arc<dyn …>` is the escape hatch:

## The escape hatch (your "occasional user writes a custom parser")

If someone needs genuinely custom *parsing* (not rendering) — a `\verb`-like delimiter, a novel argument form — expose techy's `ArgumentParser` / `ConstructParser` as subclassable Python types via PyO3. The user writes a `parse()` method; PyO3 wraps it as a Send+Sync `Py`, GIL-serialized. This one **does** run during parsing (slow, GIL-bound), but only for that single construct, only when someone opts in. Entirely acceptable for the rare case, and "free" ergonomically since they're already authoring in Python.

That gives you a clean, honest gradient:

| Need | Where it goes | Language | On parse hot path? |
|---|---|---|---|
| Declare macro/env + args | signature string → `StdCallableSpec` | data | no (pure Rust) |
| Macro *semantics* ("if #2 empty…") | render hook in your registry | Python | no (render time) |
| Custom *parsing* of a construct | PyO3 `ArgumentParser`/`ConstructParser` | Python | yes, rare/opt-in |
| New dialect, `Lang`, perf-critical | Rust using techy directly | Rust | — |

## Which host language for the layer

For an FLM reimplementation, **Python is the right call** — the audience and heritage are Python, and "occasional user writes a 5-line macro without a Rust toolchain" is the entire value proposition. Endorse it.

One thing I'd bake in deliberately: make the **signature grammar itself language-neutral and Rust-owned** (a string grammar parsed by the preset), so Python only supplies *a string + the logic*. Then the exact same bridge shape supports a JS layer or a native-Rust definition layer later, over one core, without re-deciding the declarative surface. The host-language choice becomes a per-layer decision, not something baked into techy — which is the property you want, since you've said lang-level work is "Rust using techy directly." A Rust-native definition layer (definitions as structs impl-ing a `render` trait) remains available as the max-performance alternative for anyone who doesn't want Python in the loop.

## Net revision

The earlier "declarative-first, defer scripting" advice was aimed at the wrong layer. Rescoped to your on-top FLM layer, the recommendation is:

**Definitions are declarative on the parse axis (signature → Rust spec, no host code on the hot path) and imperative on the semantic axis (a host-language render hook, invoked off the parse path). Python is the right host for the FLM layer; the rare custom *parser* is a PyO3 trait-subclass escape hatch. No embedded interpreter, no expansion engine — just AST-in, host-callbacks-at-render-time.**

Two things I'd want to pin down next, because they shape the API:

1. **Is your semantic hook a single "render to output" callback, or a richer node-transformation/interpretation step** (FLM has a notion of features/fragments and multi-pass processing)? That decides whether the hook signature is `render(node, ctx) -> output` or something that can also rewrite/annotate the tree before rendering.
2. **Do you want the signature DSL to be able to name arguments** (so `render` reads `node.args["label"]` not `node.args[1]`)? techy already supports named args/slots — worth surfacing that all the way up, since it makes the "if #2 empty" style code far more robust to signature changes.

---

**User:**

So comparison of options (1)--(4)?

---

**Assistant:**

Direct head-to-head, re-read for the **on-top layer** (not core). The single most important thing to state up front:

**Performance is not the deciding axis.** In *every* option, parsing is techy (Rust) and your semantic logic runs off the hot path (render time). So all four give you a fast parse. What actually differs is **who owns the process, author ergonomics, ecosystem access, deployment, and sandboxing.**

| | **(1) Python host**<br>techy as native module | **(2) Rust-compiled** extensions | **(3) Rust host + embedded interp** | **(4) Binding contract** *(rec)* |
|---|---|---|---|---|
| **Owns `main`** | Python; `import techy` | Rust | Rust; loads scripts | Rust core + per-layer binding |
| **Author writes** | Python | Rust | 3a: Python · 3b: Lua/JS | Python (or Rust-native layer) |
| **Author toolchain** | pip, no compiler | full cargo/Rust | none (scripts) | pip / (cargo for Rust path) |
| **Python ecosystem** (numpy, pip pkgs) | ✅ | ❌ | 3a: ✅ · 3b: ❌ | ✅ (Python layer) |
| **Parse hot path** | Rust ✅ | Rust ✅ | Rust ✅ | Rust ✅ |
| **Self-contained binary** | ❌ needs Python | ✅ | 3a: ❌/painful · 3b: ✅ | Python: ❌ · Rust: ✅ |
| **Sandbox untrusted doc-embedded extensions** | ❌ | ❌ | 3a: ❌ · 3b: ✅ | only if you add 3b later |
| **Packaging effort** | moderate (maturin, mature) | low | 3a: high · 3b: low | moderate |
| **Fit: occasional macro author** | excellent | poor | 3a: excellent · 3b: good (new lang) | excellent |

### Verdict per option

**(1) Python host — the strong default for your stated goal.** Python owns the process, techy is a maturin-built native module, authors write `@macro`-style definitions with zero compiler. Parsing stays in Rust; render logic is Python off the hot path. Gives the FLM audience full Python ecosystem access for free. Costs: not a single self-contained binary (needs a Python present), and no sandbox. For *trusted, locally-authored* extensions — which is the FLM case — those costs are irrelevant.

**(2) Rust-compiled — right tool, wrong user.** This serves people *building applications on* techy, not the occasional macro author. Dynamic plugins are an ABI trap (no stable Rust ABI), so realistically it means static linking = recompile. Keep it available as a *Rust-native definition layer* for max-perf/single-binary consumers, but it is not the FLM authoring answer.

**(3) Embedded interpreter — only coherent with a *non-Python* engine.** This is the key finding: **3a (embed CPython) is strictly dominated by Option 1** — same ecosystem benefit, but you take on libpython-linking/bundling pain and gain nothing, because if you want Python, letting Python own the process (Option 1) is simpler. **3b (embed Lua/QuickJS)** is a genuinely *different product*: it buys a self-contained, sandboxable binary at the price of abandoning the Python ecosystem (authors write Lua/JS, no `import`). Justified **only** if you need untrusted, document-bundled extensions run install-free — e.g. an `.flm` file that carries its own macro logic and you compile it in a sandbox. That's a real but separate requirement.

**(4) Binding contract — how to not paint yourself into a corner.** Define the layer by a **language-neutral, Rust-owned definition contract** (declarative signature grammar + a semantic-hook interface). Then:
- ship **Option 1 as the flagship** authoring experience (Python/pip),
- keep a **Rust-native layer (Option 2 flavor)** available over the same contract for tool builders,
- hold **3b (Lua/QuickJS) in reserve** for the sandbox/document-bundled use case if it ever materializes.

### Bottom line

For what you described — Python-savvy FLM authors, declarative-heavy definitions with real logic in the hooks, trusted local extensions — **choose Option 1 as the concrete answer, structured as Option 4 so you're not locked in.** Options 2 and 3 aren't competitors to 1; they're specializations for *different users* (Rust tool builders) and *different deployments* (sandboxed self-contained compiler) that you can add later over the same core without revisiting this decision.

The one fact that would change the pick: **do you ever need to run untrusted, document-supplied extension code?** If yes, 3b enters as a *second* surface alongside 1; if no (the usual FLM assumption), Option 1 alone covers it.

---

**User:**

I'm worried about users having to maintain a usable python distribution with standard libraries/any other relevant libraries to run the project on.  What's my best option to distribute a clean product (no dependency on user-maintained python distrib) while still exposing a customization layer (e.g. in python)?

---

**Assistant:**

Good instinct — that worry is exactly the thing that rules out "Option 1 as normally shipped." But first, a distinction that opens up the answer:

**"Depends on Python" ≠ "user maintains Python."** You can ship the runtime *inside your product*. Once you do that, the user installs nothing and manages nothing. So the goal isn't "no Python" — it's "**you** own the runtime, not the user." There are two clean ways to do that, and they differ only in *which* runtime you embed.

The architecture is the same for both: **Rust owns the process and the scripting engine is bundled into your artifact** (this is Option 3, done properly — the opposite of "user runs `python yourthing.py`"). techy core stays untouched; this is all the on-top layer.

## Route A — embed a pure-Rust interpreter (cleanest possible product)

The engine compiles *into* your binary. One self-contained executable, cross-platform, and — importantly for techy — **WASM still works**. Nothing on the user's machine.

| Engine | Language authors write | Ecosystem | Sandbox | Notes |
|---|---|---|---|---|
| **RustPython** | real Python 3 | ❌ no C-ext (no numpy) | partial | Fullest "it's Python" feel; slower than CPython; WASM-able |
| **Starlark** (`starlark-rust`) | Python *dialect* | ❌ | ✅ strong, deterministic | "Python for programmable config"; restricted (bounded loops, no recursion) — often fine for macro logic |
| **Rhai** | its own (Rust-ish) | ❌ | ✅ | Trivial to embed, safe, but not Python |
| **QuickJS/boa** | JavaScript | ❌ | ✅ | Relevant if browser/JS is a target |

The catch is uniform: **no third-party Python libraries.** But your macro logic ("typeset only if #2 empty," string/format work, light computation) is almost always *self-contained* — it doesn't need numpy. For that profile, Route A is strictly the cleanest product you can ship.

## Route B — bundle a relocatable CPython (full ecosystem, heavier)

If authors genuinely need the real Python ecosystem, you still don't require a user-maintained distro — you **ship a standalone CPython inside your artifact**:

- **`python-build-standalone`** (astral-sh) — relocatable, self-contained CPython builds. This is what `uv`/`Rye` ship; it's the industrial answer. You link it via PyO3 and package it with your binary.
- **PyApp** / **`cargo dist`** — wrap it into per-platform installers.
- **PyOxidizer** — single-binary embedding, but its development has slowed; I'd lean on python-build-standalone instead today.

Cost: a ~10–40 MB artifact, per-platform packaging, and **no WASM** (CPython doesn't embed in the browser without Pyodide's heavyweight machinery). Benefit: real Python, `import` works.

## Recommendation

**Adopt the Route A architecture — Rust owns `main`, engine embedded — and default to a pure-Rust interpreter.** That gives you the "clean product, nothing to install, single binary, WASM-capable" property you're after, which also matches techy's no_std ethos. Reserve bundled-CPython (Route B) as an optional "full-ecosystem build" for the day a user truly needs real libraries — same architecture, heavier engine, so it's an additive later choice, not a fork you must decide now.

For the engine itself, the pick hinges on two questions:

1. **Do macro authors need real third-party Python libraries, or is it self-contained logic?** Self-contained → Route A (and you're done). Needs the ecosystem → Route B.
2. **Is the browser/WASM a target?** (FLM has historically run in-browser.) If yes → pure-Rust interpreter or JS engine; *avoid* bundled CPython. If no → Route B stays open.

If both answers are "self-contained logic, and yes WASM matters," my concrete lean is **RustPython** (if the selling point is "you write Python") or **Starlark** (if you want sandboxing/determinism and the logic is config-shaped). Both are just crates you bundle — the exact "clean product, no user-maintained Python" outcome you want.

Where do you land on those two questions — is the ecosystem (numpy-style libraries) ever needed, and is in-browser a target?

---

**User:**

Thanks, I got a good picture.  Now on a different track:  Is there any way I can embed a minimal rust compiler/JIT so that extensions can be compiled in rust from within the main project execution?  It would be amazing if users could write their extensions and they would work in different execution environments (python/js).

---

**Assistant:**

Straight answer first, because there's a common misconception buried in the question: **no, you can't practically embed a "minimal Rust compiler."** There is no such thing as a minimal rustc.

## Why embedding a Rust compiler doesn't work

- **rustc is monolithic and huge.** It *is* the front-end (parse, type-check, borrow-check, MIR) plus a codegen backend (LLVM by default). There's no stable, embeddable "compiler-as-a-library" API — `rustc_driver`/`rustc_interface` are nightly-internal, unstable, and used by things like rust-analyzer/Miri that pin a specific nightly. Embedding it means shipping the whole toolchain + LLVM (hundreds of MB) — the exact opposite of the "clean product, nothing to install" goal you set last turn.
- **Cranelift isn't a Rust compiler.** It's a *codegen backend* that can JIT, but it eats IR (CLIF/MIR), not Rust source. `rustc_codegen_cranelift` still needs rustc's entire front-end to produce that IR. So "Rust JIT" = rustc front-end + Cranelift = still the whole compiler.
- The only real way to compile Rust at runtime is to **shell out to an installed `rustc`/`cargo`** (what the Rust Playground does, in a sandbox). That reintroduces precisely the user-maintained-toolchain dependency you're trying to eliminate.

So runtime Rust compilation is off the table for a clean product.

## But the dream underneath is real — and it's WASM

You said the *goal* is: "users write extensions and they work in different execution environments (Python/JS)." That's **write-once, run-in-many-hosts** portability — and you guessed *runtime compilation* as the mechanism. The actual mechanism for that is **WebAssembly**, and it's better on every axis:

- **Compilation is ahead-of-time (author-side), not JIT (runtime).** The extension author compiles their Rust to a `.wasm` module. Your product ships only a small **WASM runtime**, never a compiler.
- **One artifact runs in every host.** The same `.wasm` runs under a native host (Wasmtime/Wasmer), under **Python** (`wasmtime-py`, or Pyodide), and under **JS/browser** (the browser's own WASM engine, or Node). Exactly your "works in Python/JS" requirement.
- **Language-agnostic authoring.** Extensions can be Rust, but also C, Zig, AssemblyScript, TinyGo — anything that targets WASM.
- **Sandboxed by construction** — which also solves the "untrusted, document-bundled extensions" case from the earlier thread.
- **Matches techy's nature** — no_std/WASM-friendly core, so this is coherent end to end.

The tradeoff you *do* accept: the **extension author** needs a Rust→wasm build step. You don't get "type Rust in a box and it runs" without shipping a compiler. But you get "compile once → ship a `.wasm` → runs everywhere," which is what you actually described.

## Tooling

| Approach | What it gives you | When |
|---|---|---|
| **Extism** | Turnkey cross-language plugins: host SDKs for Python, JS, Rust, Go, …; a PDK to *write* plugins in Rust/JS/Go. Literally "one plugin, many hosts." | You want the plumbing solved; fastest path to your dream |
| **Wasmtime + Component Model + WIT** (`wit-bindgen`) | Standards-based **typed** interfaces — define `render(node) -> output` in WIT, generate host+guest bindings. More control. | You want a clean, versioned, typed plugin ABI you own |
| **wasmi** (pure-Rust interpreter) | Small, no-LLVM, no_std-ish; runs even *inside* WASM (browser) | Minimal footprint / running plugins where a native JIT runtime can't go |

Note the host runtime differs per environment (Wasmtime native, the browser's engine in-browser, wasmtime-py in Python) — but the **plugin artifact is identical** across all three. That's the property you want.

## The synthesis with the previous thread

Here's the part that ties both conversations together: you can run a **scripting interpreter compiled to WASM**. Extism, for instance, supports JS and Python plugins by bundling QuickJS / a Python runtime *as WASM*. So an extension can be "a small script + its interpreter, packaged as one `.wasm`" — giving you **both** "author writes a script, no compile step" **and** "runs in native/Python/JS." That unifies the RustPython/Starlark discussion with this one: WASM is the portable *substrate*, and on top of it you can offer either compiled-Rust plugins (fast, AOT) or scripted plugins (interpreter-in-WASM, no author build step) — both cross-host.

## techy-specific caveats

- **Render/semantic hooks over WASM are clean**: pass a serialized node (or a handle + host-functions), get output back, off the parse hot path. Ideal.
- **Custom *parse* hooks over WASM are heavier**: you'd cross the boundary inside the parse loop with live token access — more interface surface and marshaling cost. Keep it for the rare escape hatch, or keep custom parsers native-Rust.
- **The real engineering is data marshaling, not compilation** — how nodes cross the boundary (serialized bytes vs. handles into host memory). That's where your design effort goes.

## Bottom line

Don't embed a Rust compiler — it can't be minimal or clean. **Make WASM your portable extension format:** extensions are AOT-compiled `.wasm` modules; each host (native, Python, JS) embeds a WASM runtime and loads the *same* module. Start with **Extism** if you want cross-host plumbing out of the box, or **Wasmtime + Component Model/WIT** if you want a typed interface you control. That delivers exactly "write an extension once, run it in different execution environments," sandboxed, with no toolchain shipped to end users — and it fits techy's no_std/WASM grain perfectly.

---

**User:**

Is there a recommendable good WASM-based plugin architecture out there I can read up on?

---

**Assistant:**

Yes — there's good, mature prior art. Let me pull current links and confirm status rather than give you stale pointers.

Yes — there are two canonical approaches (a turnkey framework and a standards-based one), plus a few real production systems whose architecture is close enough to yours to copy. Here's a curated reading path.

## 1. Start here — the turnkey cross-host framework: Extism

This is the closest thing to the exact goal you described ("one plugin, runs in Python/JS/native hosts"). Extism is an abstraction over WASM runtimes with **idiomatic host SDKs for Python, Node/JS, Rust, Go, …** and **PDKs** (Plugin Development Kits) for *writing* plugins in Rust/JS/Go. The host↔plugin contract is a simple **bytes-in/bytes-out** interface plus host functions — you don't design an ABI.

- [extism.org](https://extism.org/) and the [FAQ/architecture](https://extism.org/docs/questions/) — read how Hosts + PDKs + the bytes-in/bytes-out model fit together.
- [github.com/extism/extism](https://github.com/extism/extism) — main repo.
- [Extism Python SDK](https://github.com/extism/python-sdk) and [JS SDK](https://extism.github.io/js-sdk/) — literally your "same `.wasm` loaded from a Python host and a JS host" story.
- [moonrepo's WASM-plugins guide](https://moonrepo.dev/docs/guides/wasm-plugins) — a real Rust dev-tool that ships Extism-based plugins; small, concrete example to imitate.

**Why for techy:** fastest way to prototype the cross-environment extension layer. Its bytes-in/bytes-out model maps directly onto "serialize a node in, get rendered output back." Trade-off: you get an untyped byte channel rather than a typed interface.

## 2. The standards-based route — WebAssembly Component Model + WIT

If you'd rather own a **typed** interface (define `render(node) -> output` in an IDL, generate host+guest bindings), this is the direction the ecosystem is standardizing on. WIT is the interface language; `wit-bindgen` generates the boilerplate; Wasmtime is the reference runtime.

- [component-model.bytecodealliance.org](https://component-model.bytecodealliance.org/) — the official book; start at the intro, then [Running components in Wasmtime](https://component-model.bytecodealliance.org/running-components/wasmtime.html).
- [Building Native Plugin Systems with WebAssembly Components](https://tartanllama.xyz/posts/wasm-plugins/) (Sy Brand) — the best hands-on tutorial for exactly a *plugin system*.
- [Building a plugin system with the Component Model](https://dev.to/topheman/webassembly-component-model-building-a-plugin-system-58o0) — another step-by-step.
- [WASI & Component Model current status](https://eunomia.dev/blog/2025/02/16/wasi-and-the-webassembly-component-model-current-status/) — where WASI 0.2/0.3 stands, so you don't build on shifting ground.

**Gotcha to note:** host and plugin must share the same WIT version, or bindings mismatch at runtime — so you'll want a versioned-API strategy from day one.

## 3. Read the code of a system shaped like yours — Zed's extensions

This is the single most instructive real-world example: a **Rust application** with **user-authored extensions** that define language behavior, built on the Component Model. It's essentially your situation (host app + third-party logic extending a language pipeline).

- [Life of a Zed Extension: Rust, WIT, Wasm](https://zed.dev/blog/zed-decoded-extensions) — excellent narrative walkthrough of WIT, `wit-bindgen`, the `wasm32-wasip2` target, versioned host APIs, and how they made host calls *async* transparently.
- [Zed Extensions System (DeepWiki)](https://deepwiki.com/zed-industries/zed/13-extensions-system) — architecture map (Extension Host vs. WASM runtime, manifest-driven registration).

**Why for techy:** copy their split of "Extension Host (lifecycle/registration) vs. WASM runtime (execution)," their versioned WIT API, and the manifest (`extension.toml`) approach for declaring what a plugin provides — which pairs nicely with your declarative-signature idea.

## 4. Case study for *untrusted* extensions in a product — Shopify Functions

If document-supplied/untrusted extensions ever matter, this is the reference for running untrusted third-party WASM in production under tight limits.

- [How Shopify Uses WebAssembly Outside of the Browser](https://shopify.engineering/shopify-webassembly) — security/perf/flexibility rationale, Wasmtime linker, multi-tenant untrusted execution.
- [Bringing JavaScript to WebAssembly for Shopify Functions](https://shopify.engineering/javascript-in-webassembly-for-shopify-functions) — how they let authors write **JS**, compiled-to-WASM via an embedded engine (the "interpreter-in-WASM" pattern we discussed — script authoring *and* cross-host portability).
- [Shopify/shopify-function-wasm-api](https://github.com/Shopify/shopify-function-wasm-api) — their input/output ABI, including a compact **NaN-boxed** encoding for passing structured data across the boundary.

**Why for techy:** the marshaling design (how structured data crosses the boundary compactly) and resource limits (fuel/time caps) are exactly the problems you'll hit passing nodes to a render hook.

## Suggested order & how it maps to techy

1. **Skim Extism** (§1) to see the turnkey cross-host model working end-to-end — this validates the whole "Python/JS/native from one plugin" premise quickly.
2. **Read the Zed blog** (§3) to see the typed, production version of the same idea in a Rust host — this is your architectural template.
3. **Skim the Component Model book** (§2) to decide whether you want Extism's byte channel or a WIT-typed interface for your `render(node) → output` boundary.
4. **Dip into Shopify** (§4) when you design node marshaling and (if needed) sandboxing/limits.

Net: **Extism = fastest path and free multi-host SDKs; Component Model (à la Zed) = a typed interface you own.** Both let extension authors write Rust (or JS, or others) that compile once to `.wasm` and run under your native, Python, and JS hosts — which is the write-once/run-anywhere property you were reaching for with the Rust-JIT idea, delivered cleanly.

Sources:
- [extism.org](https://extism.org/) · [Extism FAQ/architecture](https://extism.org/docs/questions/) · [extism/extism](https://github.com/extism/extism) · [Python SDK](https://github.com/extism/python-sdk) · [JS SDK](https://extism.github.io/js-sdk/) · [moonrepo WASM plugins](https://moonrepo.dev/docs/guides/wasm-plugins)
- [Component Model book](https://component-model.bytecodealliance.org/) · [Wasmtime + components](https://component-model.bytecodealliance.org/running-components/wasmtime.html) · [Sy Brand: Native Plugin Systems with WASM Components](https://tartanllama.xyz/posts/wasm-plugins/) · [dev.to: building a plugin system](https://dev.to/topheman/webassembly-component-model-building-a-plugin-system-58o0) · [WASI/Component Model status](https://eunomia.dev/blog/2025/02/16/wasi-and-the-webassembly-component-model-current-status/)
- [Life of a Zed Extension](https://zed.dev/blog/zed-decoded-extensions) · [Zed Extensions System (DeepWiki)](https://deepwiki.com/zed-industries/zed/13-extensions-system)
- [How Shopify Uses WebAssembly](https://shopify.engineering/shopify-webassembly) · [JS in WASM for Shopify Functions](https://shopify.engineering/javascript-in-webassembly-for-shopify-functions) · [shopify-function-wasm-api](https://github.com/Shopify/shopify-function-wasm-api)

---

**User:**

Nice.  Can I compile python code into wasm, in case an extension writer wants to use python?

---

**Assistant:**

Short answer: **yes, but with an important asterisk** — you don't compile Python *to* WASM the way you compile Rust. Python needs its interpreter, so what you actually ship is **a Python runtime compiled to WASM, bundled with the author's script**. That's a solved problem with good turnkey tooling. Let me pull current links so I'm not guessing at project status.

Confirmed — and the search clarifies the nuance. Here's the accurate picture.

## The asterisk: "compile Python to WASM" ≠ what "compile Rust to WASM" means

Python isn't compiled to native code, so in almost every case you're **shipping a CPython (or MicroPython) interpreter compiled to WASM, with the author's `.py` bundled in**. The one genuine "AOT-compile Python" tool is Wasmer's **py2wasm**, which transpiles Python→C→wasm via a Nuitka fork — but even that links CPython's API internally. It reaches ~70% of native speed (2.5–3× faster than the interpreter). So: totally doable, just know you're carrying a runtime.

## The design fork that actually matters

There are two ways to let an author "use Python," and they give very different developer experiences and artifact sizes:

**(A) Compile each Python plugin into its own `.wasm`.** Uniform "every plugin is a WASM module" model — your host only knows WASM, regardless of source language.

**(B) Embed *one* Python-interpreter `.wasm` in your host and feed it user `.py` scripts.** The extension is then just a `.py` text file, no build step for the author, one shared runtime. (This is exactly the "interpreter-in-WASM" pattern Shopify used for JS, and it unifies "author writes a plain script" with "runs in every host.")

## The concrete tools

| Tool | Approach | Fits | Size | Ecosystem | Notes |
|---|---|---|---|---|---|
| **componentize-py** (Bytecode Alliance) | A — Python → **WASM Component** implementing your WIT world | The typed Component-Model / Zed-style route | multi-MB (bundles CPython) | limited C-ext | `pip install componentize-py`; author implements your `render(node)->output` WIT interface |
| **MicroPython-wasm** | B — embed tiny interpreter, feed `.py` | "occasional user writes a 5-line macro" | **~303 KB base** | subset stdlib, **no numpy** | dramatically smaller; sandboxed; great for small render logic |
| **CPython `python.wasm`** (WASI) | A or B | full-language fidelity | ~75–150 MB with stdlib | limited C-ext | official WASI build; heavy |
| **py2wasm** (Wasmer, Nuitka) | A — AOT Python→wasm | perf-sensitive, literal "compile" | multi-MB | limited | ~70% native speed |
| **Pyodide** | B (browser/Node) | need real numpy/pandas | tens of MB | **full scientific stack** | Emscripten target; JS/browser host, not generic WASI |

## The caveats to weigh

- **Size.** A Rust plugin is tens of KB; a CPython-bundled Python plugin is *megabytes*. If you go route (A) with CPython, every plugin carries an interpreter — mitigate by route (B) (one shared runtime) or by using MicroPython. This is the single biggest reason to prefer (B) for Python.
- **No ecosystem except Pyodide.** numpy/pandas and other C-extensions only really work under Pyodide (Emscripten/browser). MicroPython and WASI-CPython give you the *language*, not the *PyPI ecosystem*.
- **Performance:** interpreted Python-in-wasm is slow-ish, but your render hooks are off the parse hot path, so it's fine.

## For techy specifically

Given your profile — small render/semantic logic ("if #2 empty…"), no numpy, "occasional author" ergonomics — my lean:

- **Best author experience:** route (B) with **MicroPython-wasm**. The extension is a plain `.py` file, ~300 KB shared sandboxed runtime, runs under any host's WASM engine. Simon Willison wrote this exact pattern up recently ([Running Python in a sandbox with MicroPython and WASM](https://simonwillison.net/2026/Jun/6/micropython-in-a-sandbox/)) — directly worth reading.
- **If you commit to the typed Component-Model route (Zed-style):** use **componentize-py** so Python authors produce a component implementing your WIT interface — uniform with Rust/JS plugins, at the cost of multi-MB artifacts.
- **Reserve Pyodide** for the "author genuinely needs numpy, host is JS/browser" case.

Net: yes, Python extension authors are fully supported. Decide **(A) uniform WASM components** (componentize-py) **vs. (B) one embedded interpreter + plain `.py` scripts** (MicroPython) — for your "small logic, easy authoring" goal, (B) with MicroPython is the sweet spot, with componentize-py as the path if you want everything to be a typed component.

Sources:
- [componentize-py (GitHub)](https://github.com/bytecodealliance/componentize-py) · [Component Model: Python guide](https://component-model.bytecodealliance.org/language-support/building-a-simple-component/python.html) · [Fermyon: Python, Wasm, and componentize-py](https://www.fermyon.com/blog/python-wasm-componentize-py)
- [py2wasm announcement (Wasmer)](https://wasmer.io/posts/py2wasm-a-python-to-wasm-compiler)
- [CPython WASI build (python/cpython)](https://github.com/python/cpython/tree/main/Tools/wasm) · [Adding Python WASI to Wasm Language Runtimes](https://wasmlabs.dev/articles/python-wasm32-wasi/)
- [MicroPython in a sandbox — Simon Willison, Jun 2026](https://simonwillison.net/2026/Jun/6/micropython-in-a-sandbox/) · [simonw/micropython-wasm](https://github.com/simonw/micropython-wasm)
- [Pyodide](https://pyodide.org/)

---

**User:**

/md-export docs/BrainstormPluginExtensions.md

---

**Assistant:**

I'll export this conversation to the specified path.
