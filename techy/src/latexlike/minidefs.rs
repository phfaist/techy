//! A toy package of familiar LaTeX definitions, for demonstrations and prototyping —
//! deliberately not a definitions database.
//!
//! [`minilatex_package`] builds the package, `"minilatex"`. It defines the handful of
//! commands one reaches for automatically and nothing more: `\emph`, `\textbf`,
//! `\textit`, the `itemize` and `enumerate` list environments, and the typography
//! specials — the tie `~` and the ligatures ``` `` ```, `''`, `--`, `---`. `\item` comes
//! with the list environments, in the nested package [`minilatex_item_package`] builds,
//! which they push for their bodies. That is enough to exercise the machinery without
//! setup, which is why the examples throughout this documentation load it.
//!
//! **Where real definitions come from.** The preset configures a parser for latexlike
//! *syntax*, not for LaTeX documents: a realistic document needs a package-structured
//! database of definitions, which techy does not ship. Registering what a document needs
//! is the program's own work — the guide chapter
//! [Defining macros, environments, and specials](crate::guide::specs) walks through it —
//! and frameworks built on techy supply the package structure they want.
//!
//! **Never preloaded**: loading is always explicit
//! ([`ParsingState::lang_initial_with_packages`](crate::core::ParsingState::lang_initial_with_packages)),
//! and no other latexlike module refers to this one, so a build that never imports it
//! leaves it out entirely. The packages' serialization recipes are registered from here
//! too ([`register_package_recipes`]).

use alloc::sync::Arc;
use alloc::vec;

use crate::node::ArgumentExt;
use crate::scopes::{Package, ScopeOp, SpecsProvider};
use crate::serialize::KnownProviders;
use crate::spec::CallableSpec;
use crate::state::ParsingStateDelta;

use super::{EnvironmentSpec, LatexlikeCallableType, LatexlikeLang, SpecialsSpec};

/// Creates the `"minilatex"` package: a toy set of familiar LaTeX definitions, for
/// demonstrations and prototyping.
///
/// It defines `\emph`, `\textbf` and `\textit` (one mandatory `"m"` argument each,
/// expression fallback on), the `itemize` and `enumerate` list environments, and the
/// typography specials — the non-breaking tie `~` (visible in every mode) and the
/// ligatures ``` `` ```, `''`, `--`, `---`.
///
/// `\item` (one optional `"o"` argument) is defined **only inside the two list
/// environments**: their body state delta pushes an inner package `"minilatex.item"` onto
/// the scope stack, so `\item` resolves in a list body and nowhere else. It is this
/// crate's worked example of body-scoped definitions.
///
/// The ligatures are visible only in the language's *seed mode* — the document-base mode
/// a parse starts in, [`Mode::Text`](super::Mode::Text) for
/// [`Latexlike`](super::Latexlike) — because they carry no math meaning: inside `$…$` they
/// stay plain characters. The tie `~` stays visible in every mode. Among the
/// multi-character triggers the longest match wins, so `---` beats `--`.
///
/// Load the returned package explicitly, for instance
/// `ParsingState::lang_initial_with_packages([minilatex_package()])`; it is never part of
/// the seed state. It is a shared [`Package`] (built with [`Package::new_shared`], its
/// specs stamped with their provenance so that they serialize by identity), and the nested
/// item package comes from [`minilatex_item_package`] — a fresh one per call, which this
/// function nests; a reading environment resolving a serialized `minilatex.item` builds
/// its own (see [`register_package_recipes`]).
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
///
/// Generic over the language family (`LLL`, [`LatexlikeLang`]); the bound on the argument
/// ext is the argument-code factory's ([`argument_specs`](super::argument_specs)). A
/// language whose seed state data cannot be built
/// ([`Lang::initial_state_data`](crate::core::Lang::initial_state_data) answers `Err`)
/// still gets the package: the ligature restriction then uses the mode type's default
/// value, and the seeding call site reports the seed failure itself.
pub fn minilatex_package<LLL: LatexlikeLang>() -> Arc<Package<LLL>>
where
    ArgumentExt<LLL>: Default,
{
    Package::new_shared("minilatex", |package| {
        // The three inline styles: one mandatory content-group argument.
        for name in ["emph", "textbf", "textit"] {
            package.define_macro(name, ["m"]).expect("the literal code list [\"m\"] is valid");
        }

        // The list environments: no arguments; the body pushes the inner item
        // package — `\item` is a definition of the *body*, not of the document.
        let item_package: Arc<dyn SpecsProvider<LLL>> = minilatex_item_package();
        let environment_type = LLL::CallableTypeId::environment_callable();
        for name in ["itemize", "enumerate"] {
            let mut spec = EnvironmentSpec::new(vec![]).with_body_delta(
                ParsingStateDelta::new().scope_op(ScopeOp::Push(Arc::clone(&item_package))),
            );
            if let Some(provenance) = package.provenance_for(environment_type, name) {
                spec = spec.with_provenance(provenance);
            }
            package.insert(environment_type, name, spec);
        }

        // The typography specials (moved from the seed package): zero-argument
        // callables sharing one spec instance (the package flyweight contract). The
        // one instance carries one stamp — the tie's — which names one of its
        // definitions: an identity reference resolves to the instance, whichever
        // trigger it was reached through.
        let specials_type = LLL::CallableTypeId::specials_callable();
        let mut spec = SpecialsSpec::<LLL>::new(vec![]);
        if let Some(provenance) = package.provenance_for_specials(specials_type, "~") {
            spec = spec.with_provenance(provenance);
        }
        let spec: Arc<dyn CallableSpec<LLL>> = Arc::new(spec);
        package.insert_specials(specials_type, "~", Arc::clone(&spec));
        // Ligatures are restricted to the language's seed (document-base) mode — the
        // generic stand-in for "text-only": the mode role trait deliberately has no
        // text-mode constructor, and for `Latexlike` the seed mode is `Mode::Text`.
        // A language whose (fallible) seed data cannot be built here still gets the
        // package; the restriction then uses the mode type's default value — the
        // same value `StateData::empty` seeds, and `Mode::Text` for the shipped
        // preset. The seeding call site surfaces the seed failure itself.
        let base_mode = LLL::initial_state_data().map(|data| data.mode).unwrap_or_default();
        for trigger in ["``", "''", "--", "---"] {
            package.insert_specials_in_modes(specials_type, trigger, Arc::clone(&spec), Some(vec![base_mode]));
        }
    })
}

/// Creates the `"minilatex.item"` package: `\item` (one optional `"o"` argument).
///
/// This is the definition the two list environments of [`minilatex_package`] push onto
/// the scope stack for their bodies. Shared and stamped like its parent; every call builds
/// a fresh package, and [`minilatex_package`] nests one of its own.
pub fn minilatex_item_package<LLL: LatexlikeLang>() -> Arc<Package<LLL>>
where
    ArgumentExt<LLL>: Default,
{
    Package::new_shared("minilatex.item", |package| {
        package.define_macro("item", ["o"]).expect("the literal code list [\"o\"] is valid");
    })
}

/// Registers the recipes of this module's packages — `minilatex` and `minilatex.item` —
/// on `known`, so that a reading session resolves serialized references to them by
/// building them.
///
/// A recipe is [`KnownProviders`]'s fallback: a provider inserted under the same name
/// takes precedence. This is the counterpart of
/// [`serialize::register_package_recipes`](super::serialize::register_package_recipes) for
/// the toy packages. It is defined in this module so that a build which never uses
/// `minidefs` does not pull the toy packages in.
///
/// The two package names are part of the preset's serialized vocabulary and are kept
/// stable like identifiers: serialized data refers to its package by name, so `minilatex`
/// and `minilatex.item` stay as they are (like the seed package's `_builtin`).
pub fn register_package_recipes<LLL: LatexlikeLang>(known: &mut KnownProviders<LLL>)
where
    ArgumentExt<LLL>: Default,
{
    known.register_recipe("minilatex", minilatex_package::<LLL>);
    known.register_recipe("minilatex.item", minilatex_item_package::<LLL>);
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
