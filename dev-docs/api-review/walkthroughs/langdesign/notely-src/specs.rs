//! Task 3: notely's standard package — two declarative callables.
//!
//! - `@bold(...)`  — one mandatory paren-delimited argument
//! - `@link(url)(text)` — two mandatory paren-delimited arguments, named

use std::sync::Arc;

use techy::constructs::GroupArgumentParser;
use techy::scopes::Package;
use techy::spec::{ArgumentSpec, StdCallableSpec};

use crate::lang::{paren_rule, Notely, NotelyCallable};

/// One mandatory `(...)` argument (delimiters minted per-argument; no expression
/// fallback — notely arguments are always parenthesized).
fn paren_argument() -> Arc<ArgumentSpec<Notely>> {
    Arc::new(ArgumentSpec::new(Arc::new(GroupArgumentParser::with_rule(
        paren_rule(),
    ))))
}

fn named_paren_argument(name: &str) -> Arc<ArgumentSpec<Notely>> {
    Arc::new(
        ArgumentSpec::new(Arc::new(GroupArgumentParser::with_rule(paren_rule())))
            .named(name),
    )
}

/// The notely standard package: `@bold`, `@link`.
pub fn notely_std() -> Package<Notely> {
    let mut package = Package::new("notely-std");
    package.insert(
        NotelyCallable::Command,
        "bold",
        Arc::new(StdCallableSpec::new(vec![paren_argument()])),
    );
    package.insert(
        NotelyCallable::Command,
        "link",
        Arc::new(StdCallableSpec::new(vec![
            named_paren_argument("url"),
            named_paren_argument("text"),
        ])),
    );
    // A specials shorthand: `-->` is an argument-less callable (an arrow).
    package.insert_specials(
        "-->",
        NotelyCallable::Shorthand,
        Arc::new(StdCallableSpec::default()),
    );
    package
}
