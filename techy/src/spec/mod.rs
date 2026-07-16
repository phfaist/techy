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
//! The declarative surface is the [`ArgumentSpec`] list exposed by the spec (arguments
//! *configure* an invocation), with [`StdCallableSpec`] as the standard implementation.
//! Slots — a parsed callable's *content regions* — have no spec-side declaration
//! (decided July 2026, slots session): they are record-level vocabulary
//! ([`ParsedSlot`](crate::node::ParsedSlot)), minted by the takeover parser that reads
//! the body; the spec announces that it takes material via
//! [`CallableSpec::requires_content`]. An argument **is a parser**: every argument routes to
//! an [`ArgumentParser`] object, with the standard forms (delimited group, optional
//! group, literal marker, …) provided as core construct parsers, parameterized by group
//! types and rules — the core has no privileged argument *spellings* (revised July
//! 2026). The [`ArgumentParser`] entry point ([`parse_argument`], returning
//! [`ParsedArgumentNodes`]) and the standard argument parsers landed in Phase 6.5; the
//! `make_invocation_parser()` factory (whose override is the full-takeover hatch) in
//! Phase 6.4 (DESIGN_RATIONALE.md §3.6).
//!
//! [`parse_argument`]: ArgumentParser::parse_argument
//!
//! Definition lookup — [`Library`](crate::library::Library) and friends — lives in the
//! neighboring [`library`](crate::library) topic.

mod callable;
mod structure;

pub use callable::{CallableSpec, FrameRole, StdCallableSpec};
pub use structure::{ArgumentParser, ArgumentSpec, ParsedArgumentNodes};

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
        type ModeId = ();
        type StateExt = ();
        type Event = ();
        type SessionExt = ();
        type SourceOrigin = Option<String>;
        type NodeExts = ();
    }

    /// Stand-in for the standard argument parsers (delimited group, optional group,
    /// marker, …) — never invoked here; structure tests only.
    #[derive(Debug)]
    struct StubParser;
    impl<L: Lang> ArgumentParser<L> for StubParser {
        fn parse_argument(
            &self,
            _cx: &mut crate::constructs::ParseContext<'_, '_, L>,
            _spec: &ArgumentSpec<L>,
        ) -> crate::constructs::ConstructParserResult<L, Option<ParsedArgumentNodes>> {
            Ok(None)
        }
    }

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
        // No arguments ⇒ nothing that couldn't match empty ⇒ bare use is fine.
        assert!(!CallableSpec::<PlainLang>::requires_content(&spec));
    }

    #[test]
    fn std_callable_spec_exposes_its_structure() {
        // \section*[placement]{title}-shaped (the stubs stand in for the core's
        // marker / optional-group / group parsers).
        let spec = StdCallableSpec::new(vec![
            Arc::new(ArgumentSpec::new(stub())),
            Arc::new(ArgumentSpec::new(stub()).named("placement")),
            Arc::new(ArgumentSpec::new(stub())),
        ]);

        let dyn_spec: &dyn CallableSpec<PlainLang> = &spec;
        assert_eq!(dyn_spec.arguments().len(), 3);
        assert_eq!(dyn_spec.arguments()[1].name.as_deref(), Some("placement"));
    }

    #[test]
    fn requires_content_derives_from_the_arguments() {
        /// A stand-in mandatory parser: cannot match empty.
        #[derive(Debug)]
        struct MandatoryStub;
        impl<L: Lang> ArgumentParser<L> for MandatoryStub {
            fn parse_argument(
                &self,
                _cx: &mut crate::constructs::ParseContext<'_, '_, L>,
                _spec: &ArgumentSpec<L>,
            ) -> crate::constructs::ConstructParserResult<L, Option<ParsedArgumentNodes>>
            {
                Ok(None)
            }
            fn can_match_empty(&self) -> bool {
                false
            }
        }

        // StubParser keeps the trait default: can match empty (the pylatexenc
        // base-class polarity).
        let optional_only: StdCallableSpec<PlainLang> =
            StdCallableSpec::new(vec![Arc::new(ArgumentSpec::new(stub()))]);
        assert!(!CallableSpec::requires_content(&optional_only));

        let with_mandatory: StdCallableSpec<PlainLang> = StdCallableSpec::new(vec![
            Arc::new(ArgumentSpec::new(stub())),
            Arc::new(ArgumentSpec::new(Arc::new(MandatoryStub))),
        ]);
        assert!(CallableSpec::requires_content(&with_mandatory));

        // The override channel for body-bearing takeover specs (`\begin`, `\verb`):
        // declares nothing, still takes material.
        #[derive(Debug)]
        struct TakeoverSpec;
        impl CallableSpec<PlainLang> for TakeoverSpec {
            fn requires_content(&self) -> bool {
                true
            }
        }
        assert!(CallableSpec::requires_content(&TakeoverSpec));
    }

    #[test]
    fn custom_argument_parser_and_delta_need_no_invocation_parser() {
        // The pylatexenc mid-granularity extension point: a bespoke argument parser and
        // a per-argument parsing-state delta, declared without a custom invocation
        // parser. (Renamed July 2026 — "have a slot" collided with the record-level
        // slot vocabulary, which this test is not about.)
        #[derive(Debug)]
        struct CharsOnly;
        impl ArgumentParser<PlainLang> for CharsOnly {
            fn parse_argument(
                &self,
                _cx: &mut crate::constructs::ParseContext<'_, '_, PlainLang>,
                _spec: &ArgumentSpec<PlainLang>,
            ) -> crate::constructs::ConstructParserResult<
                PlainLang,
                Option<ParsedArgumentNodes>,
            > {
                Ok(None)
            }
        }

        let spec: StdCallableSpec<PlainLang> = StdCallableSpec::new(vec![Arc::new(
            ArgumentSpec::new(Arc::new(CharsOnly))
                .named("label")
                .with_state_delta(crate::state::ParsingStateDelta::new()),
        )]);

        let arg = &spec.arguments[0];
        assert!(arg.parsing_state_delta.is_some());
        // Specs stay debuggable through the dyn parser.
        let dump = alloc::format!("{:?}", spec);
        assert!(dump.contains("CharsOnly"));
    }
}
