//! [`MacroSpec`] and [`SpecialsSpec`]: the preset's declarative callable specs.
//!
//! Both are [`StdCallableSpec`](crate::spec::StdCallableSpec)-shaped — an argument
//! list as plain data — as concrete preset types (decided at the 7.6 checkpoint):
//! parse tracebacks speak the preset's vocabulary ("macro ‘\frac’", "specials ‘~’"
//! instead of the core's "callable ‘…’"), and each type is a stable downcast target
//! for preset `make_node_ext`-style minting. The environment counterpart is
//! [`EnvironmentSpec`](super::EnvironmentSpec), which carries body behavior and lives
//! with the `\begin` composition.

use alloc::format;
use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::fmt;

use crate::spec::{ArgumentSpec, CallableSpec, FrameRole};

use super::{Latexlike, LatexlikeLang};

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
/// the scope stack. Any [`CallableSpec`] works there — this type adds no behavior
/// beyond the vocabulary, and generic specs ([`StdCallableSpec`](crate::spec::StdCallableSpec),
/// custom takeovers) remain first-class.
///
/// Generic over the language family (`LLL`, [`LatexlikeLang`]; defaulting to
/// [`Latexlike`]) — a family member registers the same declarative macro shape
/// under its own marker type.
pub struct MacroSpec<LLL: LatexlikeLang = Latexlike> {
    /// The argument structure, in invocation order.
    pub arguments: Vec<Arc<ArgumentSpec<LLL>>>,
}

impl<LLL: LatexlikeLang> MacroSpec<LLL> {
    /// A macro with the given argument structure.
    pub fn new(arguments: Vec<Arc<ArgumentSpec<LLL>>>) -> MacroSpec<LLL> {
        MacroSpec { arguments }
    }
}

impl<LLL: LatexlikeLang> CallableSpec<LLL> for MacroSpec<LLL> {
    fn arguments(&self) -> &[Arc<ArgumentSpec<LLL>>] {
        &self.arguments
    }

    fn stack_frame_title(&self, role: FrameRole, name: &str) -> String {
        frame_title("macro", role, name)
    }
}

// Manual impls: derives would demand `LLL: Debug`/`Clone`/`Default` although only
// `Arc`s are stored.

impl<LLL: LatexlikeLang> fmt::Debug for MacroSpec<LLL> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("MacroSpec").field("arguments", &self.arguments).finish()
    }
}

impl<LLL: LatexlikeLang> Clone for MacroSpec<LLL> {
    fn clone(&self) -> Self {
        MacroSpec { arguments: self.arguments.clone() }
    }
}

impl<LLL: LatexlikeLang> Default for MacroSpec<LLL> {
    fn default() -> Self {
        MacroSpec { arguments: Vec::new() }
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
/// Generic over the language family (`LLL`, [`LatexlikeLang`]; defaulting to
/// [`Latexlike`]) — it is also what the paragraph-break pillar
/// ([`make_paragraph_break_node`](super::make_paragraph_break_node)) stamps on
/// `Specials`-style break nodes for any family member.
pub struct SpecialsSpec<LLL: LatexlikeLang = Latexlike> {
    /// The argument structure, in invocation order.
    pub arguments: Vec<Arc<ArgumentSpec<LLL>>>,
}

impl<LLL: LatexlikeLang> SpecialsSpec<LLL> {
    /// A specials callable with the given argument structure.
    pub fn new(arguments: Vec<Arc<ArgumentSpec<LLL>>>) -> SpecialsSpec<LLL> {
        SpecialsSpec { arguments }
    }
}

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
        f.debug_struct("SpecialsSpec").field("arguments", &self.arguments).finish()
    }
}

impl<LLL: LatexlikeLang> Clone for SpecialsSpec<LLL> {
    fn clone(&self) -> Self {
        SpecialsSpec { arguments: self.arguments.clone() }
    }
}

impl<LLL: LatexlikeLang> Default for SpecialsSpec<LLL> {
    fn default() -> Self {
        SpecialsSpec { arguments: Vec::new() }
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
            ParsingState::lang_initial_with_packages([package]),
        );

        let result = language.parse(r"\emph{x} y").unwrap();
        check_latexlike_tree_invariants(&result.tree);
        assert!(result.diagnostics.is_empty());
        let emph = result.tree.root().child(0).unwrap();
        assert_eq!(emph.macro_name(), Some("emph"));
        assert_eq!(emph.arguments().unwrap().len(), 1);
        assert!(emph.arguments().unwrap().get(0).unwrap().is_provided());
    }
}
