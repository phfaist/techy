//! [`ChildStateSpec`]: per-use descent-state policy on [`NodesParser`] (ports pylatexenc's `make_child_parsing_state`).
//!
//! When the dispatch loop descends into a child construct — a group interior, or (6.4) a
//! callable invocation — the child construct parser's **base state** is resolved through
//! this policy instead of being unconditionally the loop's own state. Motivating use
//! case: a macro argument parsed "chars except groups" — a delta-restricted state
//! (commands/comments cleared, groups kept) whose group *interiors* revert to the outer,
//! unrestricted state: `group: Fixed(outer)`.
//!
//! (The 6.5 consumer,
//! [`OptionalGroupArgumentParser`](super::OptionalGroupArgumentParser)'s
//! bracket-balancing policy, since detached: its keep-or-revert semantics are
//! per-level by design — decided semantics 3 below — and are now carried by the
//! state-scoped temporary-group-rules
//! ([`GroupRules::temporary`](crate::token::GroupRules::temporary)) lifecycle,
//! which reaches every depth. The mechanism here remains for descent policies that are
//! genuinely per-use and per-level, like the chars-except-groups motivating case.)
//!
//! Decided semantics:
//!
//! 1. *Resolution precedes policy* — `ParseDriver::resolve_command` runs under the loop's own
//!    state (coherent with the state that tokenized the token); the policy only shapes
//!    what happens after.
//! 2. *The policy provides the base; spec deltas stack on top* — the group parser's
//!    expecting-close derivation, and later argument/slot `parsing_state_delta`s, derive
//!    from the policy's answer.
//! 3. *One level deep* — the `NodesParser`s recursed into by group/invocation parsers
//!    default to [`Inherit`](GroupChildState::Inherit); group-delimited *arguments* of an
//!    invocation are reached by the `invocation` policy (they are parsed inside the
//!    invocation parser), not by `group`.
//! 4. *States pass as-is* — `Arc` in, `Arc` out: `Inherit`/`Fixed`/pass-through `Compute`
//!    never force a `derived()`, and returning the same `Arc` preserves pointer identity
//!    (which keeps identity-keyed memoization sound).
//! 5. *Callbacks are pure `&dyn Fn`* — a descent policy whose answer depends on call
//!    order would be fragile; context-dependent policies precompute their candidate
//!    states and select among them.

use alloc::sync::Arc;
use core::fmt;

use crate::state::{Lang, ParsingState};
use crate::token::Token;

use super::Invocation;

/// The base state for a group interior descent — resolved by the `GroupOpen` arm of
/// [`NodesParser`](super::NodesParser) before the group parser runs (the group parser
/// then derives its `expecting_group_close` interior state *from* this base).
pub enum GroupChildState<'p, L: Lang> {
    /// Use the loop's current state (the default — plain nesting).
    Inherit,
    /// Use this state (the chars-except-groups revert-to-outer case).
    Fixed(Arc<ParsingState<L>>),
    /// Compute the base from the loop's current state and the opening token (which
    /// carries `delim` and the resolved `Arc<GroupRule>` — so the policy can key on the
    /// group's class). Pure: return one of the inputs or a precomputed state; the
    /// callback is not a place to run derivations.
    #[allow(clippy::type_complexity)]
    Compute(&'p dyn Fn(&Arc<ParsingState<L>>, &Token<'_, L>) -> Arc<ParsingState<L>>),
}

/// The base state for a callable invocation descent (`Command`/`Specials` arms).
///
/// Consulted by the invocation arms, which dispatch through
/// `make_invocation_parser`; the compute context is the
/// resolved [`Invocation`] — resolution precedes policy.
pub enum InvocationChildState<'p, L: Lang> {
    /// Use the loop's current state (the default).
    Inherit,
    /// Use this state.
    Fixed(Arc<ParsingState<L>>),
    /// Compute the base from the loop's current state and the resolved invocation
    /// (callable type, name, spec, trigger token). Pure, like
    /// [`GroupChildState::Compute`].
    #[allow(clippy::type_complexity)]
    Compute(&'p dyn Fn(&Arc<ParsingState<L>>, &Invocation<'_, '_, L>) -> Arc<ParsingState<L>>),
}

/// Per-use descent-state configuration of a [`NodesParser`](super::NodesParser) — one
/// policy per descent pathway. Same tier-2 borrowed-config role as
/// [`StopSpec`](super::StopSpec); the default (`Inherit`/`Inherit`) is plain nesting.
pub struct ChildStateSpec<'p, L: Lang> {
    /// Base-state policy for group interiors (`GroupOpen` arm).
    pub group: GroupChildState<'p, L>,
    /// Base-state policy for callable invocations (`Command`/`Specials` arms).
    pub invocation: InvocationChildState<'p, L>,
}

impl<'p, L: Lang> ChildStateSpec<'p, L> {
    /// Plain nesting: both pathways inherit the loop's state.
    pub fn inherit() -> ChildStateSpec<'p, L> {
        ChildStateSpec {
            group: GroupChildState::Inherit,
            invocation: InvocationChildState::Inherit,
        }
    }
}

impl<L: Lang> Default for ChildStateSpec<'_, L> {
    fn default() -> Self {
        ChildStateSpec::inherit()
    }
}

// Manual impls: callbacks have no useful Debug, and derives would demand `L:` bounds.
// Clone is shallow — `Fixed` states are `Arc`s and `Compute` callbacks are borrows.

impl<L: Lang> Clone for GroupChildState<'_, L> {
    fn clone(&self) -> Self {
        match self {
            GroupChildState::Inherit => GroupChildState::Inherit,
            GroupChildState::Fixed(state) => GroupChildState::Fixed(Arc::clone(state)),
            GroupChildState::Compute(compute) => GroupChildState::Compute(*compute),
        }
    }
}

impl<L: Lang> Clone for InvocationChildState<'_, L> {
    fn clone(&self) -> Self {
        match self {
            InvocationChildState::Inherit => InvocationChildState::Inherit,
            InvocationChildState::Fixed(state) => InvocationChildState::Fixed(Arc::clone(state)),
            InvocationChildState::Compute(compute) => InvocationChildState::Compute(*compute),
        }
    }
}

impl<L: Lang> Clone for ChildStateSpec<'_, L> {
    fn clone(&self) -> Self {
        ChildStateSpec { group: self.group.clone(), invocation: self.invocation.clone() }
    }
}

impl<L: Lang> fmt::Debug for GroupChildState<'_, L> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            GroupChildState::Inherit => write!(f, "Inherit"),
            GroupChildState::Fixed(_) => write!(f, "Fixed(..)"),
            GroupChildState::Compute(_) => write!(f, "Compute(..)"),
        }
    }
}

impl<L: Lang> fmt::Debug for InvocationChildState<'_, L> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            InvocationChildState::Inherit => write!(f, "Inherit"),
            InvocationChildState::Fixed(_) => write!(f, "Fixed(..)"),
            InvocationChildState::Compute(_) => write!(f, "Compute(..)"),
        }
    }
}

impl<L: Lang> fmt::Debug for ChildStateSpec<'_, L> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ChildStateSpec")
            .field("group", &self.group)
            .field("invocation", &self.invocation)
            .finish()
    }
}
