//! Callable specs: the behavior attached to anything invocable from the token stream.
//!
//! Implements ARCHITECTURE.md §specs (Phase 4; argument model rebuilt on pylatexenc's
//! `LatexArgumentSpec`, July 2026). A [`CallableSpec`] records *callable behavior*, not
//! the form or name under which it is invoked — specs are **de-keyed**: the invocation
//! form is the language's closed [`Lang::CallableTypeId`](crate::state::Lang) and the
//! name lives in the library key (normalized) and on the node (invocation spelling). One
//! spec may back several names (flyweight), including the per-type unknown-callable
//! fallback singletons of [`LibraryStack`](crate::library::LibraryStack).
//!
//! The declarative surface is the [`ArgumentSpec`]/[`SlotSpec`] lists exposed by the
//! spec (arguments *configure*; slots hold *content regions*), with [`StdCallableSpec`]
//! as the standard implementation. An argument **is a parser**: every argument routes to
//! an [`ArgumentParser`] object, with the standard forms (delimited group, optional
//! group, literal marker, …) provided by the preset — the core has no privileged
//! argument shapes (revised July 2026). The `ArgumentParser` entry point, the standard
//! argument parsers, and the `make_invocation_parser()` factory (whose override is the
//! full-takeover hatch) arrive in Phase 6.4–6.5 (DESIGN_RATIONALE.md §3.6).
//!
//! Definition lookup — [`Library`](crate::library::Library) and friends — lives in the
//! neighboring [`library`](crate::library) topic.

mod callable;
mod structure;

pub use callable::{CallableSpec, StdCallableSpec};
pub use structure::{ArgumentParser, ArgumentSpec, SlotSpec};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::Lang;
    use alloc::string::String;
    use alloc::sync::Arc;
    use alloc::vec;

    #[derive(Debug, Clone, Copy)]
    struct PlainLang;
    impl Lang for PlainLang {
        type GroupTypeId = u32;
        type CallableTypeId = u32;
        type StateExt = ();
        type Event = ();
        type SessionExt = ();
        type SourceOrigin = Option<String>;
        type NodeExts = ();
    }

    /// Stand-in for the preset-provided standard argument parsers (delimited group,
    /// optional group, marker, …).
    #[derive(Debug)]
    struct StubParser;
    impl<L: Lang> ArgumentParser<L> for StubParser {}

    fn stub() -> Arc<dyn ArgumentParser<PlainLang>> {
        Arc::new(StubParser)
    }

    #[test]
    fn default_trait_methods_are_the_neutral_callable() {
        #[derive(Debug)]
        struct Bare;
        impl CallableSpec<PlainLang> for Bare {}

        let spec = Bare;
        assert!(CallableSpec::<PlainLang>::arguments(&spec).is_empty());
        assert!(CallableSpec::<PlainLang>::slots(&spec).is_empty());
    }

    #[test]
    fn std_callable_spec_exposes_its_structures() {
        // \section*[placement]{title}-shaped, with a body slot for good measure (the
        // stubs stand in for the preset's marker / optional-group / group parsers).
        let spec = StdCallableSpec::new(
            vec![
                Arc::new(ArgumentSpec::new(stub())),
                Arc::new(ArgumentSpec::new(stub()).named("placement")),
                Arc::new(ArgumentSpec::new(stub())),
            ],
            vec![Arc::new(SlotSpec::new().named("body"))],
        );

        let dyn_spec: &dyn CallableSpec<PlainLang> = &spec;
        assert_eq!(dyn_spec.arguments().len(), 3);
        assert_eq!(dyn_spec.arguments()[1].name.as_deref(), Some("placement"));
        assert_eq!(dyn_spec.slots().len(), 1);
        assert_eq!(dyn_spec.slots()[0].name.as_deref(), Some("body"));
    }

    #[test]
    fn argument_parsers_and_state_deltas_have_a_slot() {
        // The pylatexenc mid-granularity extension point: a bespoke argument parser and
        // a per-argument parsing-state delta, declared without a custom invocation
        // parser.
        #[derive(Debug)]
        struct CharsOnly;
        impl ArgumentParser<PlainLang> for CharsOnly {}

        let spec: StdCallableSpec<PlainLang> = StdCallableSpec::new(
            vec![Arc::new(
                ArgumentSpec::new(Arc::new(CharsOnly))
                    .named("label")
                    .with_state_delta(crate::state::ParsingStateDelta::new()),
            )],
            vec![],
        );

        let arg = &spec.arguments[0];
        assert!(arg.parsing_state_delta.is_some());
        // Specs stay debuggable through the dyn parser.
        let dump = alloc::format!("{:?}", spec);
        assert!(dump.contains("CharsOnly"));
    }
}
