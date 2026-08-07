//! Temporary probe: how much stack does one nesting level cost, in the parser and in
//! the tree-consuming APIs?
//!
//! Each trial runs in a subprocess (a stack overflow aborts the process), on a thread
//! with an explicit stack size; the parent binary-searches the deepest input that
//! survives. bytes/level = stack_size / max_depth.
//!
//! Usage: cargo run [--release] --example stack_probe -- <stack_kib>

use std::sync::Arc;

use techy::core::node::{validate_tree, NodeRef};
use techy::core::specs::Package;
use techy::core::{Language, ParsingState};
use techy::error::Recovery;
use techy::latexlike::{
    argument_specs, source_recomposer, CallableType, EnvironmentSpec, Latexlike,
    LatexlikeDriver, MacroSpec,
};
use techy::recompose::recompose;
use techy::visit::{walk, NodeVisitor, VisitContext, VisitFlow};

const PARSE_KINDS: &[&str] = &["group", "macro", "optarg", "env"];
const CONSUME_OPS: &[&str] = &["walk", "recompose", "validate", "debug", "drop"];

fn testdb() -> Package<Latexlike> {
    let mut db = Package::new("probedb");
    db.insert(
        CallableType::Macro,
        "emph",
        Arc::new(MacroSpec::new(argument_specs(&["m"]).unwrap())),
    );
    db.insert(
        CallableType::Macro,
        "cite",
        Arc::new(MacroSpec::new(argument_specs(&["o", "m"]).unwrap())),
    );
    db.insert(
        CallableType::Environment,
        "itemize",
        Arc::new(EnvironmentSpec::new(Vec::new())),
    );
    db
}

fn lang() -> Language<Latexlike> {
    Language::new(
        LatexlikeDriver::new(Recovery::Tolerant),
        ParsingState::lang_initial_with_packages([testdb()]),
    )
}

fn make(kind: &str, depth: usize) -> String {
    match kind {
        "group" => format!("{}{}", "{".repeat(depth), "}".repeat(depth)),
        "macro" => format!("{}{}", "\\emph{".repeat(depth), "}".repeat(depth)),
        "optarg" => format!("{}{}", "\\cite[".repeat(depth), "]{x}".repeat(depth)),
        "env" => format!(
            "{}{}",
            "\\begin{itemize}".repeat(depth),
            "\\end{itemize}".repeat(depth)
        ),
        _ => panic!("unknown kind {kind}"),
    }
}

struct Counter(usize);
impl NodeVisitor<Latexlike, ()> for Counter {
    fn enter(
        &mut self,
        _n: NodeRef<'_, Latexlike, ()>,
        _cx: &VisitContext<'_, Latexlike, ()>,
    ) -> VisitFlow {
        self.0 += 1;
        VisitFlow::Descend
    }
}

/// Run `f` on a fresh thread with `stack` bytes.
fn on_stack<R: Send + 'static>(stack: usize, f: impl FnOnce() -> R + Send + 'static) -> R {
    std::thread::Builder::new()
        .stack_size(stack)
        .spawn(f)
        .unwrap()
        .join()
        .unwrap()
}

fn child(args: &[String]) -> ! {
    let mode = &args[1];
    let what = &args[2];
    let depth: usize = args[3].parse().unwrap();
    let stack = args[4].parse::<usize>().unwrap() * 1024;

    if mode == "parse" {
        let input = make(what, depth);
        let ok = on_stack(stack, move || lang().parse(input).is_ok());
        std::process::exit(if ok { 0 } else { 1 });
    }

    // Consume mode: parse on a generous stack, then run the op on `stack` bytes.
    let input = make("group", depth);
    let tree = on_stack(512 << 20, move || lang().parse(input).unwrap().tree);
    let op = what.clone();
    let ok = on_stack(stack, move || {
        match op.as_str() {
            "walk" => {
                let mut c = Counter(0);
                walk(tree.root(), &mut c);
                std::hint::black_box(c.0);
            }
            "recompose" => {
                let s = recompose(&tree, (), &mut source_recomposer()).unwrap();
                std::hint::black_box(s.len());
            }
            "validate" => validate_tree(&tree).unwrap(),
            "debug" => {
                std::hint::black_box(format!("{:?}", tree).len());
            }
            "drop" => drop(tree),
            _ => panic!("unknown op"),
        }
        true
    });
    std::process::exit(if ok { 0 } else { 1 });
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() >= 5 {
        child(&args);
    }

    let stack_kib: usize = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(1024);
    let exe = std::env::current_exe().unwrap();
    let budget = (stack_kib * 1024) as f64;

    let search = |mode: &str, what: &str| -> usize {
        let trial = |depth: usize| -> bool {
            std::process::Command::new(&exe)
                .args([mode, what, &depth.to_string(), &stack_kib.to_string()])
                .stderr(std::process::Stdio::null())
                .status()
                .map(|s| s.success())
                .unwrap_or(false)
        };
        let mut hi = 1usize;
        while hi <= 1 << 21 && trial(hi) {
            hi *= 2;
        }
        let (mut lo, mut hi) = (hi / 2, hi.min(1 << 21));
        while lo + 1 < hi {
            let mid = (lo + hi) / 2;
            if trial(mid) {
                lo = mid;
            } else {
                hi = mid;
            }
        }
        lo
    };

    println!("stack per thread: {stack_kib} KiB\n\nPARSE (input nesting depth):");
    for kind in PARSE_KINDS {
        let d = search("parse", kind);
        println!("  {kind:10} max depth {d:8}   ≈ {:8.0} B/level", budget / d as f64);
    }
    println!("\nCONSUME a `{{{{…}}}}` tree (parsed on a 512 MiB stack):");
    for op in CONSUME_OPS {
        let d = search("consume", op);
        println!("  {op:10} max depth {d:8}   ≈ {:8.0} B/level", budget / d as f64);
    }
}
