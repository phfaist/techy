//! [`CallableSpec`]: the behavior of anything invocable from the token stream; plus the
//! standard declarative implementation [`StdCallableSpec`].
//!
//! The invocation-form identifier is [`Lang::CallableTypeId`] — a closed per-language
//! type.

use alloc::boxed::Box;
use alloc::format;
use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::any::Any;
use core::fmt;

use crate::constructs::{ConstructParser, Invocation, StdInvocationParser};
use crate::node::BuildId;
use crate::state::Lang;

use super::structure::ArgumentSpec;

/// Which part of a callable's parse a live traceback [`Frame`](crate::engine::Frame)
/// covers — the `role` input of [`CallableSpec::stack_frame_title`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum FrameRole {
    /// The invocation itself (arguments and body included), dispatched from a content
    /// loop or an expression position.
    Invocation,
    /// One declared argument of the invocation.
    Argument {
        /// The argument's 0-based index in invocation order (rendered 1-based).
        index: usize,
    },
}

/// Behavior of anything invocable from the token stream. De-keyed: carries no name and no
/// invocation form; one spec may back several names (`\emph` and `\textit` can share), and
/// per-callable-type unknown-callable fallbacks can be shared singletons.
///
/// The declarative surface is the [`ArgumentSpec`] list (arguments *configure* an
/// invocation), pylatexenc-`arguments_spec_list`-shaped: `Arc`-shared so parsed nodes
/// can record which spec each argument was parsed against. Slots — a parsed callable's
/// *content regions* — have no spec-side declaration (slots
/// session): a body-bearing callable's takeover parser mints the
/// [`ParsedSlot`](crate::node::ParsedSlot) records directly, and announces that it
/// takes material via [`requires_content`](CallableSpec::requires_content). The default
/// method bodies describe the neutral callable — no arguments, no body — suitable for
/// simple specials (`~`) and fallback specs. The declarative standard implementation is
/// [`StdCallableSpec`].
///
/// The behavioral surface is [`make_invocation_parser`](CallableSpec::make_invocation_parser):
/// a factory returning a fresh boxed [`ConstructParser`] per resolved [`Invocation`],
/// defaulting to the declarative [`StdInvocationParser`]. Overriding it is the
/// full-takeover escape hatch for `\verb`-like constructs.
///
/// **Thread safety is part of the contract** (`Send + Sync` supertraits, decided July
/// 2026): specs are stored in parsed trees, so `NodeTree: Send + Sync` requires it.
/// Every method takes `&self`, so a stateful implementation needs interior mutability
/// regardless — under this contract that means locks or atomics (`Mutex`/`RwLock`/
/// `OnceLock`, or `spin` on `no_std`), not `RefCell`/`Cell`.
///
/// **Downcasting is part of the contract** (`Any` supertrait): a preset's
/// [`Lang::make_node_ext`](crate::state::Lang::make_node_ext)
/// mint recovers its concrete spec type from a stored `Arc<dyn CallableSpec<L>>` via
/// trait upcasting — `(&*spec as &dyn core::any::Any).downcast_ref::<MySpec>()`. The
/// `Arc`'d trait object was already implicitly `'static`; the supertrait makes it
/// per-implementor law. Downcasting to a preset's own spec *trait* (an open set of
/// third-party spec types) needs one extra move: register every spec behind one
/// concrete wrapper (`FlmSpecBox(Arc<dyn FlmSpec>)` delegating to the inner value) and
/// downcast to the wrapper.
pub trait CallableSpec<L: Lang>: fmt::Debug + Send + Sync + Any {
    /// The declarative argument structure of an invocation, in invocation order.
    /// Default: no arguments.
    fn arguments(&self) -> &[Arc<ArgumentSpec<L>>] {
        &[]
    }

    /// Would this invocation, appearing **bare** — as a single-token expression
    /// argument, `\frac\mymacro 2` — be malformed? The expression position's guard
    /// consults this before dispatching (the spec-side
    /// face of pylatexenc's `contents_can_be_empty` consultation): `true` diagnoses the
    /// bare use and stages the single-token callable with every declared argument
    /// absent; `false` dispatches the invocation in full.
    ///
    /// The default derives from the declarative surface: content is required exactly
    /// when some declared argument cannot match empty
    /// ([`ArgumentParser::can_match_empty`](super::ArgumentParser::can_match_empty)).
    /// With no spec-side slot list, this method is the **only** channel for a
    /// body-bearing takeover spec to say "I take material": a spec that declares
    /// nothing but consumes plenty (a `\begin` dispatcher, `\verb`) must override this
    /// to `true`.
    fn requires_content(&self) -> bool {
        self.arguments().iter().any(|argument| !argument.parser.can_match_empty())
    }

    /// The factory producing this spec's invocation parser: a **fresh boxed parser per
    /// resolved invocation**, ownership moved to the caller, the [`Invocation`]
    /// traveling inside the parser instance (pylatexenc's `get_node_parser(token)` shape
    /// with ownership made explicit). The dispatch loop resolves the trigger, builds the `Invocation`,
    /// calls this factory, runs `parser.parse(cx)` once, and drops the parser.
    ///
    /// When the parser runs, the trigger token has already been consumed whole by the
    /// dispatching arm; the parser's `cx.state` is the invocation's base state (see
    /// [`StdInvocationParser`]'s module docs for the full contract, including the
    /// post-space rule).
    ///
    /// The default returns the declarative [`StdInvocationParser`]. **Overriding this
    /// factory is the full-takeover escape hatch**: a custom parser reads tokens
    /// however it wants (`\verb` raw content, tabular preambles), stages its own node
    /// shape, and may return a state delta as the invocation's after-effect for
    /// subsequent siblings (`\newcommand`).
    fn make_invocation_parser<'a, 's>(
        &'a self,
        invocation: Invocation<'a, 's, L>,
    ) -> Box<dyn ConstructParser<L, Output = BuildId> + 'a>
    where
        's: 'a,
    {
        Box::new(StdInvocationParser::new(invocation))
    }

    /// Title of a parse-traceback frame covering this callable: called at *snapshot* time — the cold path, when a condition is recorded —
    /// never on push, so live frames stay allocation-free. `name` is the invocation
    /// spelling as written (`\frac`, `~`), sliced from the source at snapshot time; the
    /// spec itself is de-keyed and cannot know it.
    ///
    /// The default renders `callable ‘\frac’` / `argument #1 of ‘\frac’` — the core has
    /// no construct taxonomy; a preset overrides this to speak its own vocabulary
    /// ("macro ‘\frac’", "environment ‘align’").
    fn stack_frame_title(&self, role: FrameRole, name: &str) -> String {
        match role {
            FrameRole::Invocation => format!("callable ‘{}’", name),
            FrameRole::Argument { index } => {
                format!("argument #{} of ‘{}’", index + 1, name)
            }
        }
    }
}

mod sealed {
    use super::{CallableSpec, Lang};
    use alloc::sync::Arc;

    // Inference markers: they let the by-value blanket coexist with the Arc
    // pass-through impls (trait coherence would otherwise reject the pair on a
    // Lang-generic trait). Callers never name them — the marker parameter is
    // inferred; each argument shape matches exactly one impl.
    pub struct ByValue;
    pub struct SharedConcrete;
    pub struct SharedDyn;

    pub trait SealedSpec<L: Lang, M> {}
    impl<L: Lang, S: CallableSpec<L>> SealedSpec<L, ByValue> for S {}
    impl<L: Lang, S: CallableSpec<L>> SealedSpec<L, SharedConcrete> for Arc<S> {}
    impl<L: Lang> SealedSpec<L, SharedDyn> for Arc<dyn CallableSpec<L>> {}
}

/// Sealed conversion into a shared [`CallableSpec`] — the spec argument contract of
/// [`Package::insert`](crate::scopes::Package::insert) and its siblings, following
/// the crate's one Arc-removal conversion idiom (the provider-side sibling is
/// [`IntoSpecsProvider`](crate::scopes::IntoSpecsProvider)): a spec passes **by
/// value** (`insert(CallableType::Macro, "emph", MacroSpec::new(…))`, no `Arc::new`
/// noise), while an already-shared **`Arc<S>`** or **`Arc<dyn CallableSpec<L>>`**
/// passes through as-is — no double-wrap, so pre-shared flyweight specs (one spec
/// backing several names) keep their sharing.
///
/// Sealed: the three impls are the whole vocabulary; downstream code implements
/// [`CallableSpec`], never this trait. (The `M` parameter is a sealed inference
/// marker distinguishing the three argument shapes — it never needs to be named.)
pub trait IntoCallableSpec<L: Lang, M>: sealed::SealedSpec<L, M> {
    /// Convert into the shared spec handle providers store.
    fn into_callable_spec(self) -> Arc<dyn CallableSpec<L>>;
}

impl<L: Lang, S: CallableSpec<L>> IntoCallableSpec<L, sealed::ByValue> for S {
    fn into_callable_spec(self) -> Arc<dyn CallableSpec<L>> {
        Arc::new(self)
    }
}

impl<L: Lang, S: CallableSpec<L>> IntoCallableSpec<L, sealed::SharedConcrete> for Arc<S> {
    fn into_callable_spec(self) -> Arc<dyn CallableSpec<L>> {
        self
    }
}

impl<L: Lang> IntoCallableSpec<L, sealed::SharedDyn> for Arc<dyn CallableSpec<L>> {
    fn into_callable_spec(self) -> Arc<dyn CallableSpec<L>> {
        self
    }
}

/// The standard declarative [`CallableSpec`]: the argument structure as plain data.
pub struct StdCallableSpec<L: Lang> {
    /// The argument structure.
    pub arguments: Vec<Arc<ArgumentSpec<L>>>,
}

impl<L: Lang> StdCallableSpec<L> {
    /// A spec with the given argument structure.
    pub fn new(arguments: Vec<Arc<ArgumentSpec<L>>>) -> StdCallableSpec<L> {
        StdCallableSpec { arguments }
    }
}

impl<L: Lang> CallableSpec<L> for StdCallableSpec<L> {
    fn arguments(&self) -> &[Arc<ArgumentSpec<L>>] {
        &self.arguments
    }
}

// Manual impls: derives would demand `L: Clone`/`L: Debug`/`L: Default` although only
// `Arc`s to spec data are stored.

impl<L: Lang> Default for StdCallableSpec<L> {
    fn default() -> Self {
        StdCallableSpec { arguments: Vec::new() }
    }
}

impl<L: Lang> Clone for StdCallableSpec<L> {
    fn clone(&self) -> Self {
        StdCallableSpec { arguments: self.arguments.clone() }
    }
}

impl<L: Lang> fmt::Debug for StdCallableSpec<L> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("StdCallableSpec")
            .field("arguments", &self.arguments)
            .finish()
    }
}
