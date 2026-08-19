//! Shared `#[cfg(test)]` helpers for the latexlike preset's test modules ([`mod.rs`],
//! [`node_ref.rs`], and later `environments.rs`): one `Language`/shape/package
//! vocabulary so each sibling test file doesn't re-derive its own (7.5 review — #13/#14).
//!
//! Shrunk in 7.9: the genuinely multi-purpose pieces were promoted to public API —
//! the compact node description is [`NodeRef::summary`], and pushing packages onto
//! a language's seed is [`ParsingState::lang_initial_with_packages`]. What remains
//! here is thin test-only wiring.

use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec::Vec;

use crate::engine::{Language, ParseResult};
use crate::error::Recovery;
use crate::latexlike::check_latexlike_tree_invariants;
use crate::scopes::{IntoSpecsProvider, Package};
use crate::spec::StdCallableSpec;
use crate::state::ParsingState;

use super::{CallableType, GroupType, Latexlike, LatexlikeDriver, LatexlikeLang, Mode};
use crate::node::NodeKind;
use crate::source::SourceSpan;
use crate::state::Lang;

/// A strict-recovery latexlike `Language` (the seed defaults).
pub(super) fn strict() -> Language<Latexlike> {
    Language::new(
        LatexlikeDriver::new(Recovery::Strict),
        ParsingState::lang_initial().expect("seed state"),
    )
}

/// A tolerant-recovery latexlike `Language`.
pub(super) fn tolerant() -> Language<Latexlike> {
    Language::new(
        LatexlikeDriver::new(Recovery::Tolerant),
        ParsingState::lang_initial().expect("seed state"),
    )
}

/// A latexlike `Language` under `recovery` with `package` pushed innermost over the
/// seed defaults.
pub(super) fn with_package(recovery: Recovery, package: impl IntoSpecsProvider<Latexlike>) -> Language<Latexlike> {
    Language::new(
        LatexlikeDriver::new(recovery),
        ParsingState::lang_initial_with_packages([package]).expect("seed state"),
    )
}

/// A latexlike `Language` under `recovery` with several `packages` pushed over the
/// seed defaults (in iteration order — the last is innermost).
pub(super) fn with_packages(
    recovery: Recovery,
    packages: impl IntoIterator<Item: IntoSpecsProvider<Latexlike>>,
) -> Language<Latexlike> {
    Language::new(
        LatexlikeDriver::new(recovery),
        ParsingState::lang_initial_with_packages(packages).expect("seed state"),
    )
}

/// The root list's child summaries ([`NodeRef::summary`]).
pub(super) fn root_shapes(result: &ParseResult<Latexlike>) -> Vec<String> {
    result.tree.root().children().iter().map(|node| node.summary()).collect()
}

/// Strict-parse `input`, assert no diagnostics and valid tree invariants, and return the
/// root child summaries.
pub(super) fn parse_shapes(input: &str) -> Vec<String> {
    let result = strict().parse(input).unwrap();
    check_latexlike_tree_invariants(&result.tree);
    assert!(result.diagnostics.is_empty(), "unexpected diagnostics: {:?}", result.diagnostics);
    root_shapes(&result)
}

/// A shared package named `pkg_name` defining `macro_name` as a zero-argument
/// [`Macro`](CallableType::Macro) (stamped, so it serializes), optionally restricted
/// (package-level) to `visible_modes`.
pub(super) fn macro_package(
    pkg_name: &str,
    macro_name: &str,
    visible_modes: Option<Vec<Mode>>,
) -> Arc<Package<Latexlike>> {
    Package::new_shared(pkg_name, |package| {
        let mut spec = StdCallableSpec::<Latexlike>::default();
        if let Some(provenance) = package.provenance_for(CallableType::Macro, macro_name) {
            spec = spec.with_provenance(provenance);
        }
        package.insert(CallableType::Macro, macro_name, Arc::new(spec));
        if visible_modes.is_some() {
            package.set_visible_modes(visible_modes);
        }
    })
}

/// A member of the latexlike family whose parse trees are **not** span-tiled: the
/// preset's vocabularies and behavior, only the declaration differs. The preset's
/// parsers are generic over the family, so they serve it unchanged — which is what
/// makes the recovery node below the site under test.
#[derive(Debug, Clone, Copy)]
pub(super) struct RelaxedLatexlike;

impl Lang for RelaxedLatexlike {
    const OBEYS_SPAN_TILING: bool = false;

    type Features = crate::state::AllLangFeatures;
    type GroupTypeId = GroupType;
    type CallableTypeId = CallableType;
    type ModeId = Mode;
    type StateExt = ();
    type Event = crate::latexlike::Event;
    type SessionExt = ();
    type SourceOrigin = Option<String>;
    type Tokenization = crate::token::StdTokenization;
    type NodeExts = crate::latexlike::LatexlikeNodeExts;
    type InvocationSyntax =
        crate::latexlike::InvocationSyntaxData<crate::latexlike::StdEnvironmentSyntax<Self>>;
    type Driver = LatexlikeDriver<Self>;

    /// The preset's own seed, for this language's vocabularies.
    fn initial_state_data() -> Result<crate::state::StateData<Self>, crate::state::FinalizeError>
    {
        let mut scopes = crate::scopes::ScopeStack::new();
        scopes.push(crate::latexlike::builtin_package());
        Ok(crate::state::StateData {
            rules: crate::latexlike::default_token_rules(),
            scopes,
            mode: Mode::Text,
            ext: (),
        })
    }

    fn scan_specials(
        state: &ParsingState<Self>,
        content: &str,
        pos: usize,
    ) -> Result<Option<crate::token::SpecialsMatch<Self>>, crate::token::SpecialsScanError>
    {
        state.scopes().scan_specials(state, content, pos)
    }

    fn specials_trigger_chars(
        data: &crate::state::StateData<Self>,
    ) -> crate::token::TriggerChars {
        data.scopes.specials_trigger_chars()
    }

    fn make_node_ext(
        _kind: &NodeKind<Self>,
        _span: &SourceSpan<Self::SourceOrigin>,
        _state: &Arc<ParsingState<Self>>,
        _children: crate::node::StagedChildren<'_, Self>,
    ) -> Result<(), crate::node::NodeBuildError> {
        Ok(())
    }
}

impl LatexlikeLang for RelaxedLatexlike {}
