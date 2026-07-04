//! Definition libraries: how callable names resolve to [`CallableSpec`]s.
//!
//! Implements ARCHITECTURE.md §specs (Phase 4; `SpecLookup` semantics decided July 2026,
//! DESIGN_RATIONALE.md §3.4):
//!
//! - [`SpecLookup`] is the resolution *behavior* extension point. It receives a
//!   [`CallableQuery`] — invocation form, name, syntax context, and (when available) the
//!   triggering token — plus the current [`ParsingState`], so a preset's implementation
//!   can be mode-aware (dispatch on `state.ext()`) without the core privileging any mode.
//! - [`Library`] is the standard data-driven implementation: a plain map keyed by
//!   `(CallableTypeId, normalized name)`, many-to-one to shared specs (flyweight). It
//!   ignores the query's syntax/token context and the state.
//! - [`LibraryStack`] is the ordered stack stored in the parsing state
//!   ([`StateData::libraries`](crate::state::StateData)): innermost/last-pushed wins
//!   (lexical shadowing — `\newcommand` semantics; no `ConflictStrategy`), plus the
//!   per-[`CallableTypeId`] unknown-callable fallback singletons behind
//!   [`resolve()`](LibraryStack::resolve).
//!
//! Extending definitions mid-parse = pushing a library via
//! [`ParsingStateDelta::push_library`](crate::state::ParsingStateDelta::push_library);
//! popping happens naturally because the previous `Arc<ParsingState>` is restored when
//! the group ends (reversion is structural, deltas are never inverted).

use alloc::boxed::Box;
use alloc::collections::BTreeMap;
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::fmt;

use crate::spec::{CallableSpec, CallableTypeId};
use crate::state::{Lang, ParsingState};
use crate::token::Token;

/// The syntax through which a callable invocation was recognized.
///
/// Carried on a [`CallableQuery`] so lookups can disambiguate between coexisting command
/// syntaxes — with several [`CommandRule`](crate::token::CommandRule)s in scope, `\foo`
/// and `#foo` both tokenize as `Command { name: "foo" }`, and the escape character is
/// *not* recoverable from the token alone (a lookup has no access to the source content
/// behind the token's span).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CallableSyntax {
    /// A command token: escape character + name (`\foo`).
    Command {
        /// The escape character that fired ([`CommandRule::escape_char`](crate::token::CommandRule)).
        escape_char: char,
    },
    /// A specials trigger (queried from inside a `Lang::scan_specials` implementation,
    /// before any token exists).
    Specials,
    /// Anything else — synthesized invocations, preset-defined resolution paths.
    Other,
}

/// A callable resolution request: everything a [`SpecLookup`] may dispatch on, minus the
/// parsing state (passed alongside).
///
/// The `token` is present when resolution happens for an already-scanned token (the nodes
/// parser resolving a `Command`, Phase 6) and lets lookups inspect `pre_space` and spans;
/// it is `None` where no token exists yet (specials scan, synthesized invocations). Plain
/// libraries ignore everything but `callable_type` and `name`.
pub struct CallableQuery<'a, 's, L: Lang> {
    /// The invocation form being resolved (e.g. the preset's `MACRO`).
    pub callable_type: CallableTypeId,
    /// The name to resolve. Libraries store *normalized* names; normalization is the
    /// caller's (preset's) business — the core matches exactly.
    pub name: &'a str,
    /// The syntax context of the invocation.
    pub syntax: CallableSyntax,
    /// The triggering token, when one exists.
    pub token: Option<&'a Token<'s, L>>,
}

impl<'a, 's, L: Lang> CallableQuery<'a, 's, L> {
    /// A query without token context.
    pub fn new(
        callable_type: CallableTypeId,
        name: &'a str,
        syntax: CallableSyntax,
    ) -> CallableQuery<'a, 's, L> {
        CallableQuery { callable_type, name, syntax, token: None }
    }

    /// Attach the triggering token.
    pub fn with_token(mut self, token: &'a Token<'s, L>) -> CallableQuery<'a, 's, L> {
        self.token = Some(token);
        self
    }
}

// Manual impls: derives would demand `L: Clone`/`L: Debug` although only references and
// plain data are stored.

impl<L: Lang> Clone for CallableQuery<'_, '_, L> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<L: Lang> Copy for CallableQuery<'_, '_, L> {}

impl<L: Lang> fmt::Debug for CallableQuery<'_, '_, L> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CallableQuery")
            .field("callable_type", &self.callable_type)
            .field("name", &self.name)
            .field("syntax", &self.syntax)
            .field("token", &self.token)
            .finish()
    }
}

/// Resolution behavior: from a [`CallableQuery`] (+ parsing state) to a spec.
///
/// The dyn extension point for definition lookup. Implementations may dispatch on the
/// query's syntax/token context and on `state.ext()` (mode-aware lookup — e.g. FLM
/// resolving `\vec` differently in math mode); the standard [`Library`] is a plain map
/// that ignores both. Returning `None` means "not defined here" — unknown-callable
/// policy lives in [`LibraryStack::resolve`], not in individual lookups.
pub trait SpecLookup<L: Lang>: fmt::Debug {
    /// Resolve `query` in this lookup, or `None` if it is not defined here.
    fn lookup(
        &self,
        query: &CallableQuery<'_, '_, L>,
        state: &ParsingState<L>,
    ) -> Option<Arc<dyn CallableSpec<L>>>;
}

/// A named collection of callable definitions: the standard data-driven [`SpecLookup`].
///
/// Keys are `(CallableTypeId, normalized name)`, **many-to-one**: several names may map
/// to one shared spec (flyweight — `\emph` and `\textit` can share an `Arc`). Within one
/// library, inserting an existing key replaces the previous spec.
///
/// (Storage is a `BTreeMap` — the crate is `no_std`, `alloc` has no `HashMap`. Revisit
/// only if profiling flags lookup cost.)
pub struct Library<L: Lang> {
    name: Box<str>,
    specs: BTreeMap<CallableTypeId, BTreeMap<Box<str>, Arc<dyn CallableSpec<L>>>>,
}

impl<L: Lang> Library<L> {
    /// An empty library. The name identifies the library in diagnostics and debugging.
    pub fn new(name: impl Into<Box<str>>) -> Library<L> {
        Library { name: name.into(), specs: BTreeMap::new() }
    }

    /// This library's name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Define `name` (normalized spelling) under the invocation form `callable_type`.
    /// Returns the spec previously defined under that key, if any.
    pub fn insert(
        &mut self,
        callable_type: CallableTypeId,
        name: impl Into<Box<str>>,
        spec: Arc<dyn CallableSpec<L>>,
    ) -> Option<Arc<dyn CallableSpec<L>>> {
        self.specs.entry(callable_type).or_default().insert(name.into(), spec)
    }

    /// The spec defined under `(callable_type, name)`, if any.
    pub fn get(
        &self,
        callable_type: CallableTypeId,
        name: &str,
    ) -> Option<&Arc<dyn CallableSpec<L>>> {
        self.specs.get(&callable_type)?.get(name)
    }

    /// Number of definitions across all callable types.
    pub fn len(&self) -> usize {
        self.specs.values().map(BTreeMap::len).sum()
    }

    /// Whether the library holds no definitions.
    pub fn is_empty(&self) -> bool {
        self.specs.values().all(BTreeMap::is_empty)
    }
}

impl<L: Lang> SpecLookup<L> for Library<L> {
    fn lookup(
        &self,
        query: &CallableQuery<'_, '_, L>,
        _state: &ParsingState<L>,
    ) -> Option<Arc<dyn CallableSpec<L>>> {
        self.get(query.callable_type, query.name).cloned()
    }
}

impl<L: Lang> fmt::Debug for Library<L> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Library")
            .field("name", &self.name)
            .field("definitions", &self.len())
            .finish()
    }
}

/// The ordered lookup stack stored in the parsing state, plus the per-type
/// unknown-callable fallback policy.
///
/// Resolution order: **innermost (last-pushed) first** — lexical shadowing. The
/// fallbacks are consulted only by [`resolve()`](LibraryStack::resolve), after the whole
/// stack has missed; a fallback spec is a shared singleton (possible because specs are
/// de-keyed), so "unknown `\foo`" costs no per-instance allocation and a callable node's
/// spec can be guaranteed never-`None` for every `CallableTypeId` whose preset registered
/// a fallback.
///
/// `LibraryStack` itself implements [`SpecLookup`] with **stack-only** semantics
/// (fallbacks excluded): when a stack is nested inside another, its fallbacks must not
/// preempt the outer stack's real definitions — fallback policy belongs to the outermost
/// resolver alone.
pub struct LibraryStack<L: Lang> {
    /// Outermost first; lookups iterate in reverse.
    stack: Vec<Arc<dyn SpecLookup<L>>>,
    fallbacks: BTreeMap<CallableTypeId, Arc<dyn CallableSpec<L>>>,
}

impl<L: Lang> LibraryStack<L> {
    /// An empty stack: no definitions, no fallbacks.
    pub fn new() -> LibraryStack<L> {
        LibraryStack { stack: Vec::new(), fallbacks: BTreeMap::new() }
    }

    /// Push a lookup as the new innermost scope.
    pub fn push(&mut self, lookup: Arc<dyn SpecLookup<L>>) {
        self.stack.push(lookup);
    }

    /// Register the unknown-callable fallback spec for `callable_type` (typically a
    /// shared singleton). Returns the previously registered fallback, if any.
    pub fn set_fallback(
        &mut self,
        callable_type: CallableTypeId,
        spec: Arc<dyn CallableSpec<L>>,
    ) -> Option<Arc<dyn CallableSpec<L>>> {
        self.fallbacks.insert(callable_type, spec)
    }

    /// The registered fallback for `callable_type`, if any.
    pub fn fallback(&self, callable_type: CallableTypeId) -> Option<&Arc<dyn CallableSpec<L>>> {
        self.fallbacks.get(&callable_type)
    }

    /// Resolve `query` in the stack alone, innermost first. No fallback is consulted.
    pub fn lookup(
        &self,
        query: &CallableQuery<'_, '_, L>,
        state: &ParsingState<L>,
    ) -> Option<Arc<dyn CallableSpec<L>>> {
        self.stack.iter().rev().find_map(|l| l.lookup(query, state))
    }

    /// Resolve `query`: the stack innermost-first, then the fallback registered for the
    /// query's callable type. `None` only if the stack misses *and* no fallback is
    /// registered for that type.
    pub fn resolve(
        &self,
        query: &CallableQuery<'_, '_, L>,
        state: &ParsingState<L>,
    ) -> Option<Arc<dyn CallableSpec<L>>> {
        self.lookup(query, state).or_else(|| self.fallbacks.get(&query.callable_type).cloned())
    }

    /// Number of lookups on the stack.
    pub fn len(&self) -> usize {
        self.stack.len()
    }

    /// Whether the stack holds no lookups (fallbacks may still be registered).
    pub fn is_empty(&self) -> bool {
        self.stack.is_empty()
    }
}

impl<L: Lang> SpecLookup<L> for LibraryStack<L> {
    /// Stack-only semantics — see the type-level docs.
    fn lookup(
        &self,
        query: &CallableQuery<'_, '_, L>,
        state: &ParsingState<L>,
    ) -> Option<Arc<dyn CallableSpec<L>>> {
        LibraryStack::lookup(self, query, state)
    }
}

impl<L: Lang> Default for LibraryStack<L> {
    fn default() -> Self {
        LibraryStack::new()
    }
}

// Manual impls: derives would demand `L: Clone`/`L: Debug`; cloning is cheap (`Arc`s).

impl<L: Lang> Clone for LibraryStack<L> {
    fn clone(&self) -> Self {
        LibraryStack { stack: self.stack.clone(), fallbacks: self.fallbacks.clone() }
    }
}

impl<L: Lang> fmt::Debug for LibraryStack<L> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("LibraryStack")
            .field("stack", &self.stack)
            .field("fallbacks", &self.fallbacks)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::spec::StdCallableSpec;
    use crate::state::{ParsingStateDelta, StateData};
    use crate::token::TokenRules;
    use alloc::string::String;
    use alloc::vec;

    const MACRO: CallableTypeId = CallableTypeId::new(0);
    const ENVIRONMENT: CallableTypeId = CallableTypeId::new(1);

    #[derive(Debug, Clone, Copy)]
    struct PlainLang;
    impl Lang for PlainLang {
        type StateExt = ();
        type Event = ();
        type SourceOrigin = Option<String>;
    }

    fn min_rules() -> TokenRules {
        TokenRules {
            whitespace: None,
            double_newline_paragraphs: false,
            group_types: vec![],
            commands: vec![],
            comments: vec![],
            forbidden_chars: String::new(),
            expecting_group_close: None,
        }
    }

    fn state_with(libraries: LibraryStack<PlainLang>) -> ParsingState<PlainLang> {
        ParsingState::new(StateData { rules: min_rules(), libraries, ext: () })
    }

    fn new_spec() -> Arc<dyn CallableSpec<PlainLang>> {
        Arc::new(StdCallableSpec::default())
    }

    fn macro_query<'a>(name: &'a str) -> CallableQuery<'a, 'static, PlainLang> {
        CallableQuery::new(MACRO, name, CallableSyntax::Command { escape_char: '\\' })
    }

    #[test]
    fn library_insert_get_replace() {
        let mut lib: Library<PlainLang> = Library::new("test");
        assert!(lib.is_empty());

        let first = new_spec();
        let second = new_spec();
        assert!(lib.insert(MACRO, "foo", first.clone()).is_none());
        assert!(Arc::ptr_eq(lib.get(MACRO, "foo").unwrap(), &first));

        // Same key replaces (shadowing within one library = replacement) and returns
        // the previous spec; other callable types are separate key spaces.
        let previous = lib.insert(MACRO, "foo", second.clone()).unwrap();
        assert!(Arc::ptr_eq(&previous, &first));
        assert!(lib.get(ENVIRONMENT, "foo").is_none());
        assert_eq!(lib.len(), 1);
    }

    #[test]
    fn specs_are_flyweights_across_names() {
        let mut lib: Library<PlainLang> = Library::new("test");
        let shared = new_spec();
        lib.insert(MACRO, "emph", shared.clone());
        lib.insert(MACRO, "textit", shared.clone());

        let a = lib.get(MACRO, "emph").unwrap();
        let b = lib.get(MACRO, "textit").unwrap();
        assert!(Arc::ptr_eq(a, b));
    }

    #[test]
    fn stack_lookup_innermost_wins() {
        let outer_spec = new_spec();
        let inner_spec = new_spec();

        let mut outer: Library<PlainLang> = Library::new("outer");
        outer.insert(MACRO, "foo", outer_spec.clone());
        outer.insert(MACRO, "bar", outer_spec.clone());
        let mut inner: Library<PlainLang> = Library::new("inner");
        inner.insert(MACRO, "foo", inner_spec.clone());

        let mut stack = LibraryStack::new();
        stack.push(Arc::new(outer));
        stack.push(Arc::new(inner));
        let st = state_with(LibraryStack::new());

        // "foo" shadowed by the innermost; "bar" falls through to the outer library.
        let foo = stack.lookup(&macro_query("foo"), &st).unwrap();
        assert!(Arc::ptr_eq(&foo, &inner_spec));
        let bar = stack.lookup(&macro_query("bar"), &st).unwrap();
        assert!(Arc::ptr_eq(&bar, &outer_spec));
    }

    #[test]
    fn resolve_falls_back_per_callable_type() {
        let defined = new_spec();
        let unknown_macro = new_spec();

        let mut lib: Library<PlainLang> = Library::new("base");
        lib.insert(MACRO, "known", defined.clone());

        let mut stack = LibraryStack::new();
        stack.push(Arc::new(lib));
        stack.set_fallback(MACRO, unknown_macro.clone());
        let st = state_with(LibraryStack::new());

        // Stack hit: the fallback is not consulted.
        let known = stack.resolve(&macro_query("known"), &st).unwrap();
        assert!(Arc::ptr_eq(&known, &defined));

        // Stack miss: the per-type fallback answers — same singleton every time.
        let a = stack.resolve(&macro_query("nope"), &st).unwrap();
        let b = stack.resolve(&macro_query("also-nope"), &st).unwrap();
        assert!(Arc::ptr_eq(&a, &unknown_macro));
        assert!(Arc::ptr_eq(&a, &b));

        // No fallback registered for ENVIRONMENT; lookup() never consults fallbacks.
        let env_query =
            CallableQuery::new(ENVIRONMENT, "nope", CallableSyntax::Command { escape_char: '\\' });
        assert!(stack.resolve(&env_query, &st).is_none());
        assert!(stack.lookup(&macro_query("nope"), &st).is_none());
    }

    #[test]
    fn nested_stack_exposes_stack_only_semantics() {
        // A LibraryStack nested (as a SpecLookup) inside another stack must not let its
        // *fallbacks* preempt the outer stack's real definitions.
        let inner_fallback = new_spec();
        let outer_spec = new_spec();

        let mut inner: LibraryStack<PlainLang> = LibraryStack::new();
        inner.set_fallback(MACRO, inner_fallback);

        let mut outer_lib: Library<PlainLang> = Library::new("outer");
        outer_lib.insert(MACRO, "foo", outer_spec.clone());

        let mut stack = LibraryStack::new();
        stack.push(Arc::new(outer_lib));
        stack.push(Arc::new(inner)); // innermost, but empty stack-wise
        let st = state_with(LibraryStack::new());

        let foo = stack.lookup(&macro_query("foo"), &st).unwrap();
        assert!(Arc::ptr_eq(&foo, &outer_spec));
    }

    #[test]
    fn lookup_can_dispatch_on_syntax() {
        // A custom SpecLookup distinguishing coexisting command syntaxes: `\foo` and
        // `#foo` resolve to different specs (open question §6(a), decided July 2026).
        #[derive(Debug)]
        struct BySyntax {
            backslash: Arc<dyn CallableSpec<PlainLang>>,
            hash: Arc<dyn CallableSpec<PlainLang>>,
        }
        impl SpecLookup<PlainLang> for BySyntax {
            fn lookup(
                &self,
                query: &CallableQuery<'_, '_, PlainLang>,
                _state: &ParsingState<PlainLang>,
            ) -> Option<Arc<dyn CallableSpec<PlainLang>>> {
                match query.syntax {
                    CallableSyntax::Command { escape_char: '\\' } => {
                        Some(self.backslash.clone())
                    }
                    CallableSyntax::Command { escape_char: '#' } => Some(self.hash.clone()),
                    _ => None,
                }
            }
        }

        let by_syntax =
            BySyntax { backslash: new_spec(), hash: new_spec() };
        let backslash_spec = by_syntax.backslash.clone();
        let hash_spec = by_syntax.hash.clone();

        let mut stack = LibraryStack::new();
        stack.push(Arc::new(by_syntax));
        let st = state_with(LibraryStack::new());

        let via_backslash = stack.lookup(&macro_query("foo"), &st).unwrap();
        assert!(Arc::ptr_eq(&via_backslash, &backslash_spec));

        let hash_query =
            CallableQuery::new(MACRO, "foo", CallableSyntax::Command { escape_char: '#' });
        let via_hash = stack.lookup(&hash_query, &st).unwrap();
        assert!(Arc::ptr_eq(&via_hash, &hash_spec));
    }

    // --- mode-aware lookup: dispatch on state.ext() without privileged modes ----------

    #[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
    struct MathState {
        in_math: bool,
    }

    #[derive(Debug, Clone, Copy)]
    struct MathLang;
    impl Lang for MathLang {
        type StateExt = MathState;
        type Event = ();
        type SourceOrigin = Option<String>;
    }

    #[test]
    fn lookup_can_dispatch_on_state_ext() {
        #[derive(Debug)]
        struct ModeAware {
            text: Arc<dyn CallableSpec<MathLang>>,
            math: Arc<dyn CallableSpec<MathLang>>,
        }
        impl SpecLookup<MathLang> for ModeAware {
            fn lookup(
                &self,
                query: &CallableQuery<'_, '_, MathLang>,
                state: &ParsingState<MathLang>,
            ) -> Option<Arc<dyn CallableSpec<MathLang>>> {
                if query.name != "vec" {
                    return None;
                }
                Some(if state.ext().in_math { self.math.clone() } else { self.text.clone() })
            }
        }

        let text_spec: Arc<dyn CallableSpec<MathLang>> = Arc::new(StdCallableSpec::default());
        let math_spec: Arc<dyn CallableSpec<MathLang>> = Arc::new(StdCallableSpec::default());
        let mut libraries = LibraryStack::new();
        libraries.push(Arc::new(ModeAware { text: text_spec.clone(), math: math_spec.clone() }));

        let text_state: ParsingState<MathLang> = ParsingState::new(StateData {
            rules: min_rules(),
            libraries,
            ext: MathState::default(),
        });
        let math_state = text_state.derived(&ParsingStateDelta::new().ext(MathState { in_math: true }));

        let query: CallableQuery<'_, '_, MathLang> =
            CallableQuery::new(MACRO, "vec", CallableSyntax::Command { escape_char: '\\' });
        let in_text = text_state.libraries().lookup(&query, &text_state).unwrap();
        assert!(Arc::ptr_eq(&in_text, &text_spec));
        let in_math = math_state.libraries().lookup(&query, &math_state).unwrap();
        assert!(Arc::ptr_eq(&in_math, &math_spec));
    }

    #[test]
    fn push_library_via_delta_scopes_structurally() {
        // Mid-parse definition extension (`\newcommand`): a delta pushes a library; the
        // derived state resolves the new name, the base state never does — and "popping"
        // is just the caller keeping the previous Arc<ParsingState>.
        let new_def = new_spec();
        let mut lib: Library<PlainLang> = Library::new("newcommands");
        lib.insert(MACRO, "mycmd", new_def.clone());

        let base = state_with(LibraryStack::new());
        let derived = base.derived(&ParsingStateDelta::new().push_library(Arc::new(lib)));

        let query = macro_query("mycmd");
        assert!(base.libraries().lookup(&query, &base).is_none());
        let resolved = derived.libraries().lookup(&query, &derived).unwrap();
        assert!(Arc::ptr_eq(&resolved, &new_def));
        assert_eq!(base.libraries().len(), 0);
        assert_eq!(derived.libraries().len(), 1);
    }
}
