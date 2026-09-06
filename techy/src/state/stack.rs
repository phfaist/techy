//! [`ParsingStateStack`]: the stack of parsing states enclosing one position — the
//! record a running parse keeps, and the same thing rebuilt from a parsed tree
//! afterwards.

use alloc::sync::Arc;
use alloc::vec::Vec;
use core::fmt;

use crate::node::NodeRef;

use super::lang::Lang;
use super::parsing_state::ParsingState;

/// The [`ParsingState`]s enclosing one position, **innermost first**: the current
/// state, then each enclosing scope's state outward to the outermost (seed) state.
///
/// A stack comes from one of two places, and both are used the same way:
///
/// - **While a parse runs.** The
///   [`ParserSession`](crate::core::ParserSession) keeps one, pushing and popping it
///   at the same points as the traceback frames — every descent under a scoped state,
///   [`ParseContext::with_parsing_state`](crate::core::constructs::ParseContext::with_parsing_state)
///   — and passes it by reference to the driver hook that translates state events,
///   [`ParseDriver::resolve_state_event`](crate::core::ParseDriver::resolve_state_event).
///   The engine already holds exactly these states anyway, since leaving a scope means
///   continuing with the outer `Arc`; the stack only lists them. It is **dropped with
///   the session**, so no record of the ancestry survives into the parsed tree.
/// - **After a parse.** [`from_states`](ParsingStateStack::from_states) builds one from
///   states you supply, and
///   [`from_node_ancestors`](ParsingStateStack::from_node_ancestors) recovers one from
///   a parsed node. That lets later processing — a transform building new nodes — call
///   the same language-generic helpers the driver hook calls, such as
///   [`exit_math_context_delta`](crate::latexlike::exit_math_context_delta), with no
///   session anywhere.
///
/// # What the stack promises
///
/// Code that uses a stack searches it from the innermost entry outward for the first
/// state satisfying some condition, falling back to the outermost entry. *That* is what
/// the type guarantees, and it is deliberately not an entry-for-entry reproduction of
/// a parse's descent history: an ancestor walk repeats a state whenever consecutive
/// ancestors share one, and includes entries for ancestors that are not groups. Neither
/// can change what such a search finds.
pub struct ParsingStateStack<L: Lang> {
    /// Stored outermost-first (push/pop at the `Vec` tail = the innermost end);
    /// the public iteration order is innermost-first.
    states: Vec<Arc<ParsingState<L>>>,
}

impl<L: Lang> ParsingStateStack<L> {
    /// The empty stack: no enclosing context at all.
    ///
    /// A search over it finds nothing and has no fallback entry, so the answer is
    /// whatever the consumer documents as its do-nothing result — for instance,
    /// [`exit_math_context_delta`](crate::latexlike::exit_math_context_delta) returns
    /// an empty delta.
    pub fn new() -> ParsingStateStack<L> {
        ParsingStateStack { states: Vec::new() }
    }

    /// A stack built from the given states, listed **innermost first** — the same
    /// order [`iter()`](ParsingStateStack::iter) returns them in. `states[0]` is the
    /// current, innermost state and the last element is the outermost.
    pub fn from_states(states: Vec<Arc<ParsingState<L>>>) -> ParsingStateStack<L> {
        let mut states = states;
        states.reverse();
        ParsingStateStack { states }
    }

    /// The states enclosing `node`'s position, recovered from the parsed tree: the
    /// node's own recorded parse-time state first, then each parent's outward,
    /// innermost first — the same order a stack from a running parse uses.
    ///
    /// What this walk reproduces is the search behavior described on the type, not the
    /// session's stack entry for entry: consecutive ancestors often share one state, and
    /// ancestors that are not groups contribute entries of their own. A search from the
    /// innermost entry outward, with the outermost as fallback, cannot tell the
    /// difference.
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

    /// The states, **innermost first**: the current state, then each enclosing state
    /// outward, ending with the outermost (seed) state.
    pub fn iter(&self) -> impl Iterator<Item = &Arc<ParsingState<L>>> {
        self.states.iter().rev()
    }

    /// The outermost entry, the one a search falls back to. `None` only for an empty
    /// stack.
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

    /// Push `state` as the new innermost entry; what the session does on descent.
    pub(crate) fn push(&mut self, state: Arc<ParsingState<L>>) {
        self.states.push(state);
    }

    /// Pop the innermost entry; what the session does when leaving a scope.
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
        let outer: Arc<ParsingState<PlainLang>> =
            Arc::new(ParsingState::lang_initial().expect("seed state"));
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
            ParsingState::lang_initial().expect("seed state"),
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
        let a: Arc<ParsingState<PlainLang>> =
            Arc::new(ParsingState::lang_initial().expect("seed state"));
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
