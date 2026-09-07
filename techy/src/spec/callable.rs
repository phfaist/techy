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

/// Which part of a callable's parse a live traceback [`Frame`](crate::core::Frame)
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

/// The behavior of anything invocable from the token stream: which arguments it takes
/// and how its invocation is parsed.
///
/// A spec has no name and no invocation form of its own. The name belongs to the
/// key a [`SpecsProvider`](crate::core::specs::SpecsProvider) stores the spec under, so
/// one spec value may be stored under several names (`\emph` and `\textit` can share
/// one), and one shared spec can answer for every unknown callable of a callable type.
///
/// Most definitions do not implement this trait at all: [`StdCallableSpec`] is the
/// standard declarative implementation, and the latexlike preset's `MacroSpec` and
/// `EnvironmentSpec` build on it. Implement `CallableSpec` yourself when a callable
/// needs behavior no argument list can express.
///
/// The declarative surface is the [`ArgumentSpec`] list returned by
/// [`arguments`](CallableSpec::arguments) — arguments *configure* an invocation
/// (pylatexenc's `arguments_spec_list`). The list is `Arc`-shared so that a parsed node
/// can record which argument spec each of its arguments was parsed against. Slots — a
/// parsed callable's *content regions*, such as an environment body — have no
/// declaration here: the parser that reads the body creates the
/// [`ParsedSlot`](crate::core::node::ParsedSlot) records itself, and the spec announces
/// that it takes material at all through
/// [`requires_content`](CallableSpec::requires_content).
///
/// The behavioral surface is [`make_invocation_parser`](CallableSpec::make_invocation_parser):
/// a factory returning a fresh boxed [`ConstructParser`] per resolved [`Invocation`],
/// defaulting to the declarative [`StdInvocationParser`]. Overriding it takes over the
/// entire invocation parse, which is what `\verb`-like constructs need.
///
/// Every method has a default, and the defaults together describe the neutral
/// callable — no arguments, no body — which is what a simple specials definition (`~`)
/// or a fallback spec needs.
///
/// # Implementing this trait
///
/// **Thread safety is part of the contract** (`Send + Sync` supertraits): specs are
/// stored in parsed trees, so `NodeTree: Send + Sync` requires it. Every method takes
/// `&self`, so a stateful implementation needs interior mutability in any case — under
/// this contract that means locks or atomics (`Mutex`/`RwLock`/`OnceLock`, or `spin` on
/// `no_std`), not `RefCell`/`Cell`.
///
/// **Downcasting is part of the contract** (`Any` supertrait): a consumer recovers the
/// concrete type from a stored `Arc<dyn CallableSpec<L>>` or `&dyn CallableSpec<L>` by
/// upcasting — `(&*spec as &dyn core::any::Any).downcast_ref::<MySpec>()`. A preset's
/// [`Lang::make_node_ext`](crate::core::Lang::make_node_ext) does exactly this with the
/// spec a node stores. Every stored extension trait of the crate shares the supertrait:
/// [`SpecsProvider`](crate::core::specs::SpecsProvider),
/// [`ArgumentParser`](crate::core::constructs::ArgumentParser),
/// [`EnvironmentBehavior`](crate::latexlike::EnvironmentBehavior),
/// [`SourceResolver`](crate::source::SourceResolver).
///
/// Downcasting to a preset's own spec *trait* — an open set of third-party spec types —
/// needs one extra step: store every such spec behind one concrete wrapper type
/// (`FlmSpecBox(Arc<dyn FlmSpec>)`, delegating to the inner value) and downcast to the
/// wrapper.
///
/// **Serialization is part of the contract** ([`SerializableObject`] supertrait): a
/// spec stored in a parsed tree must be serializable through the
/// `Arc<dyn CallableSpec<L>>` the node holds, so the write-side capability sits on
/// every spec type. It is fully defaulted: a type that does not participate in
/// serialization adds the one-line empty impl
/// (`impl<L: Lang> SerializableObject<L> for MySpec<L> {}`) and nothing else, while a
/// participating type overrides
/// [`serialize_object`](SerializableObject::serialize_object). The two argument-spec
/// methods below ([`serialize_argument_spec`] and [`deserialize_argument_spec`]) belong
/// to the same capability. All of it is callable only for a language that implements
/// [`SerializableLang`]; for any other language these methods cannot be called.
///
/// [`serialize_argument_spec`]: CallableSpec::serialize_argument_spec
/// [`deserialize_argument_spec`]: CallableSpec::deserialize_argument_spec
pub trait CallableSpec<L: Lang>: fmt::Debug + Send + Sync + Any + SerializableObject<L> {
    /// The declarative argument structure of an invocation, in invocation order.
    /// Default: no arguments.
    fn arguments(&self) -> &[Arc<ArgumentSpec<L>>] {
        &[]
    }

    /// Whether an invocation of this callable requires material after it — whether
    /// appearing **bare**, as a single-token expression argument (`\frac\mymacro 2`),
    /// would be malformed.
    ///
    /// The guard at an expression position consults this before dispatching (the
    /// spec-side face of pylatexenc's `contents_can_be_empty` consultation): `true`
    /// diagnoses the bare use and stages the single-token callable with every declared
    /// argument absent; `false` dispatches the invocation in full.
    ///
    /// The default derives the answer from the declared arguments: content is required
    /// exactly when some declared argument cannot match empty
    /// ([`ArgumentParser::can_match_empty`](super::ArgumentParser::can_match_empty)).
    ///
    /// Because slots are not declared spec-side, this method is the **only** way for a
    /// callable that takes over its own parse to say "I consume material": a spec that
    /// declares no arguments but consumes plenty (a `\begin` dispatcher, `\verb`) must
    /// override it to `true`.
    fn requires_content(&self) -> bool {
        self.arguments().iter().any(|argument| !argument.parser.can_match_empty())
    }

    /// Returns a fresh parser for one resolved invocation of this callable.
    ///
    /// The dispatch loop resolves the trigger, builds the [`Invocation`], calls this
    /// factory, runs `parser.parse(cx)` once, and drops the parser: one boxed parser
    /// per invocation, owned by the caller, owning the invocation in turn (pylatexenc's
    /// `get_node_parser(token)`, with ownership made explicit).
    ///
    /// When the parser runs, the trigger token has already been consumed whole by the
    /// dispatching arm; the parser's `cx.state` is the invocation's base state (see
    /// [`StdInvocationParser`]'s documentation for the full contract, including the
    /// post-space rule).
    ///
    /// The default returns the declarative [`StdInvocationParser`], which parses the
    /// spec's declared arguments. **Overriding this factory takes over the whole
    /// invocation parse**: a custom parser reads tokens however it wants (`\verb` raw
    /// content, tabular preambles), stages its own node shape, and may return a state
    /// delta as the invocation's after-effect for subsequent siblings (`\newcommand`).
    ///
    /// The `FromInvocation` bound is the standard parser's requirement
    /// ([`StdInvocationParser`] builds the invocation-syntax payload from the
    /// invocation). A trait method's bound cannot vary per implementation, so an
    /// overriding parser inherits it; every language driving the standard dispatch
    /// satisfies it anyway (`()` and the latexlike payload are covered by techy).
    ///
    /// # Errors
    ///
    /// `Err` means **the parser could not be built** — what the factory needs
    /// (spec-side definition data, an embedding's runtime) is broken or
    /// unavailable — and **aborts the parse** under any recovery policy; the
    /// dispatch site attaches the live traceback when the error has no
    /// frames of its own
    /// ([`ParseContext::attach_hook_frames`](crate::core::constructs::ParseContext::attach_hook_frames)).
    /// Refusing to parse *deeper* is deliberately not this
    /// channel's business: nesting depth belongs to the descent guard
    /// ([`DescentLimitExceeded`](crate::core::constructs::DescentLimitExceeded), raised
    /// before any factory-built parser runs). Carry
    /// [`HookFailed`](crate::error::HookFailed) for an operational failure,
    /// [`ImplementationError`](crate::core::constructs::ImplementationError) for a
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

    /// Returns the title of a parse-traceback frame covering this callable.
    ///
    /// `role` says which part of the parse the frame covers, and `name` is the
    /// invocation spelling as written (`\frac`, `~`), sliced from the source — a spec
    /// stores no name of its own, so it is passed in.
    ///
    /// This is called only when a diagnostic is recorded and the traceback is captured,
    /// never when a frame is pushed, so live frames need no allocation.
    ///
    /// The default renders `callable ‘\frac’` and `argument #1 of ‘\frac’`, the core
    /// having no vocabulary of construct kinds; a preset overrides this to speak its
    /// own ("macro ‘\frac’", "environment ‘align’").
    fn stack_frame_title(&self, role: FrameRole, name: &str) -> String {
        match role {
            FrameRole::Invocation => format!("callable ‘{}’", name),
            FrameRole::Argument { index } => {
                format!("argument #{} of ‘{}’", index + 1, name)
            }
        }
    }

    /// Where this spec is defined, when it records that: the [`SpecProvenance`] stamp
    /// issued by a shared [`Package`](crate::core::specs::Package)
    /// ([`Package::provenance_for`](crate::core::specs::Package::provenance_for)) and
    /// stored by the spec type. Reading it through the `Arc<dyn CallableSpec<L>>` a
    /// node holds needs no downcast to the concrete spec type.
    ///
    /// The default is `None`. A spec type that records a stamp — every shipped spec
    /// type does, through its `with_provenance` builder — returns it from here. This is
    /// a reading accessor only: serialization by identity reads the stamp inside the
    /// type's own [`SerializableObject`] implementation.
    fn provenance(&self) -> Option<&SpecProvenance<L>> {
        None
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
    /// **When to override.** A callable spec whose invocation parser gives its parsed
    /// arguments an argument spec that is *not* one of its declared ones — built per
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

/// Sealed conversion into a shared [`CallableSpec`] — what
/// [`Package::insert`](crate::core::specs::Package::insert) and its siblings accept as
/// their spec argument.
///
/// A spec passes **by value** (`insert(CallableType::Macro, "emph", MacroSpec::new(…))`,
/// with no `Arc::new` at the call site), while an already-shared **`Arc<S>`** or
/// **`Arc<dyn CallableSpec<L>>`** passes through unchanged rather than being wrapped a
/// second time — so one spec value registered under several names stays one value. The
/// same conversion pattern applies to providers
/// ([`IntoSpecsProvider`](crate::core::specs::IntoSpecsProvider)) and argument parsers
/// ([`IntoArgumentParser`](super::IntoArgumentParser)).
///
/// Sealed: the three implementations are the whole vocabulary; downstream code
/// implements [`CallableSpec`], never this trait. (The `M` parameter is a sealed
/// inference marker distinguishing the three argument shapes — it never needs to be
/// named.)
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

/// The standard declarative [`CallableSpec`]: a callable's argument structure as plain
/// data, and nothing else.
///
/// [`new`](StdCallableSpec::new) takes the argument specs in invocation order. The
/// result is stored under a name in a [`Package`](crate::core::specs::Package) or a
/// [`Scope`](crate::core::specs::Scope) like any other spec.
///
/// ```
/// use techy::core::constructs::MarkerArgumentParser;
/// use techy::core::specs::{ArgumentSpec, StdCallableSpec};
/// use techy::latexlike::Latexlike;
///
/// // One optional `*` marker argument, named so parsed nodes can be asked for it by
/// // name rather than by position.
/// let spec: StdCallableSpec<Latexlike> =
///     StdCallableSpec::new([ArgumentSpec::new(MarkerArgumentParser::new("*"), "starred")]);
/// assert_eq!(spec.arguments.len(), 1);
/// ```
///
/// For a latexlike language the argument list is usually written as **argument codes**
/// rather than built parser by parser — `"o"` for an optional `[…]` argument, `"m"` for
/// a mandatory one; [`argument_specs`](crate::latexlike::argument_specs) documents the
/// full code table. The preset's own spec types
/// ([`MacroSpec`](crate::latexlike::MacroSpec),
/// [`EnvironmentSpec`](crate::latexlike::EnvironmentSpec)) have this same shape and
/// additionally name their construct kind in parse tracebacks, and
/// [`Package::define_macro`](crate::core::specs::Package::define_macro) registers one
/// in a single line.
///
/// Both fields are public, so the type can equally be built as a struct literal —
/// `StdCallableSpec { arguments, ..Default::default() }` takes argument specs that are
/// already `Arc`-shared.
///
/// **Serialization.** The argument structure holds parsers, which have no serialized
/// form, so a `StdCallableSpec` is serialized by *identity* — as a reference to the
/// provider that defined it plus its key — which requires the spec to carry a
/// [`SpecProvenance`] stamp in its [`provenance`](field@StdCallableSpec::provenance)
/// field ([`with_provenance`](StdCallableSpec::with_provenance) sets it; a shared
/// [`Package`](crate::core::specs::Package) issues the stamp). Serializing an unstamped
/// `StdCallableSpec` is an error naming the type.
pub struct StdCallableSpec<L: Lang> {
    /// The argument structure.
    pub arguments: Vec<Arc<ArgumentSpec<L>>>,
    /// Where the spec was defined, when known: the stamp that lets the spec be
    /// serialized by identity (`None` for a spec built outside a shared package).
    pub provenance: Option<SpecProvenance<L>>,
}

impl<L: Lang> StdCallableSpec<L> {
    /// A spec with the given argument structure, in invocation order, and no
    /// provenance stamp.
    ///
    /// The argument specs pass by value and are `Arc`-wrapped inside
    /// (`StdCallableSpec::new([ArgumentSpec::new(parser, "title")])`, with no
    /// `Arc::new` at the call site). To reuse argument specs that are already
    /// `Arc`-shared, build the struct literal instead:
    /// `StdCallableSpec { arguments, ..Default::default() }`.
    pub fn new(
        arguments: impl IntoIterator<Item = ArgumentSpec<L>>,
    ) -> StdCallableSpec<L> {
        StdCallableSpec {
            arguments: arguments.into_iter().map(Arc::new).collect(),
            provenance: None,
        }
    }

    /// Records where this spec is defined, so that it can be serialized by identity:
    /// the [`SpecProvenance`] stamp issued by a shared package
    /// ([`Package::provenance_for`](crate::core::specs::Package::provenance_for)).
    ///
    /// Replaces a previous stamp. Read it back through [`CallableSpec::provenance`] or
    /// the public field.
    pub fn with_provenance(mut self, provenance: SpecProvenance<L>) -> StdCallableSpec<L> {
        self.provenance = Some(provenance);
        self
    }
}

impl<L: Lang> CallableSpec<L> for StdCallableSpec<L> {
    fn arguments(&self) -> &[Arc<ArgumentSpec<L>>] {
        &self.arguments
    }

    fn provenance(&self) -> Option<&SpecProvenance<L>> {
        self.provenance.as_ref()
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
