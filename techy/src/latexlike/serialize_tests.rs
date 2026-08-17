//! Tests of the preset's serialization (M5): real latexlike parses through the tree
//! round-trip harness over a corpus (macros, environments, minilatex lists with the
//! body-pushed item package, specials, math, comments, verbatim, paragraph breaks in
//! both styles, `\input` across sources), the identity and self-contained spec forms,
//! the reading environment (held providers and recipes), the instance-not-lookup
//! guarantee (D18), scopes and fallback providers, the failure surface (unstamped
//! specs, dropped providers, unregistered identifiers, missing providers), a hostile
//! state, determinism, and — under the `serde` feature — the vocabulary parity between
//! the value conversions and the serde derives plus a pinned JSON rendering.

use alloc::string::{String, ToString};
use alloc::sync::Arc;
use alloc::vec;
use alloc::vec::Vec;
use core::any::Any;
use core::sync::atomic::{AtomicUsize, Ordering};

use crate::engine::{Language, ParseResult, StdDescentGuardInit};
use crate::error::Recovery;
use crate::node::NodeTree;
use crate::scopes::{
    CallableQuery, DefinitionKey, ErrorCallableSpec, FallbackProvider, Package, ProviderError,
    Scope, ScopeOp, SpecProvenance, SpecsProvider,
};
use crate::serialize::tree_support::{assert_trees_equivalent, ignore_annotations, round_trip_tree};
use crate::serialize::{
    DeserializableObject, DeserializeContext, DeserializeError, DeserializedCondition, KnownProviders,
    ParseResultSerialization, SerdeSession, Segment, SerialEntry, SerialIndex, SerialValue,
    SerializableObject, SerializeContext, SerializeError, StandardTableInterning,
    StandardTableReading, TreeSerialization,
};
use crate::source::MapResolver;
use crate::spec::{CallableSpec, StdCallableSpec};
use crate::state::{ParsingState, ParsingStateDelta};

use super::minidefs::minilatex_package;
use super::serialize::{register, register_package_recipes};
use super::{
    argument_specs, builtin_package, input_macro_spec, BeginSpec, BodyMarker, CallableType,
    EndSpec, EnvironmentSpec, InputMacroSpec, Latexlike, LatexlikeDriver, MacroSpec, Mode,
    ParagraphBreakSpec, ParagraphBreakStyle, SpecialsSpec, VerbatimBehavior,
};

// --- fixtures ---------------------------------------------------------------------------

/// The oracle vocabulary as a shared, stamped package: `\emph{m}`, `\frac{m}{m}`,
/// `\o{[}{{}`, `\st{s}{m}`, `\verb{v}`, `\text` (a text-mode argument through a
/// hand-built `StdCallableSpec`), environments `fig([)`/`A`/`B` + `verbatim`, and the
/// opt-in `\input` (stamped: identity).
fn oracle_package() -> Arc<Package<Latexlike>> {
    Package::new_shared("oracle", |package| {
        package.define_macro("emph", ["m"]).unwrap();
        package.define_macro("frac", ["m", "m"]).unwrap();
        package.define_macro("o", ["[", "{"]).unwrap();
        package.define_macro("st", ["s", "m"]).unwrap();
        package.define_macro("verb", ["v"]).unwrap();
        package.define_environment("fig", ["["]).unwrap();
        package.define_environment("A", Vec::<&str>::new()).unwrap();
        package.define_environment("B", Vec::<&str>::new()).unwrap();
        let verbatim = EnvironmentSpec::from_behavior(Arc::new(VerbatimBehavior::default()))
            .with_provenance(package.provenance_for(CallableType::Environment, "verbatim").unwrap());
        package.insert(CallableType::Environment, "verbatim", verbatim);
        let input = input_macro_spec(false, BodyMarker::not_body())
            .with_provenance(package.provenance_for(CallableType::Macro, "input").unwrap());
        package.insert(CallableType::Macro, "input", input);
        // A generic core spec in a latexlike package: `\plain{m}`.
        let plain = StdCallableSpec::<Latexlike> {
            arguments: argument_specs(["m"]).unwrap(),
            ..Default::default()
        }
        .with_provenance(package.provenance_for(CallableType::Macro, "plain").unwrap());
        package.insert(CallableType::Macro, "plain", plain);
    })
}

/// A language over the seed plus `packages` (outermost first), under `recovery`.
fn language(
    recovery: Recovery,
    packages: impl IntoIterator<Item = Arc<dyn SpecsProvider<Latexlike>>>,
) -> Language<Latexlike> {
    Language::new(
        LatexlikeDriver::new(recovery),
        ParsingState::lang_initial_with_packages(packages).expect("seed state"),
    )
}

/// The oracle language: minilatex (specials, lists) plus the oracle package.
fn oracle_language(recovery: Recovery) -> Language<Latexlike> {
    language(recovery, [minilatex_package() as Arc<dyn SpecsProvider<Latexlike>>, oracle_package()])
}

/// The providers a `Language`'s seed state holds, outermost first — what a reading
/// environment inserts to resolve the very instances the parse used.
fn seed_providers(language: &Language<Latexlike>) -> Vec<Arc<dyn SpecsProvider<Latexlike>>> {
    language.initial_state().scopes().providers().to_vec()
}

/// A session factory for the round-trip harness: the standard tables, the readers,
/// and a reading environment holding `providers` (by identity) plus the preset's
/// recipes (`_builtin`, `minilatex`, `minilatex.item`) as fallbacks.
fn session_factory(
    providers: Vec<Arc<dyn SpecsProvider<Latexlike>>>,
) -> impl Fn() -> SerdeSession<Latexlike> {
    move || {
        let mut session = SerdeSession::<Latexlike>::new();
        let mut known = KnownProviders::<Latexlike>::new();
        for provider in &providers {
            known.insert(Arc::clone(provider));
        }
        register_package_recipes(&mut known);
        super::minidefs::register_package_recipes(&mut known);
        session.set_user_data(known);
        register(&mut session).unwrap();
        session
    }
}

/// A session factory whose reading environment holds no provider — everything is
/// resolved through the preset's recipes.
fn recipe_only_factory() -> impl Fn() -> SerdeSession<Latexlike> {
    session_factory(Vec::new())
}

fn parse(language: &Language<Latexlike>, input: &str) -> ParseResult<Latexlike> {
    language.parse(input).expect("the corpus parses")
}

/// Parse `input` strictly with the oracle language, round-trip the tree through
/// sessions whose environment holds the seed's providers, and return the rebuilt tree.
fn round_trip_oracle(input: &str) -> (ParseResult<Latexlike>, NodeTree<Latexlike, ()>) {
    let language = oracle_language(Recovery::Strict);
    let result = parse(&language, input);
    assert!(result.diagnostics.is_empty(), "{input:?}: {:?}", result.diagnostics);
    let back = round_trip_tree(session_factory(seed_providers(&language)), &result.tree, ignore_annotations);
    (result, back)
}

fn assert_round_trips(inputs: &[&str]) {
    for input in inputs {
        round_trip_oracle(input);
    }
}

// --- the corpus -------------------------------------------------------------------------

#[test]
fn macros_with_and_without_post_space_round_trip() {
    assert_round_trips(&["\\emph  x", "\\emph{x}", "pre \\emph x post", "\\emph {spaced}"]);
}

#[test]
fn argument_shapes_round_trip() {
    assert_round_trips(&[
        "\\frac 1 2",
        "\\frac{a}{b}",
        "\\o[x]{y}",
        "\\o{y} after",
        "\\o [x] {y}",
        "\\st*{x}",
        "\\st y",
        "\\plain{core spec}",
    ]);
}

#[test]
fn environments_round_trip() {
    assert_round_trips(&[
        "\\begin{A}x\\end{A}",
        "\\begin {A}x y\\end {A}",
        "\\begin{A}a\\begin{B}b\\end{B}c\\end{A}",
        "\\begin{fig}[t]body\\end{fig}",
        "\\begin{fig}body\\end{fig}",
    ]);
}

#[test]
fn minilatex_lists_round_trip_with_the_body_pushed_item_package() {
    // `\item` resolves in the item package the body delta pushes: the body's states
    // carry a scope stack with `minilatex.item` on top, and the `\item` specs are
    // identity references into it.
    let (result, back) = round_trip_oracle("\\begin{itemize}\\item one \\item[x] two\\end{itemize}");
    let items: Vec<_> = back
        .root()
        .child(0)
        .unwrap()
        .body()
        .expect("the body slot")
        .iter()
        .filter(|node| node.macro_name() == Some("item"))
        .collect();
    assert_eq!(items.len(), 2);
    // Both `\item` nodes share one spec, before and after.
    let original_items: Vec<_> = result
        .tree
        .root()
        .child(0)
        .unwrap()
        .body()
        .unwrap()
        .iter()
        .filter(|node| node.macro_name() == Some("item"))
        .collect();
    assert!(Arc::ptr_eq(original_items[0].spec().unwrap(), original_items[1].spec().unwrap()));
    assert!(Arc::ptr_eq(items[0].spec().unwrap(), items[1].spec().unwrap()));
    // The item body's state has the item package innermost (`provider_names` lists
    // innermost first).
    let names: Vec<&str> = items[0].parsing_state().scopes().provider_names().collect();
    assert_eq!(names.first().copied(), Some("minilatex.item"));
}

#[test]
fn specials_round_trip() {
    assert_round_trips(&["a---b ~ c", "``quoted'' -- dashed"]);
}

#[test]
fn groups_math_and_comments_round_trip() {
    assert_round_trips(&[
        "{nested {groups} here}",
        "inline $x+y$ math",
        "$$display$$ and \\(i\\) and \\[d\\]",
        "$m$ {g {h}} \\[x^2\\]",
        "a% note\n  b",
        "a% end",
        "% leading\nrest",
    ]);
}

#[test]
fn verbatim_round_trips() {
    assert_round_trips(&["\\begin{verbatim}\na % b {\n\\end{verbatim}", "\\verb|a b|", "\\verb+{x}+ y"]);
}

#[test]
fn paragraph_breaks_round_trip_in_both_styles() {
    assert_round_trips(&["a\n\nb", "a\n \t\n  b"]);

    // Specials style: the break node's spec is the canonical `ParagraphBreakSpec`,
    // serialized in its self-contained form and rebuilt as the unit value.
    let language = Language::new(
        LatexlikeDriver::new(Recovery::Strict).with_paragraph_break_style(ParagraphBreakStyle::Specials),
        ParsingState::lang_initial_with_packages([oracle_package()]).expect("seed state"),
    );
    let result = parse(&language, "one\n\ntwo\n\n\nthree");
    let back = round_trip_tree(session_factory(seed_providers(&language)), &result.tree, ignore_annotations);
    let breaks: Vec<_> = back
        .root()
        .children()
        .iter()
        .filter(|node| node.spec().is_some_and(|spec| (&**spec as &dyn Any).downcast_ref::<ParagraphBreakSpec>().is_some()))
        .collect();
    assert_eq!(breaks.len(), 2);
    // The break function mints one `Arc<ParagraphBreakSpec>` per break (type identity,
    // not instance identity, is the contract): two entries, two rebuilt instances —
    // the same sharing topology as the original (the harness checked it).
    assert!(!Arc::ptr_eq(breaks[0].spec().unwrap(), breaks[1].spec().unwrap()));
}

#[test]
fn the_kitchen_sink_round_trips() {
    round_trip_oracle(concat!(
        "Intro with \\emph  {styled} text and \\frac 1 2 math $a+b$.\n",
        "% a comment\n",
        "\\begin {itemize}\n",
        "\\item one ~ two --- three\n",
        "\\o[opt]{arg}\n",
        "\\begin{verbatim}\nraw % {\n\\end{verbatim}\n",
        "\\end {itemize}\n",
        "\n",
        "New paragraph \\[d^2\\] and \\verb|v {|.",
    ));
}

#[test]
fn tolerant_recoveries_round_trip() {
    let language = oracle_language(Recovery::Tolerant);
    for input in [
        "\\begin{A}x",              // unterminated environment: empty end side
        "\\begin{A}x\\end{B}",      // terminator mismatch
        "{unclosed",                // unclosed group
        "stray }",                  // stray close
        "\\end{A} orphan",          // orphan end (EndSpec by identity through the seed's builtin)
        "\\unknown x",              // unknown macro
        "$a $ b$",                  // forbidden char in math
    ] {
        let result = parse(&language, input);
        assert!(!result.diagnostics.is_empty(), "{input:?} recovers");
        round_trip_tree(session_factory(seed_providers(&language)), &result.tree, ignore_annotations);
    }
}

#[test]
fn input_across_sources_round_trips() {
    let mut resolver = MapResolver::new();
    resolver.insert("sub.tex", "SUB {and} \\emph{more}");
    resolver.insert("empty.tex", "");
    let language = Language::new(
        LatexlikeDriver::new(Recovery::Strict).with_source_resolver(resolver.with_reference_as_origin()),
        ParsingState::lang_initial_with_packages([oracle_package()]).expect("seed state"),
    )
    .with_descent_guard_init(StdDescentGuardInit::depth_limit(64));
    for input in ["a\\input{sub.tex} b", "x \\input{empty.tex} y", "\\input{sub.tex}\\input{sub.tex}"] {
        let result = parse(&language, input);
        assert!(result.diagnostics.is_empty(), "{input:?}: {:?}", result.diagnostics);
        let attached = result.tree.root().children().iter().filter(|node| node.macro_name() == Some("input")).count();
        assert!(attached >= 1);
        // The harness compares the single-source flags and every span's source text.
        round_trip_tree(session_factory(seed_providers(&language)), &result.tree, ignore_annotations);
    }
}

// --- identity, recipes, and the reading environment -----------------------------------

#[test]
fn stamped_specs_read_back_as_the_reading_environments_instances() {
    // Identity: the reading environment holds the very package the parse used, so
    // the rebuilt tree's specs are pointer-equal to that package's Arcs.
    let oracle = oracle_package();
    let language = language(Recovery::Strict, [Arc::clone(&oracle) as Arc<dyn SpecsProvider<Latexlike>>]);
    let result = parse(&language, "\\emph{a} \\begin{fig}[t]b\\end{fig}");
    let back = round_trip_tree(session_factory(seed_providers(&language)), &result.tree, ignore_annotations);
    let emph = back.root().child(0).unwrap();
    assert!(Arc::ptr_eq(emph.spec().unwrap(), oracle.get(CallableType::Macro, "emph").unwrap()));
    let fig = back.root().child(2).unwrap();
    assert!(Arc::ptr_eq(fig.spec().unwrap(), oracle.get(CallableType::Environment, "fig").unwrap()));
    // The `\begin` in the fig node's parse resolved through the seed's builtin: the
    // environment node's spec is the fig spec, but the states' scope stack holds the
    // builtin — the very Arc of the reading environment.
    let seed_builtin = &seed_providers(&language)[0];
    let state_builtin = &fig.parsing_state().scopes().providers()[0];
    assert!(Arc::ptr_eq(seed_builtin, state_builtin));
}

#[test]
fn recipes_build_the_providers_the_environment_does_not_hold() {
    // No provider held: `_builtin`, `minilatex`, and `minilatex.item` are built by
    // their recipes, once per entry — every reference to an entry shares the built
    // Arc, and the specs resolve in the built packages.
    let language = language(Recovery::Strict, [minilatex_package() as Arc<dyn SpecsProvider<Latexlike>>]);
    let result = parse(&language, "\\emph{a} \\begin{itemize}\\item x\\end{itemize} b~c");
    let back = round_trip_tree(recipe_only_factory(), &result.tree, ignore_annotations);
    let emph = back.root().child(0).unwrap();
    let emph_spec = emph.spec().unwrap();
    // Not the writer's instance (a fresh package was built)…
    let writer_emph = result.tree.root().child(0).unwrap();
    assert!(!Arc::ptr_eq(emph_spec, writer_emph.spec().unwrap()));
    // …but the instance the rebuilt state's minilatex package holds.
    let minilatex = emph.parsing_state().scopes().providers().iter().find(|p| p.name() == "minilatex").unwrap();
    let minilatex = (&**minilatex as &dyn Any).downcast_ref::<Package<Latexlike>>().unwrap();
    assert!(Arc::ptr_eq(emph_spec, minilatex.get(CallableType::Macro, "emph").unwrap()));
    // The tie: a specials definition, resolved by trigger.
    let tie = back.root().children().iter().find(|node| node.specials_name() == Some("~")).unwrap();
    assert!(Arc::ptr_eq(tie.spec().unwrap(), minilatex.get_specials(CallableType::Specials, "~").unwrap()));
}

#[test]
fn a_held_provider_takes_precedence_over_its_recipe() {
    let minilatex = minilatex_package();
    let language = language(Recovery::Strict, [Arc::clone(&minilatex) as Arc<dyn SpecsProvider<Latexlike>>]);
    let result = parse(&language, "\\textbf{a}");
    // The factory holds the seed's providers AND registers the recipes: the held
    // instance wins.
    let back = round_trip_tree(session_factory(seed_providers(&language)), &result.tree, ignore_annotations);
    let textbf = back.root().child(0).unwrap();
    assert!(Arc::ptr_eq(textbf.spec().unwrap(), minilatex.get(CallableType::Macro, "textbf").unwrap()));
}

#[test]
fn a_recipe_is_built_once_per_entry_and_shared_by_every_reference() {
    // Two trees in one stream refer to the same minilatex entry: one build.
    let language = language(Recovery::Strict, [minilatex_package() as Arc<dyn SpecsProvider<Latexlike>>]);
    let first = parse(&language, "\\emph{a}");
    let second = parse(&language, "\\textit{b}");
    let factory = recipe_only_factory();
    let mut writer = factory();
    let p1 = writer.serialize_tree(&first.tree).unwrap();
    let p2 = writer.serialize_tree(&second.tree).unwrap();
    let segment = writer.take_segment();
    let mut reader = factory();
    reader.push_segment(segment).unwrap();
    let trees = reader.standard_tables().unwrap().trees;
    let t1: NodeTree<Latexlike, ()> = reader.tree(trees.position(p1.index())).unwrap();
    let t2: NodeTree<Latexlike, ()> = reader.tree(trees.position(p2.index())).unwrap();
    let m1 = t1.root().child(0).unwrap().parsing_state().scopes().providers().iter().find(|p| p.name() == "minilatex").cloned().unwrap();
    let m2 = t2.root().child(0).unwrap().parsing_state().scopes().providers().iter().find(|p| p.name() == "minilatex").cloned().unwrap();
    assert!(Arc::ptr_eq(&m1, &m2));
}

#[test]
fn a_missing_provider_is_a_typed_error_naming_it() {
    let language = language(Recovery::Strict, [minilatex_package() as Arc<dyn SpecsProvider<Latexlike>>]);
    let result = parse(&language, "\\emph{a}");
    let mut writer = SerdeSession::<Latexlike>::new();
    writer.serialize_tree(&result.tree).unwrap();
    let segment = writer.take_segment();

    // An environment without minilatex (and without its recipe).
    let mut reader = SerdeSession::<Latexlike>::new();
    let mut known = KnownProviders::<Latexlike>::new();
    register_package_recipes(&mut known);
    reader.set_user_data(known);
    register(&mut reader).unwrap();
    let error = reader.push_segment(segment.clone()).unwrap_err();
    assert!(matches!(innermost(&error), DeserializeError::MissingProvider { name } if name == "minilatex"), "{error}");

    // No environment at all: the same error.
    let mut bare = SerdeSession::<Latexlike>::new();
    register(&mut bare).unwrap();
    let error = bare.push_segment(segment).unwrap_err();
    assert!(matches!(innermost(&error), DeserializeError::MissingProvider { .. }), "{error}");
}

#[test]
fn a_definition_missing_from_the_environments_package_is_a_typed_error() {
    // The writer's `oracle` defines `\emph`; the reader's `oracle` does not.
    let language = language(Recovery::Strict, [oracle_package() as Arc<dyn SpecsProvider<Latexlike>>]);
    let result = parse(&language, "\\emph{a}");
    let mut writer = SerdeSession::<Latexlike>::new();
    writer.serialize_tree(&result.tree).unwrap();
    let segment = writer.take_segment();

    let other_oracle = Package::<Latexlike>::new_shared("oracle", |package| {
        package.define_macro("frac", ["m", "m"]).unwrap();
    });
    let factory = session_factory(vec![other_oracle]);
    let mut reader = factory();
    let error = reader.push_segment(segment).unwrap_err();
    match innermost(&error) {
        DeserializeError::MissingDefinition { provider, callable_type, key } => {
            assert_eq!(provider, "oracle");
            assert_eq!(callable_type, "Macro");
            assert_eq!(*key, DefinitionKey::Name("emph".into()));
        }
        other => panic!("unexpected: {other}"),
    }
}

#[test]
fn an_unregistered_identifier_is_a_typed_error() {
    let language = oracle_language(Recovery::Strict);
    let result = parse(&language, "\\emph{a}");
    let mut writer = SerdeSession::<Latexlike>::new();
    writer.serialize_tree(&result.tree).unwrap();
    let segment = writer.take_segment();
    // A session without `latexlike::serialize::register`: nothing reads specs.
    let mut reader = SerdeSession::<Latexlike>::new();
    let error = reader.push_segment(segment).unwrap_err();
    assert!(
        matches!(innermost(&error), DeserializeError::UnknownIdentifier { table, .. } if *table == "specs" || *table == "providers"),
        "{error}"
    );
}

#[test]
fn an_unstamped_spec_of_an_identity_only_type_is_a_write_error() {
    // Built with `Package::new`: no stamps.
    let mut package = Package::<Latexlike>::new("plain");
    package.insert(CallableType::Macro, "emph", MacroSpec::new(argument_specs(["m"]).unwrap()));
    let language = language(Recovery::Strict, [Arc::new(package) as Arc<dyn SpecsProvider<Latexlike>>]);
    let result = parse(&language, "\\emph{a}");
    let mut writer = SerdeSession::<Latexlike>::new();
    let error = writer.serialize_tree(&result.tree).unwrap_err();
    assert!(
        matches!(innermost_write(&error), SerializeError::MissingProvenance { spec: "MacroSpec" }),
        "{error}"
    );
    // The same for the other identity-only types, interned directly.
    let mut session = SerdeSession::<Latexlike>::new();
    let unstamped: [Arc<dyn CallableSpec<Latexlike>>; 3] = [
        Arc::new(SpecialsSpec::<Latexlike>::default()),
        Arc::new(EnvironmentSpec::<Latexlike>::new(vec![])),
        Arc::new(StdCallableSpec::<Latexlike>::default()),
    ];
    let expected = ["SpecialsSpec", "EnvironmentSpec", "StdCallableSpec"];
    for (spec, name) in unstamped.iter().zip(expected) {
        let error = session.intern_spec(spec).unwrap_err();
        assert!(matches!(innermost_write(&error), SerializeError::MissingProvenance { spec } if *spec == name), "{error}");
    }
}

#[test]
fn a_dropped_provider_is_a_write_error_naming_the_spec() {
    let spec: Arc<dyn CallableSpec<Latexlike>> = {
        let package = Package::<Latexlike>::new_shared("ephemeral", |package| {
            package.define_macro("gone", ["m"]).unwrap();
        });
        Arc::clone(package.get(CallableType::Macro, "gone").unwrap())
        // `package` is dropped here: the spec's stamp now points at nothing.
    };
    let mut session = SerdeSession::<Latexlike>::new();
    let error = session.intern_spec(&spec).unwrap_err();
    match innermost_write(&error) {
        SerializeError::ProviderDropped { callable_type, key } => {
            assert_eq!(callable_type, "Macro");
            assert_eq!(*key, DefinitionKey::Name("gone".into()));
        }
        other => panic!("unexpected: {other}"),
    }
}

#[test]
fn self_contained_specs_round_trip_without_a_stamp() {
    // Unstamped `BeginSpec` (custom pair `\open`/`\shut`), `EndSpec`, and an unstamped
    // `\input` spec: their self-contained forms, rebuilt fresh.
    let mut package = Package::<Latexlike>::new("plain");
    package.insert(CallableType::Macro, "input", input_macro_spec(true, BodyMarker::not_body()));
    let package = Arc::new(package);
    let mut resolver = MapResolver::new();
    resolver.insert("sub.tex", "in");
    let language = Language::new(
        LatexlikeDriver::new(Recovery::Strict).with_source_resolver(resolver.with_reference_as_origin()),
        ParsingState::lang_initial_with_packages([Arc::clone(&package) as Arc<dyn SpecsProvider<Latexlike>>])
            .expect("seed state"),
    );
    let result = parse(&language, "a \\input{sub.tex}");
    let back = round_trip_tree(session_factory(seed_providers(&language)), &result.tree, ignore_annotations);
    let input = back.root().child(1).unwrap();
    let input_spec = (&**input.spec().unwrap() as &dyn Any).downcast_ref::<InputMacroSpec<Latexlike>>().unwrap();
    assert!(input_spec.persist_state());
    assert!(input_spec.provenance().is_none());
    // A fresh instance, not the package's.
    assert!(!Arc::ptr_eq(input.spec().unwrap(), package.get(CallableType::Macro, "input").unwrap()));

    // The `BeginSpec` and `EndSpec` self-contained forms, interned directly.
    let mut writer = SerdeSession::<Latexlike>::new();
    let begin: Arc<dyn CallableSpec<Latexlike>> = Arc::new(BeginSpec::<Latexlike>::new("shut"));
    let end: Arc<dyn CallableSpec<Latexlike>> = Arc::new(EndSpec::<Latexlike>::new());
    let begin_position = writer.intern_spec(&begin).unwrap();
    let end_position = writer.intern_spec(&end).unwrap();
    let mut reader = SerdeSession::<Latexlike>::new();
    register(&mut reader).unwrap();
    reader.push_segment(writer.take_segment()).unwrap();
    let specs = reader.standard_tables().unwrap().specs;
    let back = reader.spec(specs.position(begin_position.index())).unwrap();
    let back = (&*back as &dyn Any).downcast_ref::<BeginSpec<Latexlike>>().unwrap();
    assert_eq!(back.end_command_name(), "shut");
    assert!(back.provenance().is_none());
    let back = reader.spec(specs.position(end_position.index())).unwrap();
    assert!((&*back as &dyn Any).downcast_ref::<EndSpec<Latexlike>>().is_some());
}

#[test]
fn a_stamped_begin_spec_prefers_identity() {
    // The builtin `\begin` is stamped: reading yields the environment's builtin
    // package's instance, not a fresh `BeginSpec`.
    let language = oracle_language(Recovery::Strict);
    let builtin = seed_providers(&language)[0].clone();
    let begin = builtin.clone();
    let begin = (&*begin as &dyn Any).downcast_ref::<Package<Latexlike>>().unwrap();
    let begin_spec = Arc::clone(begin.get(CallableType::Macro, "begin").unwrap());
    let factory = session_factory(seed_providers(&language));
    let mut writer = factory();
    let position = writer.intern_spec(&begin_spec).unwrap();
    let mut reader = factory();
    reader.push_segment(writer.take_segment()).unwrap();
    let specs = reader.standard_tables().unwrap().specs;
    let back = reader.spec(specs.position(position.index())).unwrap();
    assert!(Arc::ptr_eq(&back, &begin_spec));
}

// --- D18: instance, not lookup ------------------------------------------------------------

#[test]
fn a_mode_restricted_definition_resolves_by_identity_regardless_of_mode() {
    // `\vec` is visible in math mode only; the parse resolved it inside `$…$`. Reading
    // resolves the identity address — no mode, no state, no `retrieve_spec`.
    let package = Package::<Latexlike>::new_shared("mathdefs", |package| {
        let spec = MacroSpec::new(argument_specs(["m"]).unwrap())
            .with_provenance(package.provenance_for(CallableType::Macro, "vec").unwrap());
        package.insert_in_modes(CallableType::Macro, "vec", spec, Some(vec![Mode::Math]));
    });
    let language = language(Recovery::Strict, [Arc::clone(&package) as Arc<dyn SpecsProvider<Latexlike>>]);
    let result = parse(&language, "$\\vec{x}$");
    let back = round_trip_tree(session_factory(seed_providers(&language)), &result.tree, ignore_annotations);
    let vec_node = back.root().child(0).unwrap().child(0).unwrap();
    assert_eq!(vec_node.macro_name(), Some("vec"));
    assert!(Arc::ptr_eq(vec_node.spec().unwrap(), package.get(CallableType::Macro, "vec").unwrap()));

    // The lookup that would run at read time — the text-mode query — answers nothing:
    // the identity address is what made the read succeed.
    let query = CallableQuery::new(CallableType::Macro, "vec", crate::scopes::CallableSyntax::Command { escape_char: '\\' });
    let text_state = language.initial_state();
    assert!(package.retrieve_spec(&query, text_state).unwrap().is_none());
}

/// A provider whose answer changes with every query (`\today`-style): the first
/// `retrieve_spec` for `\now` answers spec A, every later one spec B. Its specs are
/// serialized by identity through the provider's own entry (a custom provider type
/// resolves its own spec entries — the crate's reader covers `Package` only).
#[derive(Debug)]
struct CountingProvider {
    calls: AtomicUsize,
    first: Arc<CountedSpec>,
    later: Arc<CountedSpec>,
}

impl CountingProvider {
    fn new_shared() -> Arc<CountingProvider> {
        Arc::new_cyclic(|weak: &alloc::sync::Weak<CountingProvider>| {
            let stamp = |which: &str| {
                SpecProvenance::<Latexlike>::new(
                    alloc::sync::Weak::clone(weak) as alloc::sync::Weak<dyn SpecsProvider<Latexlike>>,
                    CallableType::Macro,
                    DefinitionKey::Name(which.into()),
                )
            };
            CountingProvider {
                calls: AtomicUsize::new(0),
                first: Arc::new(CountedSpec { which: "first", provenance: stamp("first") }),
                later: Arc::new(CountedSpec { which: "later", provenance: stamp("later") }),
            }
        })
    }
}

impl SpecsProvider<Latexlike> for CountingProvider {
    fn name(&self) -> &str {
        "counting"
    }

    fn retrieve_spec(
        &self,
        query: &CallableQuery<'_, '_, Latexlike>,
        _state: &ParsingState<Latexlike>,
    ) -> Result<Option<Arc<dyn CallableSpec<Latexlike>>>, ProviderError> {
        if query.name != "now" {
            return Ok(None);
        }
        let call = self.calls.fetch_add(1, Ordering::SeqCst);
        Ok(Some(if call == 0 { Arc::clone(&self.first) as _ } else { Arc::clone(&self.later) as _ }))
    }
}

impl SerializableObject<Latexlike> for CountingProvider {
    fn serialize_object(&self, _cx: &mut SerializeContext<'_, Latexlike>) -> Result<SerialEntry, SerializeError> {
        Ok(SerialEntry { identifier: "test.counting".into(), data: SerialValue::Null })
    }
}

impl DeserializableObject<Latexlike> for CountingProvider {
    type Output = Arc<dyn SpecsProvider<Latexlike>>;

    fn deserialize_object(
        _value: &SerialValue,
        cx: &mut DeserializeContext<'_, Latexlike>,
    ) -> Result<Self::Output, DeserializeError> {
        let known = cx.user_data::<KnownProviders<Latexlike>>().ok_or_else(|| DeserializeError::failed("no env"))?;
        known.get("counting").cloned().ok_or_else(|| DeserializeError::failed("no counting provider"))
    }
}

/// A spec of the counting provider: zero arguments; serialized as its provider's
/// entry plus which of the two it is.
#[derive(Debug)]
struct CountedSpec {
    which: &'static str,
    provenance: SpecProvenance<Latexlike>,
}

impl CallableSpec<Latexlike> for CountedSpec {}

impl SerializableObject<Latexlike> for CountedSpec {
    fn serialize_object(&self, cx: &mut SerializeContext<'_, Latexlike>) -> Result<SerialEntry, SerializeError> {
        let provider = self.provenance.provider().ok_or_else(|| SerializeError::failed("dropped"))?;
        let position = cx.intern_provider(&provider)?;
        Ok(SerialEntry {
            identifier: "test.counted".into(),
            data: SerialValue::Map(vec![
                (String::from("provider"), SerialValue::Index { table: position.table(), index: position.index() }),
                (String::from("which"), SerialValue::Str(self.which.to_string())),
            ]),
        })
    }
}

impl DeserializableObject<Latexlike> for CountedSpec {
    type Output = Arc<dyn CallableSpec<Latexlike>>;

    fn deserialize_object(
        value: &SerialValue,
        cx: &mut DeserializeContext<'_, Latexlike>,
    ) -> Result<Self::Output, DeserializeError> {
        let SerialValue::Map(fields) = value else { return Err(DeserializeError::failed("shape")) };
        let SerialValue::Index { table, index } = fields[0].1 else { return Err(DeserializeError::failed("shape")) };
        let SerialValue::Str(which) = &fields[1].1 else { return Err(DeserializeError::failed("shape")) };
        let position = crate::serialize::ProviderIndex::from_parts(table, index);
        let provider = cx.provider(position)?;
        let counting = (&*provider as &dyn Any)
            .downcast_ref::<CountingProvider>()
            .ok_or_else(|| DeserializeError::failed("not the counting provider"))?;
        Ok(if which == "first" { Arc::clone(&counting.first) as _ } else { Arc::clone(&counting.later) as _ })
    }
}

#[test]
fn a_provider_whose_answer_changes_later_still_reads_back_the_parsed_instance() {
    let counting = CountingProvider::new_shared();
    let language = language(Recovery::Strict, [Arc::clone(&counting) as Arc<dyn SpecsProvider<Latexlike>>]);
    let result = parse(&language, "\\now");
    // The parse took the FIRST answer.
    let parsed = result.tree.root().child(0).unwrap();
    assert!(Arc::ptr_eq(parsed.spec().unwrap(), &(Arc::clone(&counting.first) as Arc<dyn CallableSpec<Latexlike>>)));
    assert_eq!(counting.calls.load(Ordering::SeqCst), 1);

    let counter = Arc::clone(&counting);
    let factory = move || {
        let mut session = SerdeSession::<Latexlike>::new();
        let mut known = KnownProviders::<Latexlike>::new();
        known.insert(Arc::clone(&counting));
        register_package_recipes(&mut known);
        session.set_user_data(known);
        register(&mut session).unwrap();
        let tables = session.standard_tables().unwrap();
        tables.providers.register_type::<CountingProvider>(&mut session, "test.counting", |p| p).unwrap();
        tables.specs.register_type::<CountedSpec>(&mut session, "test.counted", |s| s).unwrap();
        session
    };
    let back = round_trip_tree(factory, &result.tree, ignore_annotations);
    let node = back.root().child(0).unwrap();
    // The reading environment's provider would now answer `later` — but nothing asked
    // it: the rebuilt tree holds the parsed instance, `first`, and the counter is
    // untouched.
    let spec = node.spec().unwrap();
    let counted = (&**spec as &dyn Any).downcast_ref::<CountedSpec>().unwrap();
    assert_eq!(counted.which, "first");
    assert_eq!(counter.calls.load(Ordering::SeqCst), 1, "the round trip asked the provider nothing");
}

// --- scopes and fallback providers --------------------------------------------------------

#[test]
fn a_scope_and_a_fallback_provider_round_trip_in_full() {
    let oracle = oracle_package();
    let mut scope = Scope::<Latexlike>::new("toplevel");
    // A definition made mid-parse: the package's stamped spec under a new name (identity),
    // and a self-contained one.
    scope.insert(CallableType::Macro, "emphasize", Arc::clone(oracle.get(CallableType::Macro, "emph").unwrap()));
    scope.insert(CallableType::Macro, "shut", Arc::new(EndSpec::<Latexlike>::new()));
    scope.insert(CallableType::Macro, "removed", Arc::new(ErrorCallableSpec::with_detail("gone in v2")));
    let scope: Arc<dyn SpecsProvider<Latexlike>> = Arc::new(scope);
    let mut fallback = FallbackProvider::<Latexlike>::new("anymacro");
    fallback.set(CallableType::Macro, Arc::new(ErrorCallableSpec::new()));
    let fallback: Arc<dyn SpecsProvider<Latexlike>> = Arc::new(fallback);

    // A parse whose seed stack is [fallback, builtin, oracle, scope]: `\emphasize`
    // resolves in the scope, `\zzz` in the fallback (tolerant: an error spec).
    let seed = ParsingState::<Latexlike>::lang_initial()
        .unwrap()
        .derived(&ParsingStateDelta::new().scope_op(ScopeOp::ReplaceStack(vec![
            Arc::clone(&fallback),
            builtin_package(),
            Arc::clone(&oracle) as Arc<dyn SpecsProvider<Latexlike>>,
            Arc::clone(&scope),
        ])))
        .unwrap();
    let language = Language::new(LatexlikeDriver::new(Recovery::Tolerant), seed);
    let result = parse(&language, "\\emphasize{a} \\zzz \\removed");
    let back = round_trip_tree(session_factory(seed_providers(&language)), &result.tree, ignore_annotations);

    let providers = back.root().parsing_state().scopes().providers().to_vec();
    assert_eq!(providers.iter().map(|p| p.name()).collect::<Vec<_>>(), ["anymacro", "_builtin", "oracle", "toplevel"]);
    let back_scope = (&*providers[3] as &dyn Any).downcast_ref::<Scope<Latexlike>>().unwrap();
    assert_eq!(back_scope.len(), 3);
    // Identity survives inside the scope: `emphasize` is the oracle's `emph` Arc.
    assert!(Arc::ptr_eq(
        back_scope.get(CallableType::Macro, "emphasize").unwrap(),
        oracle.get(CallableType::Macro, "emph").unwrap()
    ));
    let removed = back_scope.get(CallableType::Macro, "removed").unwrap();
    let removed = (&**removed as &dyn Any).downcast_ref::<ErrorCallableSpec>().unwrap();
    assert_eq!(removed.detail.as_deref(), Some("gone in v2"));
    let back_fallback = (&*providers[0] as &dyn Any).downcast_ref::<FallbackProvider<Latexlike>>().unwrap();
    assert!((&**back_fallback.get(CallableType::Macro).unwrap() as &dyn Any).downcast_ref::<ErrorCallableSpec>().is_some());
    // The tree's `\emphasize` node holds the same Arc the scope does.
    let node = back.root().child(0).unwrap();
    assert!(Arc::ptr_eq(node.spec().unwrap(), back_scope.get(CallableType::Macro, "emphasize").unwrap()));
}

#[test]
fn a_scope_listing_a_definition_twice_is_rejected() {
    // Hand-built: a `core.scope` entry naming one definition twice.
    let mut writer = SerdeSession::<Latexlike>::new();
    let spec: Arc<dyn CallableSpec<Latexlike>> = Arc::new(EndSpec::<Latexlike>::new());
    let spec_position = writer.intern_spec(&spec).unwrap();
    let mut scope = Scope::<Latexlike>::new("dup");
    scope.insert(CallableType::Macro, "x", Arc::clone(&spec));
    let scope: Arc<dyn SpecsProvider<Latexlike>> = Arc::new(scope);
    writer.intern_provider(&scope).unwrap();
    let mut value = writer.take_segment().to_serial_value();
    // Duplicate the one definition.
    let providers = find_table_entries_mut(&mut value, "providers");
    let data = map_field_mut(&mut providers[0], "data");
    let definitions = list_mut(map_field_mut(data, "definitions"));
    let copy = definitions[0].clone();
    definitions.push(copy);
    let _ = spec_position;
    let segment = crate::serialize::Segment::from_serial_value(&value).unwrap();
    let mut reader = SerdeSession::<Latexlike>::new();
    register(&mut reader).unwrap();
    let error = reader.push_segment(segment).unwrap_err();
    assert!(error.to_string().contains("twice"), "{error}");
}

// --- hostile input and totality ---------------------------------------------------------

#[test]
fn a_hostile_state_over_read_providers_freezes_without_panicking() {
    // A state whose scope stack is [Scope with odd definitions, FallbackProvider,
    // recipe-built minilatex]: `Latexlike::specials_trigger_chars` runs over whatever
    // the readers produced — total, no panic; a wrong-typed definition or an unknown
    // callable type are typed errors.
    let mut writer = SerdeSession::<Latexlike>::new();
    let error_spec: Arc<dyn CallableSpec<Latexlike>> = Arc::new(ErrorCallableSpec::new());
    let mut scope = Scope::<Latexlike>::new("odd");
    scope.insert(CallableType::Specials, "~", Arc::clone(&error_spec));
    scope.insert(CallableType::Environment, "", Arc::clone(&error_spec));
    let mut fallback = FallbackProvider::<Latexlike>::new("fb");
    fallback.set(CallableType::Specials, error_spec);
    let seed = ParsingState::<Latexlike>::lang_initial()
        .unwrap()
        .derived(&ParsingStateDelta::new().scope_op(ScopeOp::ReplaceStack(vec![
            Arc::new(scope),
            Arc::new(fallback),
            minilatex_package(),
        ])))
        .unwrap();
    let state = Arc::new(seed);
    let position = writer.intern_state(&state).unwrap();
    let segment = writer.take_segment();

    let factory = recipe_only_factory();
    let mut reader = factory();
    reader.push_segment(segment.clone()).unwrap();
    let states = reader.standard_tables().unwrap().states;
    let back = reader.state(states.position(position.index())).unwrap();
    assert_eq!(back.scopes().provider_names().collect::<Vec<_>>(), ["minilatex", "fb", "odd"]);
    // The trigger characters were rebuilt from the read providers (minilatex's).
    assert!(back.rules().specials.enabled);

    // Hostile edits: an unknown callable type in a scope definition.
    let mut value = segment.to_serial_value();
    let providers = find_table_entries_mut(&mut value, "providers");
    let data = map_field_mut(&mut providers[0], "data");
    let definitions = list_mut(map_field_mut(data, "definitions"));
    *map_field_mut(&mut definitions[0], "callable_type") = SerialValue::Str("sorcery".into());
    let edited = crate::serialize::Segment::from_serial_value(&value).unwrap();
    let mut reader = factory();
    let error = reader.push_segment(edited).unwrap_err();
    assert!(matches!(innermost(&error), DeserializeError::Value(_)), "{error}");

    // A spec identity naming a provider that is not a package (the fallback).
    let mut value = segment.to_serial_value();
    let providers = find_table_entries_mut(&mut value, "providers");
    let fallback_position = providers.iter().position(|entry| {
        matches!(map_field(entry, "identifier"), Some(SerialValue::Str(id)) if id == "core.fallback-provider")
    })
    .unwrap();
    let specs = find_table_entries_mut(&mut value, "specs");
    specs.push(SerialValue::Map(vec![
        (String::from("identifier"), SerialValue::Str("core.provider-spec-identity".into())),
        (
            String::from("data"),
            SerialValue::Map(vec![
                (
                    String::from("provider"),
                    SerialValue::Index { table: crate::serialize::TableId::new(3), index: fallback_position as u32 },
                ),
                (String::from("callable_type"), SerialValue::Str("macro".into())),
                (String::from("definition"), SerialValue::Map(vec![(String::from("name"), SerialValue::Str("x".into()))])),
            ]),
        ),
    ]));
    let edited = crate::serialize::Segment::from_serial_value(&value).unwrap();
    let mut reader = factory();
    let error = reader.push_segment(edited).unwrap_err();
    assert!(error.to_string().contains("not a Package"), "{error}");
}

// --- parse results with their diagnostics (M6) ---------------------------------------------

/// The segment as the reading side receives it: through JSON under the `serde`
/// feature, as is otherwise.
fn pass_through(segment: Segment) -> Segment {
    #[cfg(feature = "serde")]
    {
        let json = serde_json::to_string(&segment).expect("segment to JSON");
        serde_json::from_str(&json).expect("segment from JSON")
    }
    #[cfg(not(feature = "serde"))]
    {
        segment
    }
}

/// Serialize `result` in a session from `factory`, pass the segment through, read it
/// back in another, and check it against the original: the tree deep-compared, the
/// diagnostics compared on everything the wire carries (identifier, projection,
/// message, rendering, severity, span, frames) with their sources shared with the
/// rebuilt tree, the collection's counts, the session ext.
fn round_trip_parse_result(
    factory: impl Fn() -> SerdeSession<Latexlike>,
    result: &Arc<ParseResult<Latexlike>>,
) -> Arc<ParseResult<Latexlike>> {
    let mut writer = factory();
    let position = writer.serialize_parse_result(result).unwrap();
    let segment = pass_through(writer.take_segment());
    let mut reader = factory();
    reader.push_segment(segment).unwrap();
    let parse_results = reader.standard_tables().unwrap().parse_results;
    let back = reader.parse_result(parse_results.position(position.index())).unwrap();

    assert_trees_equivalent(&result.tree, &back.tree, ignore_annotations);
    // (The preset's session ext is the unit type: nothing to compare.)
    let (a, b) = (&result.diagnostics, &back.diagnostics);
    assert_eq!((b.len(), b.limit(), b.suppressed(), b.error_count()), (a.len(), a.limit(), a.suppressed(), a.error_count()));
    assert_eq!(b.render_all(), a.render_all());
    for (original, rebuilt) in a.iter().zip(b.iter()) {
        assert_eq!(rebuilt.severity(), original.severity());
        assert_eq!(rebuilt.identifier(), original.identifier());
        assert_eq!(rebuilt.data().serializable_data(), original.data().serializable_data());
        assert_eq!(rebuilt.message(), original.message());
        assert_eq!(rebuilt.render(), original.render());
        assert_eq!((rebuilt.span().start(), rebuilt.span().end()), (original.span().start(), original.span().end()));
        assert_eq!(rebuilt.frames().len(), original.frames().len());
        for (fa, fb) in original.frames().iter().zip(rebuilt.frames()) {
            assert_eq!(fa.title(), fb.title());
            assert_eq!((fa.span().start(), fa.span().end()), (fb.span().start(), fb.span().end()));
        }
        // The diagnostic's source is the rebuilt tree's very source (single-source
        // parses here).
        assert!(rebuilt.span().same_source(back.tree.root().span()));
        assert!(rebuilt.data().downcast_ref::<DeserializedCondition>().is_some());
    }
    back
}

#[test]
fn strict_parse_results_round_trip() {
    let language = oracle_language(Recovery::Strict);
    for input in ["\\emph{x} and $m$", "\\begin{itemize}\\item one\\end{itemize}", "plain % c\n text"] {
        let result = Arc::new(parse(&language, input));
        assert!(result.diagnostics.is_empty());
        round_trip_parse_result(session_factory(seed_providers(&language)), &result);
    }
}

#[test]
fn tolerant_parse_results_round_trip_with_their_diagnostics_and_tracebacks() {
    let language = oracle_language(Recovery::Tolerant);
    for (input, framed) in [
        ("\\emph{x", true),                       // unclosed group inside an argument: two frames
        ("\\begin{A}\\emph{x\\end{A}", true),     // orphan end + unclosed group + missing terminator
        ("\\begin{itemize}\\item \\emph{x \\end{itemize}", true),
        ("\\unknown x", false),                   // unknown macro (error spec): recorded at top level
        ("stray }", false),
    ] {
        let result = Arc::new(parse(&language, input));
        assert!(!result.diagnostics.is_empty(), "{input:?} recovers");
        assert_eq!(result.diagnostics.iter().any(|d| !d.frames().is_empty()), framed, "{input:?}: traceback presence");
        let back = round_trip_parse_result(session_factory(seed_providers(&language)), &result);
        assert!(back.diagnostics.has_errors());
        // Found by identifier, as before the round trip.
        for original in result.diagnostics.iter() {
            assert!(back.diagnostics.with_identifier(original.identifier()).next().is_some());
        }
    }
}

#[test]
fn a_parse_result_and_its_diagnostics_share_the_trees_sources() {
    let language = oracle_language(Recovery::Tolerant);
    let result = Arc::new(parse(&language, "\\begin{A}\\emph{x\\end{A}"));
    assert_eq!(result.diagnostics.len(), 3, "{:?}", result.diagnostics);
    let mut writer = session_factory(seed_providers(&language))();
    writer.serialize_parse_result(&result).unwrap();
    let segment = writer.take_segment();
    let count = |name: &str| segment.tables().iter().find(|t| t.name() == name).unwrap().entries().len();
    assert_eq!(count("sources"), 1, "the tree, the diagnostics, and every frame share one source entry");
    assert_eq!(count("diagnostics"), 3);
    assert_eq!(count("trees"), 1);
    assert_eq!(count("parse-results"), 1);
}

// --- determinism ------------------------------------------------------------------------

/// The determinism input: macros, a minilatex list (the body-pushed item package),
/// specials, and math — states, specs, and providers of every kind on the wire.
const DETERMINISM_INPUT: &str = "\\emph{a} \\begin{itemize}\\item x~y\\end{itemize} $m$";

#[test]
fn two_sessions_emit_identical_segments() {
    let language = oracle_language(Recovery::Strict);
    let result = parse(&language, DETERMINISM_INPUT);
    let factory = session_factory(seed_providers(&language));
    let mut a = factory();
    let mut b = factory();
    a.serialize_tree(&result.tree).unwrap();
    b.serialize_tree(&result.tree).unwrap();
    assert_eq!(a.take_segment(), b.take_segment());
}

/// Cross-process determinism: the JSON rendering of the determinism input is pinned
/// by its length and a 64-bit FNV-1a digest (a rendering snapshot recorded by an
/// earlier run of this test — two sessions of one process agreeing proves less, since
/// they share every process-wide state; a hash-map iteration order that reached the
/// wire would show up here). On a mismatch the message carries the JSON, so that a
/// deliberate change of the wire vocabulary can re-pin it. The small-parse
/// `a_small_latexlike_parse_has_a_pinned_json_rendering` pins the exact text.
#[cfg(feature = "serde")]
#[test]
fn the_rendering_of_the_determinism_input_is_pinned_across_processes() {
    let language = oracle_language(Recovery::Strict);
    let result = parse(&language, DETERMINISM_INPUT);
    let mut session = session_factory(seed_providers(&language))();
    session.serialize_tree(&result.tree).unwrap();
    let json = serde_json::to_string(&session.take_segment()).unwrap();
    let digest = json.bytes().fold(0xcbf2_9ce4_8422_2325_u64, |hash, byte| {
        (hash ^ u64::from(byte)).wrapping_mul(0x0000_0100_0000_01b3)
    });
    assert_eq!(
        (json.len(), digest),
        PINNED_DETERMINISM_RENDERING,
        "the rendering changed; re-pin (len, fnv1a64) if the change is deliberate:\n{json}"
    );
}

/// The pinned `(byte length, FNV-1a 64-bit digest)` of the determinism input's JSON.
#[cfg(feature = "serde")]
const PINNED_DETERMINISM_RENDERING: (usize, u64) = (8013, 3_299_525_534_588_708_154);

// --- helpers ------------------------------------------------------------------------------

fn innermost(error: &DeserializeError) -> &DeserializeError {
    match error {
        DeserializeError::InEntry { cause, .. } | DeserializeError::InNode { cause, .. } => innermost(cause),
        other => other,
    }
}

fn innermost_write(error: &SerializeError) -> &SerializeError {
    match error {
        SerializeError::InTable { cause, .. } | SerializeError::InNode { cause, .. } => innermost_write(cause),
        other => other,
    }
}

fn map_field<'a>(value: &'a SerialValue, key: &str) -> Option<&'a SerialValue> {
    match value {
        SerialValue::Map(entries) => entries.iter().find(|(k, _)| k == key).map(|(_, v)| v),
        _ => None,
    }
}

fn map_field_mut<'a>(value: &'a mut SerialValue, key: &str) -> &'a mut SerialValue {
    match value {
        SerialValue::Map(entries) => &mut entries.iter_mut().find(|(k, _)| k == key).expect("field").1,
        _ => panic!("not a map"),
    }
}

fn list_mut(value: &mut SerialValue) -> &mut Vec<SerialValue> {
    match value {
        SerialValue::List(items) => items,
        _ => panic!("not a list"),
    }
}

/// The entry list of the table named `name` in a segment's serial value.
fn find_table_entries_mut<'a>(segment: &'a mut SerialValue, name: &str) -> &'a mut Vec<SerialValue> {
    let tables = list_mut(map_field_mut(segment, "tables"));
    let table = tables
        .iter_mut()
        .find(|table| matches!(map_field(table, "name"), Some(SerialValue::Str(n)) if n == name))
        .expect("table");
    list_mut(map_field_mut(table, "entries"))
}

// --- under the serde feature: parity and a pinned rendering ---------------------------

#[cfg(feature = "serde")]
mod rendering {
    use super::*;
    use crate::engine::{DescentGuard, StdDescentGuard, StdDescentGuardInit};
    use crate::latexlike::{Event, GroupType, MathGroupForm};
    use crate::serialize::{from_value, to_value, DeserializableValue, SerializableValue};

    fn with_contexts<R>(
        f: impl FnOnce(&mut SerializeContext<'_, Latexlike>, &mut DeserializeContext<'_, Latexlike>) -> R,
    ) -> R {
        let mut w = SerdeSession::<Latexlike>::new();
        let mut r = SerdeSession::<Latexlike>::new();
        let mut gw = StdDescentGuard::init(&StdDescentGuardInit::default());
        let mut gr = StdDescentGuard::init(&StdDescentGuardInit::default());
        let mut wcx = SerializeContext::new(&mut w, &mut gw);
        let mut rcx = DeserializeContext::new(&mut r, &mut gr, None);
        f(&mut wcx, &mut rcx)
    }

    /// The value conversion and the serde derive of a vocabulary value render
    /// identically, and both read back.
    fn assert_parity<T>(value: T)
    where
        T: SerializableValue<Latexlike>
            + DeserializableValue<Latexlike>
            + serde::Serialize
            + serde::de::DeserializeOwned
            + PartialEq
            + core::fmt::Debug
            + Clone,
    {
        with_contexts(|wcx, rcx| {
            let by_conversion = value.serialize_value(wcx).unwrap();
            let by_derive = to_value(&value).unwrap();
            assert_eq!(by_conversion, by_derive, "{value:?}");
            assert_eq!(T::deserialize_value(&by_conversion, rcx).unwrap(), value);
            assert_eq!(from_value::<T>(&by_derive).unwrap(), value);
        });
    }

    #[test]
    fn vocabulary_names_agree_between_the_conversions_and_the_derives() {
        for value in [CallableType::Macro, CallableType::Environment, CallableType::Specials] {
            assert_parity(value);
        }
        for value in [
            GroupType::Content,
            GroupType::Math(MathGroupForm::Inline),
            GroupType::Math(MathGroupForm::Display),
            GroupType::Verbatim,
        ] {
            assert_parity(value);
        }
        assert_parity(MathGroupForm::Inline);
        assert_parity(MathGroupForm::Display);
        assert_parity(Mode::Text);
        assert_parity(Mode::Math);
        assert_parity(Event::ExitMathContext);
        assert_parity(BodyMarker::not_body());
        assert_parity(<BodyMarker as crate::node::BodySlotExt>::make_body());
    }

    #[test]
    fn a_small_latexlike_parse_has_a_pinned_json_rendering() {
        let defs = Package::<Latexlike>::new_shared("d", |package| {
            package.define_macro("e", ["m"]).unwrap();
        });
        let language = language(Recovery::Strict, [Arc::clone(&defs) as Arc<dyn SpecsProvider<Latexlike>>]);
        let result = parse(&language, "\\e{x}");
        let mut writer = SerdeSession::<Latexlike>::new();
        writer.serialize_tree(&result.tree).unwrap();
        let json = serde_json::to_string(&writer.take_segment()).unwrap();
        let expected = concat!(
            r#"{"version":1,"meta":{},"tables":["#,
            r#"{"name":"sources","table":0,"start":0,"entries":[{"origin":null,"provenance":"primary","line_number_offset":1,"column_number_offset":1,"text":{"embedded":"\\e{x}"}}]},"#,
            r#"{"name":"states","table":1,"start":0,"entries":["#,
            r#"{"token_rules":{"whitespace":{"enabled":true,"chars":" \t\n\r\u000b\f"},"paragraphs":{"enabled":true},"groups":{"enabled":true,"rules":[{"group_type":"content","open":"{","close":"}"},{"group_type":{"math":"inline"},"open":"$","close":"$"},{"group_type":{"math":"display"},"open":"$$","close":"$$"},{"group_type":{"math":"inline"},"open":"\\(","close":"\\)"},{"group_type":{"math":"display"},"open":"\\[","close":"\\]"}],"temporary":[]},"commands":{"enabled":true,"rules":[{"escape_char":"\\","name_chars":"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ"}]},"comments":{"enabled":true,"rules":[{"start":"%"}]},"specials":{"enabled":true},"forbidden_chars":{"chars":""}},"mode":"text","ext":null,"scopes":[{"$index":[3,0]},{"$index":[3,1]}]},"#,
            r#"{"token_rules":{"whitespace":{"enabled":true,"chars":" \t\n\r\u000b\f"},"paragraphs":{"enabled":true},"groups":{"enabled":true,"rules":[{"group_type":"content","open":"{","close":"}"},{"group_type":{"math":"inline"},"open":"$","close":"$"},{"group_type":{"math":"display"},"open":"$$","close":"$$"},{"group_type":{"math":"inline"},"open":"\\(","close":"\\)"},{"group_type":{"math":"display"},"open":"\\[","close":"\\]"}],"temporary":[],"expecting_close":{"group_type":"content","open":"{","close":"}"}},"commands":{"enabled":true,"rules":[{"escape_char":"\\","name_chars":"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ"}]},"comments":{"enabled":true,"rules":[{"start":"%"}]},"specials":{"enabled":true},"forbidden_chars":{"chars":""}},"mode":"text","ext":null,"scopes":[{"$index":[3,0]},{"$index":[3,1]}]}]},"#,
            r#"{"name":"specs","table":2,"start":0,"entries":[{"identifier":"core.provider-spec-identity","data":{"provider":{"$index":[3,1]},"callable_type":"macro","definition":"#,
            r#"{"name":"e"}}}]},"#,
            r#"{"name":"providers","table":3,"start":0,"entries":[{"identifier":"core.package","data":"#,
            r#"{"name":"_builtin"}},{"identifier":"core.package","data":"#,
            r#"{"name":"d"}}]},"#,
            r#"{"name":"trees","table":4,"start":0,"entries":[{"identifier":"core.tree","data":{"nodes":["#,
            r#"{"kind":"list","span":{"source":{"$index":[0,0]},"start":0,"end":5},"state":{"$index":[1,0]},"ext":null,"children":{"start":1,"end":2}},"#,
            r#"{"kind":{"callable":{"callable_type":"macro","name":"e","spec":{"$index":[2,0]},"arguments":[{"region":{"children":{"start":0,"end":1},"content":{"in_children_of":{"node":2,"start":0,"end":1}}},"ext":null}],"slots":[],"invocation_syntax":{"macro":{"escape_char":"\\","post_space":{"owned":""}}}}},"span":{"source":{"$index":[0,0]},"start":0,"end":5},"state":{"$index":[1,0]},"ext":null,"children":{"start":2,"end":3}},"#,
            r#"{"kind":{"group":{"group_type":"content","open":{"spanned":{"start":2,"end":3}},"close":{"spanned":{"start":4,"end":5}}}},"span":{"source":{"$index":[0,0]},"start":2,"end":5},"state":{"$index":[1,0]},"ext":null,"children":{"start":3,"end":4}},"#,
            r#"{"kind":{"chars":{"content":{"spanned":{"start":3,"end":4}}}},"span":{"source":{"$index":[0,0]},"start":3,"end":4},"state":{"$index":[1,1]},"ext":null,"children":{"start":4,"end":4}}]}}]},"#,
            r#"{"name":"diagnostics","table":5,"start":0,"entries":[]},"#,
            r#"{"name":"parse-results","table":6,"start":0,"entries":[]}]}"#,
        );
        assert_eq!(json, expected);
    }

    /// The setup of the schema draft's worked example
    /// (`dev-docs/serialization/schema_draft.md`): a tolerant parse of `\e{x} {`
    /// (a macro `\e` with one mandatory argument, defined in a shared package `d`;
    /// then an unclosed group) serialized as a parse result into a fresh session
    /// declaring the profile `schema-draft example`, the segment naming the parse
    /// result as its main entry. Ignored: run it to REGENERATE the draft's example
    /// after a wire change —
    /// `cargo test --features serde -p techy --lib schema_draft_worked_example -- --ignored --nocapture`
    /// — and paste its output; the draft's example is never edited by hand. Prints
    /// the exact canonical line first, then a readable per-entry layout.
    #[test]
    #[ignore = "prints the schema draft's worked example; run with --ignored --nocapture to regenerate it"]
    fn schema_draft_worked_example() {
        let defs = Package::<Latexlike>::new_shared("d", |package| {
            package.define_macro("e", ["m"]).unwrap();
        });
        let language = language(Recovery::Tolerant, [Arc::clone(&defs) as Arc<dyn SpecsProvider<Latexlike>>]);
        let result = Arc::new(parse(&language, "\\e{x} {"));
        let mut writer = SerdeSession::<Latexlike>::new();
        writer.set_profile("schema-draft example");
        let position = writer.serialize_parse_result(&result).unwrap();
        let segment = writer.take_segment_with_main(position).unwrap();

        std::println!("=== canonical rendering (one line) ===");
        std::println!("{}", serde_json::to_string(&segment).unwrap());
        std::println!("=== envelope ===");
        std::println!(
            "version {}; meta {}; main {}",
            segment.version(),
            segment.meta().profile().map_or(String::from("{}"), |p| alloc::format!("{{\"profile\": {p:?}}}")),
            segment.main().map_or(String::from("none"), |(t, i)| alloc::format!("{{\"$index\": [{}, {}]}}", t.ordinal(), i)),
        );
        for table in segment.tables() {
            std::println!(
                "=== table `{}` (ordinal {}, start {}, {} entries) ===",
                table.name(),
                table.id().ordinal(),
                table.start(),
                table.entries().len()
            );
            for (position, entry) in table.entries().iter().enumerate() {
                std::println!("--- entry {position} ---");
                if table.name() == "trees" {
                    // One node per line keeps the tree readable.
                    let SerialValue::Map(fields) = entry else { panic!() };
                    let SerialValue::Map(data) = &fields[1].1 else { panic!() };
                    let SerialValue::List(nodes) = &data[0].1 else { panic!() };
                    std::println!("{{\"identifier\": {}, \"data\": {{\"nodes\": [", serde_json::to_string(&fields[0].1).unwrap());
                    for (i, node) in nodes.iter().enumerate() {
                        let comma = if i + 1 < nodes.len() { "," } else { "" };
                        std::println!("  {}{comma}", serde_json::to_string(node).unwrap());
                    }
                    std::println!("]}}}}");
                } else {
                    print_two_levels(entry);
                }
            }
        }
    }

    /// Print a map value with its keys one per line and, for a value that is itself a
    /// map of several keys, that map's keys one per line too (compact below that):
    /// the readable layout of the schema draft's examples.
    fn print_two_levels(value: &SerialValue) {
        fn compact(value: &SerialValue) -> String {
            serde_json::to_string(value).unwrap()
        }
        let SerialValue::Map(fields) = value else {
            std::println!("{}", compact(value));
            return;
        };
        std::println!("{{");
        for (i, (key, value)) in fields.iter().enumerate() {
            let comma = if i + 1 < fields.len() { "," } else { "" };
            match value {
                SerialValue::Map(inner) if inner.len() > 1 => {
                    std::println!("  {key:?}: {{");
                    for (j, (k, v)) in inner.iter().enumerate() {
                        let comma = if j + 1 < inner.len() { "," } else { "" };
                        std::println!("    {k:?}: {}{comma}", compact(v));
                    }
                    std::println!("  }}{comma}");
                }
                other => std::println!("  {key:?}: {}{comma}", compact(other)),
            }
        }
        std::println!("}}");
    }
}

