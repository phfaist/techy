//! [`ParsingStateStack`]: the enclosing-state stack — the session's live record of
//! the states a parse descended through, and the constructible post-parse
//! equivalent.

use alloc::sync::Arc;
use alloc::vec::Vec;
use core::fmt;

use crate::node::NodeRef;

use super::lang::Lang;
use super::parsing_state::ParsingState;

/// A stack of enclosing [`ParsingState`]s, **innermost-first**: the context a
/// parse position sits in — the current state first, then each enclosing scope's
/// state outward to the outermost (seed) state.
///
/// Two producers feed the same consumer signatures:
///
/// - **The live session stack.** During a parse, the
///   [`ParserSession`](crate::engine::ParserSession) maintains its enclosing-state
///   stack as one of these — pushed/popped at the same descent points as the
///   traceback frame stack (every scoped-state descent,
///   [`ParseContext::with_parsing_state`](crate::constructs::ParseContext::with_parsing_state))
///   — and lends it by reference to the driver's event-lowering hook
///   ([`ParseDriver::resolve_state_event`](crate::engine::ParseDriver::resolve_state_event)).
///   The engine retains exactly these states implicitly anyway (leaving a scope
///   structurally restores the outer `Arc`); the stack only materializes them, and
///   it is **dropped with the session** — no ancestry data survives into parsed
///   material.
/// - **Post-parse construction.** [`from_states`](ParsingStateStack::from_states)
///   assembles one from explicit states, and
///   [`from_node_ancestors`](ParsingStateStack::from_node_ancestors) recovers one
///   from a parsed node — so post-parse processing (transforms synthesizing nodes)
///   can feed the same `LLL`-generic behavior functions the driver hook feeds, e.g.
///   [`exit_math_context_delta`](crate::latexlike::exit_math_context_delta), with
///   no session anywhere.
///
/// # Scan semantics, not stack identity
///
/// Consumers of this type scan it (innermost-first) for the first state matching a
/// predicate, with the outermost entry as the fallback. That **scan contract** is
/// the type's interface — deliberately *not* an entry-for-entry reproduction of
/// any particular parse's descent history: a node-ancestor walk contains
/// `Arc`-equal duplicates (ancestors that share one state) and entries for
/// non-group ancestors, none of which can change what a scan finds.
pub struct ParsingStateStack<L: Lang> {
    /// Stored outermost-first (push/pop at the `Vec` tail = the innermost end);
    /// the public iteration order is innermost-first.
    states: Vec<Arc<ParsingState<L>>>,
}

impl<L: Lang> ParsingStateStack<L> {
    /// The empty stack (no enclosing context at all). Scans over it find nothing
    /// and have no fallback entry; the documented consumer behavior is a no-op
    /// answer (e.g. [`exit_math_context_delta`](crate::latexlike::exit_math_context_delta)
    /// returns the empty delta).
    pub fn new() -> ParsingStateStack<L> {
        ParsingStateStack { states: Vec::new() }
    }

    /// A stack from explicit states, given **innermost-first** (the same order
    /// [`iter`](ParsingStateStack::iter) yields): `states[0]` is the current
    /// (innermost) state, the last element the outermost.
    pub fn from_states(states: Vec<Arc<ParsingState<L>>>) -> ParsingStateStack<L> {
        let mut states = states;
        states.reverse();
        ParsingStateStack { states }
    }

    /// The enclosing-state stack at `node`'s position, recovered from the parsed
    /// tree: the node's own recorded parse-time state first, then each parent's
    /// outward via the stored parent table — innermost-first, current state first,
    /// exactly the live stack's convention.
    ///
    /// The contract is **scan semantics** (see the type docs), not stack identity:
    /// the ancestor walk is not entry-for-entry the parse-time session stack —
    /// consecutive ancestors often share one state (`Arc`-equal duplicates) and
    /// non-group ancestors contribute entries too — but a first-match scan with an
    /// outermost fallback cannot tell the difference.
    pub fn from_node_ancestors<A>(node: NodeRef<'_, L, A>) -> ParsingStateStack<L> {
        let mut states = Vec::new();
        states.push(Arc::clone(node.parsing_state()));
        let mut current = node.parent();
        while let Some(ancestor) = current {
            states.push(Arc::clone(ancestor.parsing_state()));
            current = ancestor.parent();
        }
        ParsingStateStack::from_states(states)
    }

    /// The states, **innermost-first**: the current state, then each enclosing
    /// state outward; the last item is the outermost (seed) state.
    pub fn iter(&self) -> impl Iterator<Item = &Arc<ParsingState<L>>> {
        self.states.iter().rev()
    }

    /// The outermost (seed-side) entry — the scan fallback. `None` only on the
    /// empty stack.
    pub fn outermost(&self) -> Option<&Arc<ParsingState<L>>> {
        self.states.first()
    }

    /// Number of entries.
    pub fn len(&self) -> usize {
        self.states.len()
    }

    /// Whether the stack has no entries.
    pub fn is_empty(&self) -> bool {
        self.states.is_empty()
    }

    /// Push `state` as the new innermost entry (the session's descent op).
    pub(crate) fn push(&mut self, state: Arc<ParsingState<L>>) {
        self.states.push(state);
    }

    /// Pop the innermost entry (the session's scope-exit op).
    pub(crate) fn pop(&mut self) -> Option<Arc<ParsingState<L>>> {
        self.states.pop()
    }

    /// The innermost entry, if any.
    pub(crate) fn innermost(&self) -> Option<&Arc<ParsingState<L>>> {
        self.states.last()
    }
}

impl<L: Lang> Default for ParsingStateStack<L> {
    fn default() -> Self {
        ParsingStateStack::new()
    }
}

// Manual impls: derives would demand `L: Clone`/`L: Debug` although only `Arc`s of
// associated-type-bounded data are stored.

impl<L: Lang> Clone for ParsingStateStack<L> {
    fn clone(&self) -> Self {
        ParsingStateStack { states: self.states.clone() }
    }
}

impl<L: Lang> fmt::Debug for ParsingStateStack<L> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ParsingStateStack").field("len", &self.states.len()).finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::{ParsingStateDelta, TrivialLang};

    #[derive(Debug, Clone, Copy)]
    struct PlainLang;
    impl TrivialLang for PlainLang {}

    #[test]
    fn from_states_iterates_innermost_first() {
        let outer: Arc<ParsingState<PlainLang>> = Arc::new(ParsingState::lang_initial());
        let inner = Arc::new(outer.derived(&ParsingStateDelta::new()).unwrap());

        let stack =
            ParsingStateStack::from_states(alloc::vec![Arc::clone(&inner), Arc::clone(&outer)]);
        assert_eq!(stack.len(), 2);
        assert!(!stack.is_empty());
        let order: alloc::vec::Vec<bool> =
            stack.iter().map(|state| Arc::ptr_eq(state, &inner)).collect();
        assert_eq!(order, [true, false]);
        assert!(Arc::ptr_eq(stack.outermost().unwrap(), &outer));

        let empty: ParsingStateStack<PlainLang> = ParsingStateStack::new();
        assert!(empty.is_empty());
        assert!(empty.outermost().is_none());
        assert_eq!(empty.iter().count(), 0);
    }

    #[test]
    fn from_node_ancestors_walks_innermost_first() {
        use crate::engine::Language;
        use crate::error::Recovery;
        use crate::latexlike::{Latexlike, LatexlikeDriver, Mode};

        let language: Language<Latexlike> = Language::new(
            LatexlikeDriver::new(Recovery::Strict),
            ParsingState::lang_initial(),
        );
        let result = language.parse("{a $x y$ b}").unwrap();
        let brace = result.tree.root().child(0).unwrap();
        let math = brace.child(1).unwrap();
        let interior_chars = math.child(0).unwrap();

        let stack = ParsingStateStack::from_node_ancestors(interior_chars);
        // The node's own recorded (math) state first, then outward — the walk
        // contract is scan semantics: Arc-equal duplicates (ancestors sharing a
        // state) are expected and harmless.
        let modes: alloc::vec::Vec<Mode> =
            stack.iter().map(|state| state.mode()).collect();
        assert_eq!(modes[0], Mode::Math);
        assert_eq!(*modes.last().unwrap(), Mode::Text);
        // The first entry is exactly the node's recorded state…
        assert!(Arc::ptr_eq(
            stack.iter().next().unwrap(),
            interior_chars.parsing_state()
        ));
        // …and the outermost is the root's.
        assert!(Arc::ptr_eq(
            stack.outermost().unwrap(),
            result.tree.root().parsing_state()
        ));
        // A scan for the first non-math state finds the brace interior's text
        // context, not the seed.
        let first_non_math = stack.iter().find(|state| state.mode() == Mode::Text).unwrap();
        assert!(Arc::ptr_eq(first_non_math, math.parsing_state()));
    }

    #[test]
    fn push_pop_track_the_innermost_entry() {
        let a: Arc<ParsingState<PlainLang>> = Arc::new(ParsingState::lang_initial());
        let b = Arc::new(a.derived(&ParsingStateDelta::new()).unwrap());

        let mut stack: ParsingStateStack<PlainLang> = ParsingStateStack::new();
        stack.push(Arc::clone(&a));
        stack.push(Arc::clone(&b));
        assert!(Arc::ptr_eq(stack.innermost().unwrap(), &b));
        assert!(Arc::ptr_eq(stack.iter().next().unwrap(), &b));
        assert!(Arc::ptr_eq(stack.outermost().unwrap(), &a));
        let popped = stack.pop().unwrap();
        assert!(Arc::ptr_eq(&popped, &b));
        assert!(Arc::ptr_eq(stack.innermost().unwrap(), &a));
    }
}
