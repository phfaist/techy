//! The preset's declarative callable specs: [`MacroSpec`] and [`SpecialsSpec`].
//!
//! Both hold an argument list as plain data and nothing else, so defining a macro or a
//! specials trigger comes down to declaring its arguments — usually with the
//! [argument codes](super::argument_specs). They are preset types rather than the
//! generic [`StdCallableSpec`](crate::core::specs::StdCallableSpec) for two reasons:
//! parse tracebacks then speak the preset's vocabulary ("macro ‘\frac’", "specials
//! ‘~’" instead of "callable ‘…’"), and each type is a stable target for code that
//! recognizes a spec by downcasting it. The environment counterpart,
//! [`EnvironmentSpec`](super::EnvironmentSpec), also holds the body behavior and is
//! defined with the `\begin` composition.
//!
//! The one-line definition methods on a latexlike [`Package`] are defined here too:
//! [`define_macro`](Package::define_macro) and
//! [`define_environment`](Package::define_environment).

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

/// A macro definition: its argument structure as plain data.
///
/// Register one under [`CallableType::Macro`](super::CallableType::Macro) in a
/// [`Package`] or a [`Scope`](crate::core::specs::Scope), and the
/// [`LatexlikeDriver`](super::LatexlikeDriver) resolves every command token against
/// the scope stack in force. [`Package::define_macro`] spells the definition and the
/// registration in one line.
///
/// Any [`CallableSpec`] implementation can be registered under that role, including
/// one that takes over parsing of its invocation entirely. What this type adds is the
/// preset traceback vocabulary and one optional behavior: the after-effect of
/// [`with_after_effect`](MacroSpec::with_after_effect).
///
/// Build one with [`new`](MacroSpec::new), [`Default`], or
/// [`with_after_effect`](MacroSpec::with_after_effect). The after-effect field is
/// private, so there is no struct-literal form; the public `arguments` field can be
/// read and assigned on a value you own.
///
/// The type is generic over the language family (`LLL`, [`LatexlikeLang`], by default
/// [`Latexlike`]), so a family member registers the same declarative shape under its
/// own marker type.
///
/// # Examples
///
/// ```
/// use techy::core::specs::Package;
/// use techy::latexlike::{argument_specs, CallableType, Latexlike, MacroSpec};
///
/// let mut package: Package<Latexlike> = Package::new("mydefs");
/// package.insert(
///     CallableType::Macro,
///     "cite",
///     MacroSpec::new(argument_specs(["o", "m"]).unwrap()),
/// );
/// ```
///
/// # Serialization
///
/// The argument structure holds parsers, which have no serialized form, so a macro
/// spec is serialized by *identity*: a reference to the provider that defined it plus
/// its key. That requires the [`SpecProvenance`] stamp a shared package issues
/// ([`with_provenance`](MacroSpec::with_provenance)), which
/// [`Package::define_macro`] applies for you in a shared package. Serializing an
/// unstamped macro spec is an error naming this type.
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
    /// A macro taking the given arguments, in invocation order.
    ///
    /// The argument specs usually come from the argument codes
    /// ([`argument_specs`](super::argument_specs), or
    /// [`argument_specs_named`](super::argument_specs_named) to name them); an empty
    /// list declares a macro with no arguments, as [`Default`] does.
    pub fn new(arguments: Vec<Arc<ArgumentSpec<LLL>>>) -> MacroSpec<LLL> {
        MacroSpec { arguments, after_effect: None, provenance: None }
    }

    /// Records where this spec is defined, so that it can be serialized by identity.
    ///
    /// `provenance` is the stamp a shared package issues
    /// ([`Package::provenance_for`]); a stamp set earlier is replaced.
    /// [`Package::define_macro`] applies it for you in a shared package.
    pub fn with_provenance(mut self, provenance: SpecProvenance<LLL>) -> MacroSpec<LLL> {
        self.provenance = Some(provenance);
        self
    }

    /// Gives the macro an after-effect: a parsing-state change every invocation
    /// leaves behind for the content that follows it.
    ///
    /// The change reaches the invocation's later siblings, to the end of the enclosing
    /// group or of the document — the way a definition macro (`\newcommand`-style)
    /// makes a name usable for the rest of the group.
    ///
    /// `delta` is applied to the surrounding state after the invocation's node is
    /// staged, so it never affects the invocation's own arguments. After-effects
    /// accumulate in document order: each later sibling parses under every delta left
    /// before it. Like all parsing state, the effect is scoped — content outside the
    /// enclosing group is unaffected.
    ///
    /// For `\input`-like inclusion, whether after-effects arising *inside* the
    /// included content persist past the inclusion is that construct's own choice
    /// (the `persist_state` parameter of
    /// [`input_macro_spec`](super::input_macro_spec)).
    ///
    /// Calling this a second time replaces the delta set before.
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

    fn provenance(&self) -> Option<&SpecProvenance<LLL>> {
        self.provenance.as_ref()
    }

    /// Never fails: this implementation only wraps the parser it builds in `Ok`.
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

/// The invocation parser of a [`MacroSpec`] with an after-effect: the standard
/// declarative parse, followed by the spec's delta as the invocation's after-effect
/// for the following siblings.
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

/// A specials definition: the argument structure of a specials-form callable as plain
/// data.
///
/// The counterpart of [`MacroSpec`] for triggers that are not commands — `~`, `---`,
/// `$` — registered with
/// [`Package::insert_specials`](crate::core::specs::Package::insert_specials) under
/// [`CallableType::Specials`](super::CallableType::Specials). Most specials take no
/// arguments at all, which is what [`Default`] builds; parse tracebacks speak of
/// "specials ‘~’".
///
/// The trigger sequence is the registration key rather than part of the spec, so one
/// spec can serve several triggers: the
/// [`minilatex_package`](super::minidefs::minilatex_package) registers a single
/// argument-less instance for all of its typography triggers.
///
/// Generic over the language family (`LLL`, [`LatexlikeLang`], by default
/// [`Latexlike`]), like [`MacroSpec`]. Paragraph-break nodes are not specified by this
/// type: the driver stamps them with the canonical
/// [`ParagraphBreakSpec`](super::ParagraphBreakSpec).
///
/// # Serialization
///
/// As for [`MacroSpec`]: by identity, through the [`SpecProvenance`] stamp of
/// [`with_provenance`](SpecialsSpec::with_provenance) — the stamp of a specials
/// definition is [`Package::provenance_for_specials`]. An unstamped specials spec
/// cannot be serialized.
pub struct SpecialsSpec<LLL: LatexlikeLang = Latexlike> {
    /// The argument structure, in invocation order.
    pub arguments: Vec<Arc<ArgumentSpec<LLL>>>,
    /// Where the spec was defined, when known.
    provenance: Option<SpecProvenance<LLL>>,
}

impl<LLL: LatexlikeLang> SpecialsSpec<LLL> {
    /// A specials callable taking the given arguments, in invocation order.
    ///
    /// The argument specs usually come from the argument codes
    /// ([`argument_specs`](super::argument_specs)); an empty list declares a specials
    /// trigger with no arguments, the shape most of them have.
    pub fn new(arguments: Vec<Arc<ArgumentSpec<LLL>>>) -> SpecialsSpec<LLL> {
        SpecialsSpec { arguments, provenance: None }
    }

    /// Records where this spec is defined, so that it can be serialized by identity.
    ///
    /// `provenance` is the stamp a shared package issues for a specials definition
    /// ([`Package::provenance_for_specials`]); a stamp set earlier is replaced.
    pub fn with_provenance(mut self, provenance: SpecProvenance<LLL>) -> SpecialsSpec<LLL> {
        self.provenance = Some(provenance);
        self
    }
}

// The `SerializableObject` impl (identity through the provenance stamp) lives in
// `super::serialize`.

impl<LLL: LatexlikeLang> CallableSpec<LLL> for SpecialsSpec<LLL> {
    fn arguments(&self) -> &[Arc<ArgumentSpec<LLL>>] {
        &self.arguments
    }

    fn provenance(&self) -> Option<&SpecProvenance<LLL>> {
        self.provenance.as_ref()
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

/// The preset's one-line definition methods on a latexlike [`Package`]: each pairs
/// the callable type with the matching spec type by construction and resolves its
/// argument codes on the spot. They are a shorter spelling of the same
/// [`insert`](Package::insert) operation, not a second registration model.
impl<LLL: LatexlikeLang> Package<LLL> {
    /// Defines the macro `name` with the given argument codes.
    ///
    /// One line for the whole
    /// `insert(macro_callable(), name, MacroSpec::new(argument_specs(codes)?))`
    /// ceremony:
    ///
    /// ```
    /// # use techy::core::specs::Package;
    /// # use techy::latexlike::Latexlike;
    /// let mut package: Package<Latexlike> = Package::new("mydefs");
    /// package.define_macro("includegraphics", ["o", "m"]).unwrap();
    /// ```
    ///
    /// `codes` is [`argument_specs`](super::argument_specs)'s list form, word codes
    /// included. On success this returns the spec previously defined under the key,
    /// like [`insert`](Package::insert).
    ///
    /// `name` follows [`insert`](Package::insert)'s normalized-spelling contract and
    /// carries no escape character. No escape-character validation happens here
    /// either, deliberately: escape characters can change during a parse, and a name
    /// beginning with what looks like one can be intended.
    ///
    /// In a package built with [`Package::new_shared`] the spec is stamped with its
    /// provenance ([`MacroSpec::with_provenance`]) so that it can be serialized by
    /// identity; in one built with [`Package::new`] there is nothing to stamp it
    /// with, and it is not.
    ///
    /// # Errors
    ///
    /// Returns the [`ArgumentCodeError`] of a malformed code. The definition is then
    /// not inserted, and the package is unchanged.
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

    /// Defines the environment `name` with the given argument codes.
    ///
    /// The [`define_macro`](Package::define_macro) sibling for the environment role:
    /// it builds an [`EnvironmentSpec`](super::EnvironmentSpec) with the default body
    /// handling. For a body-scoped state change or a custom body behavior, build the
    /// [`EnvironmentSpec`](super::EnvironmentSpec) yourself and
    /// [`insert`](Package::insert) it.
    ///
    /// ```
    /// # use techy::core::specs::Package;
    /// # use techy::latexlike::Latexlike;
    /// let mut package: Package<Latexlike> = Package::new("mydefs");
    /// package.define_environment("figure", ["o"]).unwrap();
    /// ```
    ///
    /// The arguments are the ones parsed after `\begin{name}`. In a shared package
    /// the spec is stamped with its provenance, as in
    /// [`define_macro`](Package::define_macro), and the spec previously defined under
    /// the key is returned.
    ///
    /// # Errors
    ///
    /// Returns the [`ArgumentCodeError`] of a malformed code. The definition is then
    /// not inserted, and the package is unchanged.
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
