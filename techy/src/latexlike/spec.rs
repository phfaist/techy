//! [`MacroSpec`] and [`SpecialsSpec`]: the preset's declarative callable specs.
//!
//! Both are [`StdCallableSpec`](crate::spec::StdCallableSpec)-shaped — an argument
//! list as plain data — as concrete preset types (decided at the 7.6 checkpoint):
//! parse tracebacks speak the preset's vocabulary ("macro ‘\frac’", "specials ‘~’"
//! instead of the core's "callable ‘…’"), and each type is a stable downcast target
//! for preset `finalize_node`-style hooks. The environment counterpart is
//! [`EnvironmentSpec`](super::EnvironmentSpec), which carries body behavior and lives
//! with the `\begin` composition.

use alloc::format;
use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec::Vec;

use crate::spec::{ArgumentSpec, CallableSpec, FrameRole};

use super::Latexlike;

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
#[derive(Debug, Clone, Default)]
pub struct MacroSpec {
    /// The argument structure, in invocation order.
    pub arguments: Vec<Arc<ArgumentSpec<Latexlike>>>,
}

impl MacroSpec {
    /// A macro with the given argument structure.
    pub fn new(arguments: Vec<Arc<ArgumentSpec<Latexlike>>>) -> MacroSpec {
        MacroSpec { arguments }
    }
}

impl CallableSpec<Latexlike> for MacroSpec {
    fn arguments(&self) -> &[Arc<ArgumentSpec<Latexlike>>] {
        &self.arguments
    }

    fn stack_frame_title(&self, role: FrameRole, name: &str) -> String {
        frame_title("macro", role, name)
    }
}

/// The preset's declarative specials spec: the argument structure of a
/// [`Specials`](super::CallableType::Specials) callable as plain data, with the
/// preset's traceback vocabulary ("specials ‘~’").
///
/// Registered via [`Package::insert_specials`](crate::scopes::Package::insert_specials);
/// the trigger sequence is the registration key, not spec data (specs are de-keyed —
/// the [`base_package`](super::base_package) registers one shared argument-less
/// instance for all its triggers).
#[derive(Debug, Clone, Default)]
pub struct SpecialsSpec {
    /// The argument structure, in invocation order.
    pub arguments: Vec<Arc<ArgumentSpec<Latexlike>>>,
}

impl SpecialsSpec {
    /// A specials callable with the given argument structure.
    pub fn new(arguments: Vec<Arc<ArgumentSpec<Latexlike>>>) -> SpecialsSpec {
        SpecialsSpec { arguments }
    }
}

impl CallableSpec<Latexlike> for SpecialsSpec {
    fn arguments(&self) -> &[Arc<ArgumentSpec<Latexlike>>] {
        &self.arguments
    }

    fn stack_frame_title(&self, role: FrameRole, name: &str) -> String {
        frame_title("specials", role, name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::constructs::GroupArgumentParser;
    use crate::engine::Language;
    use crate::node::check_tree_invariants;
    use crate::scopes::Package;
    use crate::state::ParsingStateDelta;
    use alloc::vec;

    use super::super::{CallableType, GroupType};

    fn brace_arg() -> Arc<ArgumentSpec<Latexlike>> {
        Arc::new(ArgumentSpec::new(Arc::new(GroupArgumentParser::new(GroupType::Content))))
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

        let tilde = SpecialsSpec::default();
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
        assert!(!SpecialsSpec::default().requires_content());
    }

    #[test]
    fn macro_spec_parses_through_the_language() {
        let mut package = Package::new("defs");
        package.insert(
            CallableType::Macro,
            "emph",
            Arc::new(MacroSpec::new(vec![brace_arg()])),
        );
        let language = Language::<Latexlike>::default()
            .with_seed_delta(ParsingStateDelta::new().push_provider(Arc::new(package)))
            .unwrap();

        let result = language.parse(r"\emph{x} y").unwrap();
        check_tree_invariants(&result.tree);
        assert!(result.diagnostics.is_empty());
        let emph = result.tree.root().child(0).unwrap();
        assert_eq!(emph.macro_name(), Some("emph"));
        assert_eq!(emph.arguments().unwrap().len(), 1);
        assert!(emph.arguments().unwrap().get(0).unwrap().is_provided());
    }
}
