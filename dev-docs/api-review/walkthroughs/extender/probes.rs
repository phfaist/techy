//! Probes: mistakes and variants a real extender would try, to see what the API
//! does. Not part of the polished example; findings feed FRICTION.md.

use std::sync::Arc;

use techy::engine::Language;
use techy::latexlike::{
    argument_specs, argument_specs_from_str, CallableType, EnvironmentSpec, Latexlike, MacroSpec,
};
use techy::scopes::Package;
use techy::spec::ArgumentSpec;

fn base_language(package: Package<Latexlike>) -> Language<Latexlike> {
    Language::<Latexlike>::default().with_provider(Arc::new(package)).unwrap()
}

fn main() {
    // ---- Probe A: registering the name WITH a backslash, as a LaTeX user would. ----
    println!("== Probe A: package.insert(CallableType::Macro, \"\\\\greet\", ...) ==");
    let mut package = Package::new("p");
    package.insert(
        CallableType::Macro,
        r"\greet",
        Arc::new(MacroSpec::new(argument_specs(["m"]).unwrap())),
    );
    let language = base_language(package);
    match language.parse(r"\greet{x}") {
        Ok(result) => println!(
            "   parse OK, child0 = {}",
            result.tree.root().child(0).unwrap().summary()
        ),
        Err(err) => println!("   parse Err: {err}"),
    }

    // ---- Probe B: wrong spec type under a callable type (EnvironmentSpec as Macro). ----
    println!("== Probe B: EnvironmentSpec registered under CallableType::Macro ==");
    let mut package = Package::new("p");
    package.insert(
        CallableType::Macro,
        "thing",
        Arc::new(EnvironmentSpec::new(argument_specs(["m"]).unwrap())),
    );
    let language = base_language(package);
    match language.parse(r"\thing{x}") {
        Ok(result) => println!(
            "   parse OK, child0 = {}",
            result.tree.root().child(0).unwrap().summary()
        ),
        Err(err) => println!("   parse Err: {err}"),
    }

    // ---- Probe C: MacroSpec registered under CallableType::Environment. ----
    println!("== Probe C: MacroSpec registered under CallableType::Environment ==");
    let mut package = Package::new("p");
    package.insert(
        CallableType::Environment,
        "thing",
        Arc::new(MacroSpec::new(argument_specs(["m"]).unwrap())),
    );
    let language = base_language(package);
    match language.parse(r"\begin{thing}{x} body \end{thing}") {
        Ok(result) => println!(
            "   parse OK, child0 = {}",
            result.tree.root().child(0).unwrap().summary()
        ),
        Err(err) => println!("   parse Err: {err}"),
    }

    // ---- Probe D: absent optional vs out-of-range index — distinguishable? ----
    println!("== Probe D: argument_content_nodes for absent-optional vs out-of-range ==");
    let mut package = Package::new("p");
    package.insert(
        CallableType::Macro,
        "greet",
        Arc::new(MacroSpec::new(argument_specs(["o", "m"]).unwrap())),
    );
    let language = base_language(package);
    let result = language.parse(r"\greet{World}").unwrap();
    let greet = result.tree.root().child(0).unwrap();
    println!(
        "   absent optional  : argument_content_nodes(0) = {:?}",
        greet.argument_content_nodes(0).map(|n| n.len())
    );
    println!(
        "   out-of-range (7) : argument_content_nodes(7) = {:?}",
        greet.argument_content_nodes(7).map(|n| n.len())
    );
    println!(
        "   arguments().len(): {:?}, arguments().get(7): {}",
        greet.arguments().map(|a| a.len()),
        greet.arguments().unwrap().get(7).is_some(),
    );

    // ---- Probe E: the compact code string — same result as the list form? ----
    println!("== Probe E: argument_specs_from_str(\"om\") vs argument_specs([\"o\",\"m\"]) ==");
    let mut package = Package::new("p");
    package.insert(
        CallableType::Macro,
        "greet",
        Arc::new(MacroSpec::new(argument_specs_from_str("om").unwrap())),
    );
    let language = base_language(package);
    let result = language.parse(r"\greet[Hi]{World}").unwrap();
    let greet = result.tree.root().child(0).unwrap();
    println!(
        "   compact form parses the same: optional = {:?}, mandatory = {:?}",
        greet.argument_content_nodes(0).unwrap().source_text(),
        greet.argument_content_nodes(1).unwrap().source_text(),
    );

    // ---- Probe F: zero-argument macro — is there a nicer spelling than Vec::new()? ----
    println!("== Probe F: zero-argument macro via argument_specs_from_str(\"\") ==");
    let empty = argument_specs_from_str("").unwrap();
    println!("   argument_specs_from_str(\"\") -> {} specs (OK)", empty.len());
    // argument_specs([]) needs a type annotation to compile:
    let empty2 = argument_specs::<[&str; 0]>([]).unwrap();
    println!("   argument_specs::<[&str; 0]>([]) -> {} specs (needs turbofish)", empty2.len());

    // ---- Probe G: same name as macro AND environment — do they collide? ----
    println!("== Probe G: \\proof macro and {{proof}} environment side by side ==");
    let mut package = Package::new("p");
    package.insert(
        CallableType::Macro,
        "proof",
        Arc::new(MacroSpec::new(Vec::new())),
    );
    package.insert(
        CallableType::Environment,
        "proof",
        Arc::new(EnvironmentSpec::new(Vec::new())),
    );
    let language = base_language(package);
    match language.parse(r"\proof and \begin{proof}x\end{proof}") {
        Ok(result) => {
            let shapes: Vec<String> =
                result.tree.root().children().iter().map(|n| n.summary()).collect();
            println!("   both coexist: {shapes:?}");
        }
        Err(err) => println!("   parse Err: {err}"),
    }

    // ---- Probe I: named arguments — can I combine argument codes with names? ----
    println!("== Probe I: naming the arguments of \\greet for by-name access ==");
    // argument_specs() attaches no names; ArgumentSpec::named() exists but the factory
    // hands back Arc'd specs. Workaround: rebuild each spec around its (public) parser.
    let named_specs: Vec<Arc<ArgumentSpec<Latexlike>>> = argument_specs(["o", "m"])
        .unwrap()
        .into_iter()
        .zip(["greeting", "name"])
        .map(|(spec, name)| Arc::new(ArgumentSpec::new(spec.parser.clone()).named(name)))
        .collect();
    let mut package = Package::new("p");
    package.insert(CallableType::Macro, "greet", Arc::new(MacroSpec::new(named_specs)));
    let language = base_language(package);
    let result = language.parse(r"\greet[Hi]{World}").unwrap();
    let greet = result.tree.root().child(0).unwrap();
    println!(
        "   by name: greeting = {:?}, name = {:?}",
        greet.argument_content_nodes_named("greeting").unwrap().source_text(),
        greet.argument_content_nodes_named("name").unwrap().source_text(),
    );

    // ---- Probe H: missing \end{...}: what does my user see? ----
    println!("== Probe H: forgotten \\end{{theorem}} ==");
    let mut package = Package::new("p");
    package.insert(
        CallableType::Environment,
        "theorem",
        Arc::new(EnvironmentSpec::new(Vec::new())),
    );
    let language = base_language(package);
    match language.parse(r"\begin{theorem} oops") {
        Ok(_) => println!("   parse OK?!"),
        Err(err) => {
            println!("   parse Err: {err}");
            println!("{}", err.render());
        }
    }
}
