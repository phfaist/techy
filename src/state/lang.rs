//! The [`Lang`] trait: the compile-time customization bundle; with it, the
//! [`NodeExtTypes`] node-ext bundle and the [`SimpleLang`] all-defaults convenience.
//!
//! `NodeExtTypes` is defined here, next to `Lang`, rather than in the `node` topic:
//! its *meaning* is a node concern, but it is a constituent of the compile-time bundle,
//! and moving it there would recreate a module cycle for cosmetics (ARCHITECTURE.md
//! §engine stratum note).

use alloc::string::String;
use core::fmt;

use crate::source::SourceOrigin;
use crate::token::{SpecialsMatch, TokenResult, TriggerChars};

use super::parsing_state::{ParsingState, StateData};

/// The bundle of node extension types of a language: the **two-tier ext system** of
/// ARCHITECTURE.md §nodes, orthogonal to structural node identity (a group with custom
/// data is still a group to all generic tooling).
///
/// Tier 1 — [`NodeExt`](NodeExtTypes::NodeExt) — sits uniformly on every node
/// (cross-cutting per-instance concerns: bindings handles, IDs, …). Tier 2 — the
/// per-kind `<Kind>NodeExt` types — carries kind-specific per-instance parse results.
///
/// Bundled behind one associated type (`Lang::NodeExts`) to keep [`Lang`] small; `()`
/// implements the bundle with every type `()`. Keep ext types word-sized where possible
/// (an index or `Arc` into Lang-owned storage) — `NodeKind` stores tier-2 exts inline.
///
/// The `Default` bound supplies the value builders use when a node carries no meaningful
/// ext (consistent with `Lang::StateExt: Default`).
pub trait NodeExtTypes {
    /// Tier 1: the uniform ext on every node.
    type NodeExt: Clone + fmt::Debug + Default;
    /// Tier 2: ext of `Chars` nodes.
    type CharsNodeExt: Clone + fmt::Debug + Default;
    /// Tier 2: ext of `Group` nodes.
    type GroupNodeExt: Clone + fmt::Debug + Default;
    /// Tier 2: ext of `Callable` nodes.
    type CallableNodeExt: Clone + fmt::Debug + Default;
    /// Tier 2: ext of `Comment` nodes.
    type CommentNodeExt: Clone + fmt::Debug + Default;
    /// Tier 2: ext of `List` nodes.
    type ListNodeExt: Clone + fmt::Debug + Default;
}

/// The no-ext bundle: every ext type is `()`.
impl NodeExtTypes for () {
    type NodeExt = ();
    type CharsNodeExt = ();
    type GroupNodeExt = ();
    type CallableNodeExt = ();
    type CommentNodeExt = ();
    type ListNodeExt = ();
}

/// The compile-time type bundle of a language definition. Every core type takes one
/// `L: Lang` parameter — never five (ARCHITECTURE.md §2 principle 2).
///
/// A minimal language is a ZST with only the associated types filled in; all methods have
/// working defaults (no transition customization, no specials). The latexlike preset
/// (Phase 7) and FLM are the intended full implementors.
pub trait Lang: Sized {
    /// Language-specific parsing state (e.g. a math-mode flag). Typed — no `Any` maps;
    /// `()` for languages without extra state.
    type StateExt: Clone + fmt::Debug + Default;

    /// Semantic transition events (e.g. an `EnterMath`), consumed by
    /// [`finalize_transition`](Lang::finalize_transition). `()` if unused.
    type Event: Clone + fmt::Debug;

    /// Origin metadata type for sources (plugged into `Source<O>`); conventionally
    /// `Option<String>`.
    type SourceOrigin: SourceOrigin;

    /// The node extension type bundle ([`NodeExtTypes`]); `()` for languages without
    /// custom node data.
    type NodeExts: NodeExtTypes;

    /// Transition customizer — the choke-point hook, run exactly once per
    /// [`derived()`](ParsingState::derived) transition, after the delta's overrides have
    /// been applied and before the new state is frozen. Cross-cutting rules centralize
    /// here (e.g. FLM's "in math mode the escape char is `#`"); the override policy —
    /// pure normalization vs. event-driven — is this function's business
    /// (ARCHITECTURE.md §state). The default does nothing.
    fn finalize_transition(
        new: &mut StateData<Self>,
        prev: &ParsingState<Self>,
        events: &[Self::Event],
    ) {
        let _ = (new, prev, events);
    }

    /// Specials scan: is a callable-triggering character sequence at `content[pos..]`?
    ///
    /// Recognition and resolution happen in one call — a [`SpecialsMatch`] carries both
    /// the name and the resolved spec (unknown-name fallback policy included), which
    /// makes scanning/lookup mismatches impossible by construction. Typically implemented
    /// by a preset dispatching to the state's libraries (Phase 4+). Positions are
    /// absolute byte offsets into `content`. May fail recoverably (the error participates
    /// in the recovery-token protocol like any tokenization step).
    ///
    /// Only consulted when the current character is in
    /// [`specials_trigger_chars`](Lang::specials_trigger_chars) (cached per state).
    /// The default recognizes nothing.
    fn scan_specials<'s>(
        state: &ParsingState<Self>,
        content: &'s str,
        pos: usize,
    ) -> TokenResult<'s, Self, Option<SpecialsMatch<'s, Self>>> {
        let _ = (state, content, pos);
        Ok(None)
    }

    /// The characters that may start a specials trigger under `data` — the hot-path
    /// filter for [`scan_specials`](Lang::scan_specials). Computed when a state is
    /// frozen and cached on the state instance (rebuilt only at transitions, like the
    /// `PrefixTable`); receives [`StateData`] rather than [`ParsingState`] because it
    /// runs *while* the state is being built. Return [`TriggerChars::Any`] for fully
    /// dynamic scanners. The default: no specials.
    fn specials_trigger_chars(data: &StateData<Self>) -> TriggerChars {
        let _ = data;
        TriggerChars::default()
    }
}

/// All-defaults language marker: `impl SimpleLang for MyLang {}` yields a [`Lang`] with
/// every associated type defaulted (`StateExt`/`Event`/`NodeExts` = `()`, `SourceOrigin` =
/// `Option<String>`) and the default method behavior — the workaround for associated-type
/// defaults being unstable.
///
/// A language needing *any* customization implements [`Lang`] directly instead (the
/// blanket impl makes the two mutually exclusive).
pub trait SimpleLang: Sized {}

impl<T: SimpleLang> Lang for T {
    type StateExt = ();
    type Event = ();
    type SourceOrigin = Option<String>;
    type NodeExts = ();
}
