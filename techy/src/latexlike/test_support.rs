//! Shared `#[cfg(test)]` helpers for the latexlike preset's test modules ([`mod.rs`],
//! [`node_ref.rs`], and later `environments.rs`): one `Language`/shape/package
//! vocabulary so each sibling test file doesn't re-derive its own (7.5 review — #13/#14).
//!
//! Shrunk in 7.9: the genuinely multi-purpose pieces were promoted to public API —
//! the compact node description is [`NodeRef::summary`], and pushing a provider onto
//! a language's seed is [`Language::with_provider`]. What remains here is thin
//! test-only wiring.

use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec::Vec;

use crate::engine::{Language, ParseResult};
use crate::error::Recovery;
use crate::node::check_tree_invariants;
use crate::scopes::Package;
use crate::spec::StdCallableSpec;

use super::{CallableType, Latexlike, LatexlikeDriver, Mode};

/// A strict-recovery latexlike `Language` (the seed defaults).
pub(super) fn strict() -> Language<Latexlike> {
    Language::default()
}

/// A tolerant-recovery latexlike `Language`.
pub(super) fn tolerant() -> Language<Latexlike> {
    Language::new(LatexlikeDriver::new(Recovery::Tolerant))
}

/// The root list's child summaries ([`NodeRef::summary`]).
pub(super) fn root_shapes(result: &ParseResult<Latexlike>) -> Vec<String> {
    result.tree.root().children().iter().map(|node| node.summary()).collect()
}

/// Strict-parse `input`, assert no diagnostics and valid tree invariants, and return the
/// root child summaries.
pub(super) fn parse_shapes(input: &str) -> Vec<String> {
    let result = strict().parse(input).unwrap();
    check_tree_invariants(&result.tree);
    assert!(result.diagnostics.is_empty(), "unexpected diagnostics: {:?}", result.diagnostics);
    root_shapes(&result)
}

/// A package named `pkg_name` defining `macro_name` as a zero-argument
/// [`Macro`](CallableType::Macro), optionally restricted (package-level) to
/// `visible_modes`.
pub(super) fn macro_package(
    pkg_name: &str,
    macro_name: &str,
    visible_modes: Option<Vec<Mode>>,
) -> Package<Latexlike> {
    let mut package = Package::new(pkg_name);
    package.insert(CallableType::Macro, macro_name, Arc::new(StdCallableSpec::new(Vec::new())));
    if visible_modes.is_some() {
        package.set_visible_modes(visible_modes);
    }
    package
}
