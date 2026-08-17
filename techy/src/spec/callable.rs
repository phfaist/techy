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

use crate::constructs::{ConstructParser, FromInvocation, Invocation, StdInvocationParser};
use crate::node::BuildId;
use crate::serialize::{
    DeserializeContext, DeserializeError, SerialValue, SerializableLang, SerializableObject,
    SerializeContext, SerializeError,
};
use crate::scopes::SpecProvenance;
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
/// *content regions* — have no spec-side declaration: a
/// body-bearing callable's takeover parser mints the
/// [`ParsedSlot`](crate::node::ParsedSlot) records directly, and announces that it
/// takes material via [`requires_content`](CallableSpec::requires_content). The default
/// method bodies describe the neutral callable — no arguments, no body — suitable for
/// simple specials (`~`) and fallback specs. The declarative standard implementation is
/// [`StdCallableSpec`].
///
/// The behavioral surface is [`make_invocation_parser`](CallableSpec::make_invocation_parser):
/// a factory returning a fresh boxed [`ConstructParser`] per resolved [`Invocation`],
/// defaulting to the declarative [`StdInvocationParser`]. Overriding it is the
/// full-takeover route for `\verb`-like constructs.
///
/// **Thread safety is part of the contract** (`Send + Sync` supertraits): specs are
/// stored in parsed trees, so `NodeTree: Send + Sync` requires it.
/// Every method takes `&self`, so a stateful implementation needs interior mutability
/// regardless — under this contract that means locks or atomics (`Mutex`/`RwLock`/
/// `OnceLock`, or `spin` on `no_std`), not `RefCell`/`Cell`.
///
/// **Downcasting is part of the contract** (`Any` supertrait — shared by every stored
/// extension trait: [`SpecsProvider`](crate::scopes::SpecsProvider),
/// [`ArgumentParser`](crate::spec::ArgumentParser),
/// [`EnvironmentBehavior`](crate::latexlike::EnvironmentBehavior),
/// [`SourceResolver`](crate::source::SourceResolver)): a
/// consumer recovers the concrete type from a stored `Arc<dyn _>` or `&dyn _` via
/// trait upcasting — `(&*spec as &dyn core::any::Any).downcast_ref::<MySpec>()`. A
/// preset's [`Lang::make_node_ext`](crate::state::Lang::make_node_ext)
/// mint does exactly this with the `Arc<dyn CallableSpec<L>>` a node stores. The
/// `Arc`'d trait objects were already implicitly `'static`; the supertrait makes it
/// per-implementor law. Downcasting to a preset's own spec *trait* (an open set of
/// third-party spec types) needs one extra move: register every spec behind one
/// concrete wrapper (`FlmSpecBox(Arc<dyn FlmSpec>)` delegating to the inner value) and
/// downcast to the wrapper.
///
/// **Serialization is part of the contract** ([`SerializableObject`] supertrait): a
/// spec stored in a parsed tree must be serializable through the
/// `Arc<dyn CallableSpec<L>>` the node holds, so the write-side capability sits on
/// every spec type. It is fully
/// defaulted — a type that does not participate in serialization adds the one-line
/// empty impl (`impl<L: Lang> SerializableObject<L> for MySpec<L> {}`) and nothing
/// else; a participating type overrides
/// [`serialize_object`](SerializableObject::serialize_object). The two
/// argument-spec methods below ([`serialize_argument_spec`] and
/// [`deserialize_argument_spec`]) belong to the same capability. All of it is
/// callable only for a language that implements [`SerializableLang`]; for any other
/// language these methods cannot be called.
///
/// [`serialize_argument_spec`]: CallableSpec::serialize_argument_spec
/// [`deserialize_argument_spec`]: CallableSpec::deserialize_argument_spec
pub trait CallableSpec<L: Lang>: fmt::Debug + Send + Sync + Any + SerializableObject<L> {
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
    /// [`StdInvocationParser`]'s documentation for the full contract, including the
    /// post-space rule).
    ///
    /// The default returns the declarative [`StdInvocationParser`]. **Overriding this
    /// factory is the full-takeover route**: a custom parser reads tokens
    /// however it wants (`\verb` raw content, tabular preambles), stages its own node
    /// shape, and may return a state delta as the invocation's after-effect for
    /// subsequent siblings (`\newcommand`).
    ///
    /// The `FromInvocation` clause is the standard parser's
    /// ([`StdInvocationParser`] mints the invocation-syntax payload from the
    /// bundle); a trait method's clause cannot be per-implementation, so overriding
    /// takeovers inherit it — every language driving the standard dispatch
    /// satisfies it anyway (`()` and the latexlike payload are covered by techy).
    ///
    /// # Errors
    ///
    /// `Err` means **the parser could not be built** — what the factory needs
    /// (spec-side definition data, an embedding's runtime) is broken or
    /// unavailable — and **aborts the parse** under any recovery policy; the
    /// dispatch site attaches the live traceback when the error carries no
    /// frames of its own. Refusing to parse *deeper* is deliberately not this
    /// channel's business: nesting depth belongs to the descent guard
    /// ([`DescentLimitExceeded`](crate::constructs::DescentLimitExceeded), raised
    /// before any factory-built parser runs). Carry
    /// [`HookFailed`](crate::error::HookFailed) for an operational failure,
    /// [`ImplementationError`](crate::constructs::ImplementationError) for a
    /// violated library contract. An infallible implementation wraps its parser
    /// in `Ok(...)` and that is the only change.
    // The boxed-parser-or-abort pair is the decided factory signature; an alias
    // would only rename it.
    #[allow(clippy::type_complexity)]
    fn make_invocation_parser<'a>(
        &'a self,
        invocation: Invocation<'a, L>,
    ) -> Result<
        Box<dyn ConstructParser<L, Output = BuildId> + 'a>,
        crate::error::ParseError<L::SourceOrigin>,
    >
    where
        L::InvocationSyntax: FromInvocation<L>,
    {
        Ok(Box::new(StdInvocationParser::new(invocation)))
    }

    /// Title of a parse-traceback frame covering this callable: called at *snapshot* time — only when a condition is recorded —
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

    /// Serialize the argument spec a parsed argument of this callable was parsed
    /// against — the [`ArgumentSpec`] `Arc` recorded on the parsed argument at
    /// position `index` (invocation order). Called by the tree serialization for each
    /// parsed argument of a callable node whose spec is `self`.
    ///
    /// **The index rule (the default).** Normally a parsed argument's spec *is* the
    /// callable spec's declared one: the `Arc` on the parsed argument is the very same
    /// allocation as `self.arguments()[index]`. The default checks exactly that
    /// (`Arc::ptr_eq`, pointer identity — not structural equality) and returns
    /// `Ok(None)`: nothing needs writing beyond the index, and reading rebuilds the
    /// spec as `self.arguments()[index]` again (the default of
    /// [`deserialize_argument_spec`](CallableSpec::deserialize_argument_spec)).
    ///
    /// **When to override.** A callable spec whose invocation parser hands parsed
    /// arguments an argument spec that is *not* one of its declared ones — minted per
    /// invocation, say, or chosen among alternatives — has *out-of-band* argument
    /// specs; the default cannot serialize those. Such a spec overrides this method to
    /// return `Ok(Some(value))` with whatever describes the argument spec, and
    /// overrides `deserialize_argument_spec` to rebuild it from that value.
    /// `argument_spec` is the parsed argument's own `Arc`, and `cx` gives the call
    /// access to the state of the serialization in progress.
    ///
    /// # Errors
    ///
    /// The default reports [`SerializeError::ArgumentSpecOutOfBand`] when `index` is
    /// beyond `self.arguments()` or the `Arc` at that index is not `argument_spec`
    /// itself. An override returns an error when it cannot describe the argument spec.
    fn serialize_argument_spec(
        &self,
        index: usize,
        argument_spec: &Arc<ArgumentSpec<L>>,
        cx: &mut SerializeContext<'_, L>,
    ) -> Result<Option<SerialValue>, SerializeError>
    where
        L: SerializableLang,
    {
        let _ = cx;
        let declared = self.arguments();
        match declared.get(index) {
            Some(spec) if Arc::ptr_eq(spec, argument_spec) => Ok(None),
            _ => Err(SerializeError::ArgumentSpecOutOfBand { index, count: declared.len() }),
        }
    }

    /// Rebuild the argument spec of a parsed argument of this callable from its
    /// serialized form — the counterpart of
    /// [`serialize_argument_spec`](CallableSpec::serialize_argument_spec), called on the
    /// freshly rebuilt callable spec for each serialized argument at position `index`
    /// (invocation order). `value` is what `serialize_argument_spec` returned when the
    /// argument was written: `None` under the index rule, `Some(_)` from an override.
    /// `cx` gives the call access to the state of the deserialization in progress.
    ///
    /// The default implements the index rule: for `value == None` it returns a clone
    /// of `self.arguments()[index]`, the declared argument spec at that position. It
    /// reads no description: a `Some(_)` value reaching the default is an error, not
    /// something to ignore — it was written by a callable spec type that overrides
    /// `serialize_argument_spec`, so the reading environment's callable spec is not
    /// of the type that wrote the argument. A spec that overrides
    /// `serialize_argument_spec` overrides this method as well, rebuilding the
    /// argument spec from `value`.
    ///
    /// # Errors
    ///
    /// The default reports [`DeserializeError::UnexpectedArgumentSpecPayload`] when
    /// `value` is `Some(_)`, and [`DeserializeError::ArgumentIndexOutOfRange`] when
    /// `index` is beyond `self.arguments()` — the serialized data was written against
    /// a callable spec declaring more arguments than this one does. An override
    /// returns an error when `value` does not describe an argument spec it can
    /// rebuild.
    fn deserialize_argument_spec(
        &self,
        index: usize,
        value: Option<&SerialValue>,
        cx: &mut DeserializeContext<'_, L>,
    ) -> Result<Arc<ArgumentSpec<L>>, DeserializeError>
    where
        L: SerializableLang,
    {
        let _ = cx;
        if value.is_some() {
            return Err(DeserializeError::UnexpectedArgumentSpecPayload { index });
        }
        let declared = self.arguments();
        declared.get(index).cloned().ok_or(DeserializeError::ArgumentIndexOutOfRange {
            index,
            count: declared.len(),
        })
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
///
/// **Serialization.** The argument structure holds parsers, which have no serialized
/// form, so a `StdCallableSpec` is serialized by *identity* — as a reference to the
/// provider that defined it plus its key — which requires the spec to carry a
/// [`SpecProvenance`] stamp in its [`provenance`](StdCallableSpec::provenance) field
/// ([`with_provenance`](StdCallableSpec::with_provenance) sets it; a shared
/// [`Package`](crate::scopes::Package) hands the stamp out). Serializing an unstamped
/// `StdCallableSpec` is an error naming the type. Both fields are public — the type
/// is plain data, built by [`new`](StdCallableSpec::new) or as a struct literal
/// (`StdCallableSpec { arguments, ..Default::default() }` for pre-`Arc`'d argument
/// specs).
pub struct StdCallableSpec<L: Lang> {
    /// The argument structure.
    pub arguments: Vec<Arc<ArgumentSpec<L>>>,
    /// Where the spec was defined, when known: the stamp that lets the spec be
    /// serialized by identity (`None` for a spec built outside a shared package).
    pub provenance: Option<SpecProvenance<L>>,
}

impl<L: Lang> StdCallableSpec<L> {
    /// A spec with the given argument structure — specs by value, `Arc`'d inside
    /// (`StdCallableSpec::new([ArgumentSpec::new(parser, "title")])`, no `Arc::new`
    /// noise) and no provenance stamp. A caller that shares pre-`Arc`'d specs
    /// (flyweights referenced from parsed records) builds the struct literal instead
    /// (`StdCallableSpec { arguments, ..Default::default() }`).
    pub fn new(
        arguments: impl IntoIterator<Item = ArgumentSpec<L>>,
    ) -> StdCallableSpec<L> {
        StdCallableSpec {
            arguments: arguments.into_iter().map(Arc::new).collect(),
            provenance: None,
        }
    }

    /// Record where this spec is defined — the [`SpecProvenance`] stamp a shared
    /// package hands out ([`Package::provenance_for`](crate::scopes::Package::provenance_for))
    /// — so that the spec can be serialized by identity. Replaces a previous stamp.
    pub fn with_provenance(mut self, provenance: SpecProvenance<L>) -> StdCallableSpec<L> {
        self.provenance = Some(provenance);
        self
    }

    /// The provenance stamp, if the spec carries one (the
    /// [`provenance`](StdCallableSpec::provenance) field, as a getter — the method
    /// name the preset's spec types share).
    pub fn provenance(&self) -> Option<&SpecProvenance<L>> {
        self.provenance.as_ref()
    }
}

impl<L: Lang> CallableSpec<L> for StdCallableSpec<L> {
    fn arguments(&self) -> &[Arc<ArgumentSpec<L>>] {
        &self.arguments
    }
}

// The `SerializableObject` (identity through the provenance stamp) impl lives in
// `crate::serialize::drivers::specs`, with the other core spec/provider types'.

// Manual impls: derives would demand `L: Clone`/`L: Debug`/`L: Default` although only
// `Arc`s to spec data are stored.

impl<L: Lang> Default for StdCallableSpec<L> {
    fn default() -> Self {
        StdCallableSpec { arguments: Vec::new(), provenance: None }
    }
}

impl<L: Lang> Clone for StdCallableSpec<L> {
    fn clone(&self) -> Self {
        StdCallableSpec { arguments: self.arguments.clone(), provenance: self.provenance.clone() }
    }
}

impl<L: Lang> fmt::Debug for StdCallableSpec<L> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("StdCallableSpec")
            .field("arguments", &self.arguments)
            .field("provenance", &self.provenance)
            .finish()
    }
}
