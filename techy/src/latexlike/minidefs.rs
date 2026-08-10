//! [`minilatex_package`]: a toy definitions package — deliberately **not** a
//! definitions database.
//!
//! One package, `"minilatex"`, mirroring only the handful of LaTeX commands one
//! reaches for automatically: `\emph`, `\textbf`, `\textit`, `itemize`,
//! `enumerate` (with `\item` available *inside* the two list environments), plus
//! the typography specials (`~` and the `` `` ``/`''`/`--`/`---` ligatures) that
//! moved here from the seed package. It exists for debugging and prototyping —
//! just enough to exercise the machinery without setup overhead on a first run.
//!
//! The latexlike preset configures a parser so that it can parse *latexlike
//! content*, not LaTeX documents: anything techy shipped would fall short of a
//! package-structured database capable of realistic documents, while frameworks
//! built on techy roll exactly the package structure they want. So this module
//! stays a toy by design; the standard-definitions port belongs to the layers
//! above.
//!
//! **Never preloaded**: activation is always explicit
//! ([`ParsingState::lang_initial_with_packages`](crate::state::ParsingState::lang_initial_with_packages)),
//! and no other latexlike module references this one, so builds that never import
//! it dead-strip it entirely.

use alloc::sync::Arc;
use alloc::vec;

use crate::node::ArgumentExt;
use crate::scopes::{Package, ScopeOp, SpecsProvider};
use crate::spec::CallableSpec;
use crate::state::ParsingStateDelta;

use super::{
    argument_specs, EnvironmentSpec, LatexlikeCallableType, LatexlikeLang, MacroSpec,
    SpecialsSpec,
};

/// The `"minilatex"` package: `\emph`/`\textbf`/`\textit` (one mandatory `"m"`
/// argument each, expression fallback on), the `itemize`/`enumerate` list
/// environments, and the typography specials — the non-breaking tie `~` (every
/// mode) and the ligatures ``` `` ```, `''`, `--`, `---`.
///
/// `\item` (one optional `"o"` argument) is defined **only inside the two list
/// environments**: their body state delta pushes an inner package
/// `"minilatex.item"` onto the scope stack, so `\item` resolves in a list body
/// and nowhere else — the in-tree exemplar of body-scoped definitions.
///
/// The ligatures are visible only in the language's *seed mode* (the
/// document-base mode a parse starts in — [`Mode::Text`](super::Mode::Text) for
/// [`Latexlike`](super::Latexlike)): they carry no math meaning, so inside
/// `$…$` they stay plain characters. The tie `~` stays visible in every mode.
/// The multi-character triggers ride the scope-stack scan's longest-match rule
/// (`---` beats `--`). A language whose seed state data cannot be built
/// ([`Lang::initial_state_data`](crate::state::Lang::initial_state_data)
/// answers `Err`) still gets the package: the ligature restriction then uses
/// the mode type's default value — the same mode
/// [`StateData::empty`](crate::state::StateData::empty) seeds, and
/// [`Mode::Text`](super::Mode::Text) for the shipped preset — while the
/// seeding call site reports the seed failure itself.
///
/// Returns a bare [`Package`] — load it explicitly, e.g.
/// `ParsingState::lang_initial_with_packages([minilatex_package()])`; it is never
/// part of the seed. Generic over the language family (`LLL`,
/// [`LatexlikeLang`]); the bound on the argument ext is the argument-code
/// factory's ([`argument_specs`]).
///
/// ```
/// use techy::core::{Language, ParsingState};
/// use techy::error::Recovery;
/// use techy::latexlike::minidefs::minilatex_package;
/// use techy::latexlike::{Latexlike, LatexlikeDriver};
///
/// let language: Language<Latexlike> = Language::new(
///     LatexlikeDriver::new(Recovery::Strict),
///     ParsingState::lang_initial_with_packages([minilatex_package()]).expect("seed state"),
/// );
/// let result = language.parse(r"\emph{try} it --- now").unwrap();
/// assert_eq!(result.tree.root().child(0).unwrap().macro_name(), Some("emph"));
/// ```
pub fn minilatex_package<LLL: LatexlikeLang>() -> Package<LLL>
where
    ArgumentExt<LLL>: Default,
{
    let mut package = Package::new("minilatex");

    // The three inline styles: one mandatory content-group argument.
    for name in ["emph", "textbf", "textit"] {
        package.insert(
            LLL::CallableTypeId::macro_callable(),
            name,
            MacroSpec::new(argument_specs(["m"]).expect("the literal code list [\"m\"] is valid")),
        );
    }

    // The list environments: no arguments; the body pushes the inner item package —
    // `\item` is a definition of the *body*, not of the document.
    let mut item_package = Package::new("minilatex.item");
    item_package.insert(
        LLL::CallableTypeId::macro_callable(),
        "item",
        MacroSpec::new(argument_specs(["o"]).expect("the literal code list [\"o\"] is valid")),
    );
    let item_package: Arc<dyn SpecsProvider<LLL>> = Arc::new(item_package);
    for name in ["itemize", "enumerate"] {
        package.insert(
            LLL::CallableTypeId::environment_callable(),
            name,
            EnvironmentSpec::new(vec![]).with_body_delta(
                ParsingStateDelta::new()
                    .scope_op(ScopeOp::Push(Arc::clone(&item_package))),
            ),
        );
    }

    // The typography specials (moved from the seed package): zero-argument
    // callables sharing one spec instance (the package flyweight contract).
    let spec: Arc<dyn CallableSpec<LLL>> = Arc::new(SpecialsSpec::new(vec![]));
    package.insert_specials(
        LLL::CallableTypeId::specials_callable(),
        "~",
        Arc::clone(&spec),
    );
    // Ligatures are restricted to the language's seed (document-base) mode — the
    // generic stand-in for "text-only": the mode role trait deliberately has no
    // text-mode constructor, and for `Latexlike` the seed mode is `Mode::Text`.
    // A language whose (fallible) seed data cannot be built here still gets the
    // package; the restriction then uses the mode type's default value — the same
    // value `StateData::empty` seeds, and `Mode::Text` for the shipped preset. The
    // seeding call site surfaces the seed failure itself.
    let base_mode = LLL::initial_state_data().map(|data| data.mode).unwrap_or_default();
    for trigger in ["``", "''", "--", "---"] {
        package.insert_specials_in_modes(
            LLL::CallableTypeId::specials_callable(),
            trigger,
            Arc::clone(&spec),
            Some(vec![base_mode]),
        );
    }

    package
}

#[cfg(test)]
mod tests {
    use super::super::test_support::{root_shapes, with_package};
    use super::super::{check_latexlike_tree_invariants, GroupType, ParagraphBreakSpec};
    use super::*;
    use crate::error::Recovery;
    use alloc::string::String;
    use alloc::vec::Vec;

    /// Strict-parse `input` with minilatex loaded; assert clean and return root
    /// child summaries.
    fn shapes(input: &str) -> Vec<String> {
        let language = with_package(Recovery::Strict, minilatex_package());
        let result = language.parse(input).unwrap();
        check_latexlike_tree_invariants(&result.tree);
        assert!(
            result.diagnostics.is_empty(),
            "unexpected diagnostics: {:?}",
            result.diagnostics
        );
        root_shapes(&result)
    }

    #[test]
    fn minilatex_specials_parse_when_loaded() {
        // The moved seed specials fire under explicit activation; `&` is NOT among
        // them — removed from the shipped definitions entirely, a plain char even
        // with minilatex loaded.
        assert_eq!(
            shapes("a~b & c"),
            ["chars(a)", "Specials(~)", "chars(b & c)"]
        );

        // Negative spec-identity pin: an ordinary specials node is NOT identified
        // as a paragraph break — the ParagraphBreakSpec downcast must fail.
        let language = with_package(Recovery::Strict, minilatex_package());
        let result = language.parse("a~b").unwrap();
        let tilde = result.tree.root().child(1).unwrap();
        assert_eq!(tilde.specials_name(), Some("~"));
        let spec = tilde.spec().expect("a callable node");
        assert!((&**spec as &dyn core::any::Any)
            .downcast_ref::<ParagraphBreakSpec>()
            .is_none());
    }

    #[test]
    fn ligature_specials_take_the_longest_match() {
        assert_eq!(
            shapes("x---y--z"),
            ["chars(x)", "Specials(---)", "chars(y)", "Specials(--)", "chars(z)"]
        );
        assert_eq!(shapes("``q''"), ["Specials(``)", "chars(q)", "Specials('')"]);
        // `` !` `` and `` ?` `` are not among the shipped ligatures (dropped in the
        // July 2026 review): plain characters.
        assert_eq!(shapes("!`Si?`"), ["chars(!`Si?`)"]);
    }

    #[test]
    fn ligature_specials_are_text_only() {
        // Inside math the seed-mode-only typography ligatures stay plain chars,
        // while the every-mode tie `~` still fires (per-entry mode visibility).
        let language = with_package(Recovery::Strict, minilatex_package());
        let result = language.parse("$a~b---c$").unwrap();
        check_latexlike_tree_invariants(&result.tree);
        let math = result.tree.root().child(0).unwrap();
        assert!(matches!(math.group_type(), Some(GroupType::Math(_))));
        let interior: Vec<String> =
            math.children().iter().map(|node| node.summary()).collect();
        assert_eq!(interior, ["chars(a)", "Specials(~)", "chars(b---c)"]);

        // In the seed (text) mode the same ligature fires.
        assert_eq!(shapes("a---b"), ["chars(a)", "Specials(---)", "chars(b)"]);
    }

    #[test]
    fn trigger_chars_without_a_match_stay_plain() {
        // `'` and `` ` `` are trigger *first characters*, but alone (no second
        // backtick / quote) the scan declines and they remain chars.
        assert_eq!(shapes("a!b?c'd`e"), ["chars(a!b?c'd`e)"]);
    }

    #[test]
    fn the_inline_styles_take_one_mandatory_argument() {
        for (name, input) in [
            ("emph", r"\emph{x}"),
            ("textbf", r"\textbf{x}"),
            ("textit", r"\textit{x}"),
        ] {
            let language = with_package(Recovery::Strict, minilatex_package());
            let result = language.parse(input).unwrap();
            check_latexlike_tree_invariants(&result.tree);
            let node = result.tree.root().child(0).unwrap();
            assert_eq!(node.macro_name(), Some(name));
            let content: Vec<_> =
                node.argument_content_nodes(0).unwrap().iter().collect();
            assert_eq!(content[0].chars(), Some("x"));
        }

        // The `m` code keeps its expression fallback: `\emph x` takes the `x`.
        let language = with_package(Recovery::Strict, minilatex_package());
        let result = language.parse(r"\emph x").unwrap();
        let node = result.tree.root().child(0).unwrap();
        let content: Vec<_> = node.argument_content_nodes(0).unwrap().iter().collect();
        assert_eq!(content[0].chars(), Some("x"));
    }

    #[test]
    fn item_is_defined_only_inside_list_bodies() {
        // The body-scoped-definitions exemplar: `\item` (with its optional `[…]`
        // argument) resolves inside `itemize`/`enumerate` bodies…
        for env in ["itemize", "enumerate"] {
            let language = with_package(Recovery::Strict, minilatex_package());
            let input = alloc::format!("\\begin{{{env}}}\\item[a] x\\item y\\end{{{env}}}");
            let result = language.parse(&input).unwrap();
            check_latexlike_tree_invariants(&result.tree);
            let body: Vec<String> = result.tree.root().child(0).unwrap().body().unwrap()
                .iter().map(|node| node.summary()).collect();
            assert_eq!(
                body,
                ["Macro(item)", "chars( x)", "Macro(item)", "chars(y)"],
                "in {env}"
            );
        }

        // …and nowhere else: outside a list body the name does not resolve.
        let language = with_package(Recovery::Strict, minilatex_package());
        let err = language.parse(r"\item x").unwrap_err();
        assert!(err.to_string().contains("cannot resolve command ‘\\item’"), "{err}");
    }
}
