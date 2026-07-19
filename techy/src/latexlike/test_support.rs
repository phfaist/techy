//! Shared `#[cfg(test)]` helpers for the latexlike preset's test modules ([`mod.rs`],
//! [`node_ref.rs`], and later `environments.rs`): one `Language`/shape/package
//! vocabulary so each sibling test file doesn't re-derive its own (7.5 review — #13/#14).

use alloc::format;
use alloc::string::{String, ToString};
use alloc::sync::Arc;
use alloc::vec::Vec;

use crate::engine::{Language, ParseResult};
use crate::error::Recovery;
use crate::node::{check_tree_invariants, NodeRef};
use crate::scopes::Package;
use crate::spec::StdCallableSpec;
use crate::state::ParsingStateDelta;

use super::{CallableType, Latexlike, LatexlikeDriver, Mode};

/// A strict-recovery latexlike `Language` (the seed defaults).
pub(super) fn strict() -> Language<Latexlike> {
    Language::default()
}

/// A tolerant-recovery latexlike `Language`.
pub(super) fn tolerant() -> Language<Latexlike> {
    Language::new(LatexlikeDriver::new(Recovery::Tolerant))
}

/// Compact shape string for a node: `chars(text)`, `group(Math $ $)`, `Macro(emph)`,
/// `Specials(~)`, `comment(text)`.
pub(super) fn shape(node: NodeRef<'_, Latexlike>) -> String {
    if let Some(text) = node.chars() {
        format!("chars({text})")
    } else if node.is_group() {
        let class = node
            .group_type()
            .map_or_else(|| "?".to_string(), |group_type| format!("{group_type:?}"));
        let (open, close) = node.group_delimiters().unwrap();
        format!("group({class} {open} {close})")
    } else if node.is_callable() {
        format!("{:?}({})", node.callable_type().unwrap(), node.name().unwrap())
    } else if let Some(text) = node.comment() {
        format!("comment({text})")
    } else {
        "other".to_string()
    }
}

/// The root list's child shapes.
pub(super) fn root_shapes(result: &ParseResult<Latexlike>) -> Vec<String> {
    result.tree.root().children().map(shape).collect()
}

/// Strict-parse `input`, assert no diagnostics and valid tree invariants, and return the
/// root child shapes.
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

/// `language` with `package` pushed as a seed-delta provider.
pub(super) fn with_provider(
    language: Language<Latexlike>,
    package: Package<Latexlike>,
) -> Language<Latexlike> {
    language
        .with_seed_delta(ParsingStateDelta::new().push_provider(Arc::new(package)))
        .unwrap()
}
