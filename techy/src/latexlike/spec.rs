//! [`MacroSpec`] and [`SpecialsSpec`]: the preset's declarative callable specs.
//!
//! Both are [`StdCallableSpec`](crate::spec::StdCallableSpec)-shaped — an argument
//! list as plain data — as concrete preset types (decided at the 7.6 checkpoint):
//! parse tracebacks speak the preset's vocabulary ("macro ‘\frac’", "specials ‘~’"
//! instead of the core's "callable ‘…’"), and each type is a stable downcast target
//! for preset `make_node_ext`-style minting. The environment counterpart is
//! [`EnvironmentSpec`](super::EnvironmentSpec), which carries body behavior and lives
//! with the `\begin` composition.

use alloc::boxed::Box;
use alloc::format;
use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::fmt;

use crate::constructs::{
    ConstructParser, ConstructParserResult, Invocation, ParseContext, StdInvocationParser,
};
use crate::node::{ArgumentExt, BuildId};
use crate::scopes::{Package, SpecProvenance};
use crate::spec::{ArgumentSpec, CallableSpec, FrameRole};
use crate::state::ParsingStateDelta;

use super::{
    argument_specs, ArgumentCodeError, EnvironmentSpec, Latexlike, LatexlikeCallableType,
    LatexlikeLang,
};

/// Render a preset frame title: the callable-kind word (`macro`, `environment`,
/// `specials`) with the invocation spelling — the shared body of the preset's
/// [`stack_frame_title`](CallableSpec::stack_frame_title) overrides.
pub(crate) fn frame_title(kind: &str, role: FrameRole, name: &str) -> String {
    match role {
        FrameRole::Invocation => format!("{kind} ‘{name}’"),
        FrameRole::Argument { index } => {
            format!("argument #{} of {kind} ‘{name}’", index + 1)
        }
    }
}

/// The preset's declarative macro spec: the argument structure of a
/// [`Macro`](super::CallableType::Macro) callable as plain data, with the preset's
/// traceback vocabulary ("macro ‘\frac’").
///
/// Registered under [`CallableType::Macro`](super::CallableType::Macro) in a
/// [`Package`](crate::scopes::Package) or [`Scope`](crate::scopes::Scope); the
/// [`LatexlikeDriver`](super::LatexlikeDriver) resolves every command token through
/// the scope stack. Any [`CallableSpec`] works there — beyond the vocabulary, this
/// type adds only the optional after-effect
/// ([`with_after_effect`](MacroSpec::with_after_effect)), and generic specs
/// ([`StdCallableSpec`](crate::spec::StdCallableSpec),
/// custom takeovers) remain first-class.
///
/// Constructed through [`new`](MacroSpec::new), [`Default`], or
/// [`with_after_effect`](MacroSpec::with_after_effect) — the after-effect field is
/// private, so there is no struct-literal form; the public `arguments` field stays
/// readable and assignable on an owned value.
///
/// **Serialization.** The argument structure holds parsers, which have no
/// serialized form, so a macro spec is serialized by *identity* — a reference to the
/// provider that defined it plus its key — which needs the [`SpecProvenance`] stamp
/// a shared package hands out ([`with_provenance`](MacroSpec::with_provenance);
/// [`Package::define_macro`] stamps automatically in a shared package). An
/// unstamped macro spec cannot be serialized (the error names the type).
///
/// Generic over the language family (`LLL`, [`LatexlikeLang`]; defaulting to
/// [`Latexlike`]) — a family member registers the same declarative macro shape
/// under its own marker type.
pub struct MacroSpec<LLL: LatexlikeLang = Latexlike> {
    /// The argument structure, in invocation order.
    pub arguments: Vec<Arc<ArgumentSpec<LLL>>>,
    /// The parsing-state change an invocation leaves behind for the content that
    /// follows it; `None` (the default) leaves the surrounding state untouched.
    /// Set with [`with_after_effect`](MacroSpec::with_after_effect).
    after_effect: Option<ParsingStateDelta<LLL>>,
    /// Where the spec was defined, when known.
    provenance: Option<SpecProvenance<LLL>>,
}

impl<LLL: LatexlikeLang> MacroSpec<LLL> {
    /// A macro with the given argument structure.
    pub fn new(arguments: Vec<Arc<ArgumentSpec<LLL>>>) -> MacroSpec<LLL> {
        MacroSpec { arguments, after_effect: None, provenance: None }
    }

    /// Record where this spec is defined — the [`SpecProvenance`] stamp a shared
    /// package hands out ([`Package::provenance_for`]) — so that the spec can be
    /// serialized by identity. Replaces a previous stamp.
    pub fn with_provenance(mut self, provenance: SpecProvenance<LLL>) -> MacroSpec<LLL> {
        self.provenance = Some(provenance);
        self
    }

    /// Where this spec is defined, if it was stamped.
    pub fn provenance(&self) -> Option<&SpecProvenance<LLL>> {
        self.provenance.as_ref()
    }

    /// Give the macro an **after-effect**: a parsing-state change every invocation
    /// leaves behind for the content that *follows* it — its later siblings, to the
    /// end of the enclosing group or document — the way a definition macro
    /// (`\newcommand`-style) makes a name usable for the rest of the group.
    ///
    /// `delta` is applied to the surrounding state after the invocation's node is
    /// staged; it never affects the invocation's own arguments. Invocations'
    /// after-effects accumulate in document order: each later sibling parses under
    /// every delta left before it. The effect is scoped like all parsing state —
    /// content outside the enclosing group is not affected. For `\input`-like
    /// inclusion, whether after-effects arising *inside* the included content
    /// persist past the inclusion is that construct's own choice
    /// ([`input_macro_spec`](super::input_macro_spec)'s `persist_state` parameter).
    ///
    /// Calling this a second time replaces the previously set delta.
    pub fn with_after_effect(mut self, delta: ParsingStateDelta<LLL>) -> MacroSpec<LLL> {
        self.after_effect = Some(delta);
        self
    }
}

// The `SerializableObject` impl (identity through the provenance stamp) lives in
// `super::serialize`, with the preset's other serialization impls.

impl<LLL: LatexlikeLang> CallableSpec<LLL> for MacroSpec<LLL> {
    fn arguments(&self) -> &[Arc<ArgumentSpec<LLL>>] {
        &self.arguments
    }

    /// Infallible: `Ok(...)` wrapping is this implementation's whole use of the
    /// `Result`.
    fn make_invocation_parser<'a>(
        &'a self,
        invocation: Invocation<'a, LLL>,
    ) -> Result<
        Box<dyn ConstructParser<LLL, Output = BuildId> + 'a>,
        crate::error::ParseError<LLL::SourceOrigin>,
    >
    {
        let inner = StdInvocationParser::new(invocation);
        Ok(match &self.after_effect {
            None => Box::new(inner),
            Some(delta) => Box::new(AfterEffectInvocationParser { inner, delta }),
        })
    }

    fn stack_frame_title(&self, role: FrameRole, name: &str) -> String {
        frame_title("macro", role, name)
    }
}

/// The invocation parser of a [`MacroSpec`] carrying an after-effect: the standard
/// declarative parse, then the spec's delta as the invocation's after-effect for
/// the following siblings.
struct AfterEffectInvocationParser<'a, LLL: LatexlikeLang> {
    inner: StdInvocationParser<'a, LLL>,
    delta: &'a ParsingStateDelta<LLL>,
}

impl<LLL: LatexlikeLang> ConstructParser<LLL> for AfterEffectInvocationParser<'_, LLL> {
    type Output = BuildId;

    fn parse(
        &mut self,
        cx: &mut ParseContext<'_, '_, LLL>,
    ) -> ConstructParserResult<LLL, (BuildId, Option<Box<ParsingStateDelta<LLL>>>)> {
        // The discarded tuple slot is the inner parse's own sibling after-effect;
        // `StdInvocationParser` never produces one.
        let (id, _) = self.inner.parse(cx)?;
        Ok((id, Some(Box::new(self.delta.clone()))))
    }
}

// Manual impls: derives would demand `LLL: Debug`/`Clone`/`Default` although only
// `Arc`s are stored.

impl<LLL: LatexlikeLang> fmt::Debug for MacroSpec<LLL> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("MacroSpec")
            .field("arguments", &self.arguments)
            .field("after_effect", &self.after_effect)
            .field("provenance", &self.provenance)
            .finish()
    }
}

impl<LLL: LatexlikeLang> Clone for MacroSpec<LLL> {
    fn clone(&self) -> Self {
        MacroSpec {
            arguments: self.arguments.clone(),
            after_effect: self.after_effect.clone(),
            provenance: self.provenance.clone(),
        }
    }
}

impl<LLL: LatexlikeLang> Default for MacroSpec<LLL> {
    fn default() -> Self {
        MacroSpec { arguments: Vec::new(), after_effect: None, provenance: None }
    }
}

/// The preset's declarative specials spec: the argument structure of a
/// specials-form callable ([`CallableType::Specials`](super::CallableType::Specials))
/// as plain data, with the preset's traceback vocabulary ("specials ‘~’").
///
/// Registered via [`Package::insert_specials`](crate::scopes::Package::insert_specials);
/// the trigger sequence is the registration key, not spec data (specs are de-keyed —
/// the [`minilatex_package`](super::minidefs::minilatex_package) registers one shared
/// argument-less instance for all its typography triggers).
///
/// **Serialization.** Like [`MacroSpec`]: by identity through a [`SpecProvenance`]
/// stamp ([`with_provenance`](SpecialsSpec::with_provenance); the stamp of a specials
/// definition is [`Package::provenance_for_specials`]) — an unstamped specials spec
/// cannot be serialized.
///
/// Generic over the language family (`LLL`, [`LatexlikeLang`]; defaulting to
/// [`Latexlike`]) — it is also what the paragraph-break behavior function
/// ([`make_paragraph_break_node`](super::make_paragraph_break_node)) stamps on
/// `Specials`-style break nodes for any family member.
pub struct SpecialsSpec<LLL: LatexlikeLang = Latexlike> {
    /// The argument structure, in invocation order.
    pub arguments: Vec<Arc<ArgumentSpec<LLL>>>,
    /// Where the spec was defined, when known.
    provenance: Option<SpecProvenance<LLL>>,
}

impl<LLL: LatexlikeLang> SpecialsSpec<LLL> {
    /// A specials callable with the given argument structure.
    pub fn new(arguments: Vec<Arc<ArgumentSpec<LLL>>>) -> SpecialsSpec<LLL> {
        SpecialsSpec { arguments, provenance: None }
    }

    /// Record where this spec is defined — the [`SpecProvenance`] stamp a shared
    /// package hands out ([`Package::provenance_for_specials`]) — so that the spec
    /// can be serialized by identity. Replaces a previous stamp.
    pub fn with_provenance(mut self, provenance: SpecProvenance<LLL>) -> SpecialsSpec<LLL> {
        self.provenance = Some(provenance);
        self
    }

    /// Where this spec is defined, if it was stamped.
    pub fn provenance(&self) -> Option<&SpecProvenance<LLL>> {
        self.provenance.as_ref()
    }
}

// The `SerializableObject` impl (identity through the provenance stamp) lives in
// `super::serialize`.

impl<LLL: LatexlikeLang> CallableSpec<LLL> for SpecialsSpec<LLL> {
    fn arguments(&self) -> &[Arc<ArgumentSpec<LLL>>] {
        &self.arguments
    }

    fn stack_frame_title(&self, role: FrameRole, name: &str) -> String {
        frame_title("specials", role, name)
    }
}

// Manual impls: derives would demand `LLL: Debug`/`Clone`/`Default` although only
// `Arc`s are stored.

impl<LLL: LatexlikeLang> fmt::Debug for SpecialsSpec<LLL> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SpecialsSpec")
            .field("arguments", &self.arguments)
            .field("provenance", &self.provenance)
            .finish()
    }
}

impl<LLL: LatexlikeLang> Clone for SpecialsSpec<LLL> {
    fn clone(&self) -> Self {
        SpecialsSpec { arguments: self.arguments.clone(), provenance: self.provenance.clone() }
    }
}

impl<LLL: LatexlikeLang> Default for SpecialsSpec<LLL> {
    fn default() -> Self {
        SpecialsSpec { arguments: Vec::new(), provenance: None }
    }
}

// --- the preset one-liners on Package ------------------------------------------------

/// The preset **definition one-liners** — inherent methods on latexlike-shaped
/// [`Package`]s, written in the preset module (the
/// in-crate mechanism behind the `NodeRef` sugar as well): each pairs the callable
/// type and spec type correctly *by construction* and parses its argument codes
/// eagerly. A shorter spelling of the same [`insert`](Package::insert) operation —
/// deliberately not a second registration model.
impl<LLL: LatexlikeLang> Package<LLL> {
    /// Define the macro `name` with the given argument codes — one line for the
    /// full `insert(macro_callable(), name, MacroSpec::new(argument_specs(…)?))`
    /// ceremony:
    ///
    /// ```
    /// # use techy::core::specs::Package;
    /// # use techy::latexlike::Latexlike;
    /// let mut package: Package<Latexlike> = Package::new("mydefs");
    /// package.define_macro("includegraphics", ["o", "m"]).unwrap();
    /// ```
    ///
    /// `codes` is [`argument_specs`](super::argument_specs)'s list form (word codes
    /// included); a malformed code is the eager
    /// [`Err`](super::ArgumentCodeError). On success, returns the spec previously
    /// defined under the key, like [`insert`](Package::insert). The name follows
    /// [`insert`](Package::insert)'s normalized-spelling contract (no escape
    /// character), and — deliberately — **no escape-char validation happens here
    /// either** (escape characters can change mid-parse, and a leading
    /// escape-character-like char can be intended).
    ///
    /// In a package built shared ([`Package::new_shared`]) the spec is stamped with
    /// its provenance ([`MacroSpec::with_provenance`]), so that it can be serialized
    /// by identity; in a package built with [`Package::new`] it is not (nothing to
    /// stamp with).
    pub fn define_macro<I>(
        &mut self,
        name: impl Into<Box<str>>,
        codes: I,
    ) -> Result<Option<Arc<dyn CallableSpec<LLL>>>, ArgumentCodeError>
    where
        ArgumentExt<LLL>: Default,
        I: IntoIterator,
        I::Item: AsRef<str>,
    {
        let callable_type = LLL::CallableTypeId::macro_callable();
        let name: Box<str> = name.into();
        let mut spec = MacroSpec::new(argument_specs(codes)?);
        if let Some(provenance) = self.provenance_for(callable_type, &*name) {
            spec = spec.with_provenance(provenance);
        }
        Ok(self.insert(callable_type, name, spec))
    }

    /// Define the environment `name` with the given argument codes — the
    /// [`define_macro`](Package::define_macro) sibling over
    /// [`EnvironmentSpec`](super::EnvironmentSpec) under the environment role
    /// (default body handling; for body deltas or custom behavior, build the
    /// [`EnvironmentSpec`](super::EnvironmentSpec) yourself and
    /// [`insert`](Package::insert) it). Stamps the spec with its provenance in a
    /// shared package, like [`define_macro`](Package::define_macro).
    pub fn define_environment<I>(
        &mut self,
        name: impl Into<Box<str>>,
        codes: I,
    ) -> Result<Option<Arc<dyn CallableSpec<LLL>>>, ArgumentCodeError>
    where
        ArgumentExt<LLL>: Default,
        I: IntoIterator,
        I::Item: AsRef<str>,
    {
        let callable_type = LLL::CallableTypeId::environment_callable();
        let name: Box<str> = name.into();
        let mut spec = EnvironmentSpec::new(argument_specs(codes)?);
        if let Some(provenance) = self.provenance_for(callable_type, &*name) {
            spec = spec.with_provenance(provenance);
        }
        Ok(self.insert(callable_type, name, spec))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::constructs::GroupArgumentParser;
    use crate::engine::Language;
    use crate::latexlike::check_latexlike_tree_invariants;
    use crate::scopes::Package;
    use crate::state::ParsingState;
    use crate::latexlike::LatexlikeDriver;
    use alloc::vec;

    use super::super::{CallableType, GroupType};

    fn brace_arg() -> Arc<ArgumentSpec<Latexlike>> {
        Arc::new(ArgumentSpec::new_unnamed(Arc::new(GroupArgumentParser::new(GroupType::Content))))
    }

    #[test]
    fn frame_titles_speak_the_preset_vocabulary() {
        let frac = MacroSpec::new(vec![brace_arg(), brace_arg()]);
        assert_eq!(
            frac.stack_frame_title(FrameRole::Invocation, "\\frac"),
            "macro ‘\\frac’"
        );
        assert_eq!(
            frac.stack_frame_title(FrameRole::Argument { index: 0 }, "\\frac"),
            "argument #1 of macro ‘\\frac’"
        );

        let tilde: SpecialsSpec = SpecialsSpec::default();
        assert_eq!(tilde.stack_frame_title(FrameRole::Invocation, "~"), "specials ‘~’");
        assert_eq!(
            tilde.stack_frame_title(FrameRole::Argument { index: 1 }, "~"),
            "argument #2 of specials ‘~’"
        );
    }

    #[test]
    fn macro_spec_exposes_its_structure_through_the_trait() {
        let spec = MacroSpec::new(vec![brace_arg()]);
        let dyn_spec: &dyn CallableSpec<Latexlike> = &spec;
        assert_eq!(dyn_spec.arguments().len(), 1);
        // One non-emptiable argument ⇒ bare expression use is diagnosed.
        assert!(dyn_spec.requires_content());
        assert!(!SpecialsSpec::<Latexlike>::default().requires_content());
    }

    #[test]
    fn macro_spec_parses_through_the_language() {
        let mut package = Package::new("defs");
        package.insert(
            CallableType::Macro,
            "emph",
            MacroSpec::new(vec![brace_arg()]),
        );
        let language = Language::new(
            LatexlikeDriver::new(crate::error::Recovery::Strict),
            ParsingState::lang_initial_with_packages([package]).expect("seed state"),
        );

        let result = language.parse(r"\emph{x} y").unwrap();
        check_latexlike_tree_invariants(&result.tree);
        assert!(result.diagnostics.is_empty());
        let emph = result.tree.root().child(0).unwrap();
        assert_eq!(emph.macro_name(), Some("emph"));
        assert_eq!(emph.arguments().unwrap().len(), 1);
        assert!(emph.arguments().unwrap().get(0).unwrap().is_provided());
    }

    /// A package defining `\{defined}` as a zero-argument macro.
    fn defining(defined: &str) -> Arc<Package<Latexlike>> {
        let mut package: Package<Latexlike> = Package::new("defined");
        package.insert(CallableType::Macro, defined, MacroSpec::default());
        Arc::new(package)
    }

    /// A tolerant language where `\{name}` carries the after-effect "define `\{defined}`".
    fn language_with_definer(pairs: &[(&str, &str)]) -> Language<Latexlike> {
        let mut lib: Package<Latexlike> = Package::new("defs");
        for (name, defined) in pairs {
            lib.insert(
                CallableType::Macro,
                *name,
                MacroSpec::new(vec![]).with_after_effect(
                    crate::state::ParsingStateDelta::new().push_provider(defining(defined)),
                ),
            );
        }
        Language::new(
            LatexlikeDriver::new(crate::error::Recovery::Tolerant),
            ParsingState::lang_initial_with_packages([lib]).expect("seed state"),
        )
    }

    #[test]
    fn an_after_effect_changes_state_for_following_siblings_only() {
        let language = language_with_definer(&[("def", "x")]);

        // The after-effect makes `\x` resolvable for the rest of the content run…
        let result = language.parse(r"\def\x").unwrap();
        check_latexlike_tree_invariants(&result.tree);
        assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);

        // …never retroactively (`\x` before `\def` stays unresolvable)…
        let result = language.parse(r"\x\def").unwrap();
        assert_eq!(result.diagnostics.len(), 1);

        // …and scoped: leaving the enclosing group reverts the state, so a `\x`
        // after the group is unresolvable again.
        let result = language.parse(r"{\def\x}\x").unwrap();
        check_latexlike_tree_invariants(&result.tree);
        assert_eq!(result.diagnostics.len(), 1);
    }

    #[test]
    fn two_sibling_after_effects_merge_in_document_order() {
        let language = language_with_definer(&[("defx", "x"), ("defy", "y")]);
        let result = language.parse(r"\defx\defy\x\y").unwrap();
        check_latexlike_tree_invariants(&result.tree);
        // Both effects are live after both invocations: `\x` and `\y` resolve.
        assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
    }

    #[test]
    fn define_one_liners_pair_type_and_spec_by_construction() {
        let mut package: Package<Latexlike> = Package::new("mydefs");
        package.define_macro("emph", ["m"]).unwrap();
        package.define_environment("enumerate", ["o"]).unwrap();

        // A malformed code errors eagerly, before anything is inserted.
        assert!(package.define_macro("bad", ["x"]).is_err());
        assert!(package.get(CallableType::Macro, "bad").is_none());

        // Redefinition returns the replaced spec, like `insert`.
        assert!(package.define_macro("emph", ["m"]).unwrap().is_some());

        let language = Language::new(
            LatexlikeDriver::new(crate::error::Recovery::Strict),
            ParsingState::lang_initial_with_packages([package]).expect("seed state"),
        );
        let result = language
            .parse("\\emph{x}\\begin{enumerate}[i] y\\end{enumerate}")
            .unwrap();
        check_latexlike_tree_invariants(&result.tree);
        assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
        let emph = result.tree.root().child(0).unwrap();
        assert_eq!(emph.macro_name(), Some("emph"));
        let env = result.tree.root().child(1).unwrap();
        assert_eq!(env.environment_name(), Some("enumerate"));
        assert!(env.arguments().unwrap().get(0).unwrap().is_provided());
    }
}
