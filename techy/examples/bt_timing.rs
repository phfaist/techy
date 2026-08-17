//! Parse-throughput probe for the "better tokens" work: generate a deterministic
//! ~5 MB LaTeX-like document and print the wall-clock milliseconds the parse takes
//! (generation excluded).
//!
//! Run with `cargo run --release --example bt_timing`. The same file is run against
//! the branch and against the baseline `main`, and the medians of five runs each are
//! compared. It uses only public parse entry points that exist on both trees.
//!
//! Temporary: this example is deleted in the plan's final stage.

use std::sync::Arc;
use std::time::Instant;

use techy::core::specs::{ArgumentSpec, Package};
use techy::core::{Language, ParsingState};
use techy::error::Recovery;
use techy::latexlike::{
    argument_specs, builtin_package, CallableType, EnvironmentSpec, Latexlike,
    LatexlikeDriver, MacroSpec,
};

/// Target size of the generated document, in bytes (the generator overshoots by at
/// most one fragment).
const TARGET_BYTES: usize = 5 * 1024 * 1024;

/// A linear congruential generator (the constants of Numerical Recipes): a fixed seed
/// makes the document identical on every run and on every tree.
struct Lcg(u32);

impl Lcg {
    fn next(&mut self) -> u32 {
        self.0 = self.0.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        self.0
    }

}

fn words(rng: &mut Lcg, count: u32, out: &mut String) {
    const WORDS: [&str; 8] =
        ["alpha", "beta", "gamma", "delta", "epsilon", "zeta", "eta", "theta"];
    for i in 0..count {
        if i > 0 {
            out.push(' ');
        }
        out.push_str(WORDS[(rng.next() % WORDS.len() as u32) as usize]);
    }
}

fn document() -> String {
    let mut rng = Lcg(0x5eed_1234);
    let mut out = String::with_capacity(TARGET_BYTES + 4096);
    while out.len() < TARGET_BYTES {
        match rng.next() % 8 {
            0 => {
                // Plain text.
                let count = 6 + rng.next() % 10;
                words(&mut rng, count, &mut out);
                out.push(' ');
            }
            1 => {
                // A command with one mandatory argument.
                out.push_str("\\emph{");
                let count = 1 + rng.next() % 4;
                words(&mut rng, count, &mut out);
                out.push_str("} ");
            }
            2 => {
                // A command with no arguments.
                out.push_str("\\alpha ");
            }
            3 => {
                // A bare group.
                out.push('{');
                let count = 2 + rng.next() % 5;
                words(&mut rng, count, &mut out);
                out.push_str("} ");
            }
            4 => {
                // A comment line.
                out.push_str("% ");
                let count = 3 + rng.next() % 6;
                words(&mut rng, count, &mut out);
                out.push('\n');
            }
            5 => {
                // A paragraph break.
                out.push_str("\n\n");
            }
            6 => {
                // A nested command inside a group.
                out.push_str("{\\textbf{");
                words(&mut rng, 2, &mut out);
                out.push_str("} ");
                words(&mut rng, 3, &mut out);
                out.push_str("} ");
            }
            _ => {
                // An environment with a few items.
                out.push_str("\\begin{itemize}\n");
                let items = 1 + rng.next() % 4;
                for _ in 0..items {
                    out.push_str("  \\item ");
                    let count = 4 + rng.next() % 6;
                words(&mut rng, count, &mut out);
                    out.push('\n');
                }
                out.push_str("\\end{itemize}\n");
            }
        }
    }
    out
}

fn definitions() -> Package<Latexlike> {
    let one_argument: Vec<Arc<ArgumentSpec<Latexlike>>> =
        argument_specs(&["m"]).expect("argument specs");
    let mut package = Package::new("bt_timing");
    for name in ["emph", "textbf"] {
        package.insert(
            CallableType::Macro,
            name,
            Arc::new(MacroSpec::new(one_argument.clone())),
        );
    }
    for name in ["alpha", "item"] {
        package.insert(CallableType::Macro, name, Arc::new(MacroSpec::new(vec![])));
    }
    package.insert(
        CallableType::Environment,
        "itemize",
        Arc::new(EnvironmentSpec::new(vec![])),
    );
    package
}

fn main() {
    let content = document();
    let language: Language<Latexlike> = Language::new(
        LatexlikeDriver::new(Recovery::Strict),
        ParsingState::lang_initial_with_packages([
            builtin_package(),
            Arc::new(definitions()),
        ])
        .expect("seed state"),
    );

    let started = Instant::now();
    let result = language.parse(&content).expect("the generated document parses");
    let elapsed = started.elapsed();

    // Touch the result so nothing is optimized away.
    let nodes = result.tree.root().child_count();
    println!(
        "bytes {} nodes {} diagnostics {} parse_ms {:.1}",
        content.len(),
        nodes,
        result.diagnostics.len(),
        elapsed.as_secs_f64() * 1000.0
    );
}
