//! Callable specs: the behavior attached to anything invocable from the token stream.
//!
//! Implements ARCHITECTURE.md §specs (Phase 4). A [`CallableSpec`] records *callable
//! behavior*, not the form or name under which it is invoked — specs are **de-keyed**:
//! the invocation form is an interned [`CallableTypeId`] and the name lives in the
//! library key (normalized) and on the node (invocation spelling). One spec may back
//! several names (flyweight), including the per-type unknown-callable fallback
//! singletons of [`LibraryStack`](crate::library::LibraryStack).
//!
//! The declarative surface is the two structure specs — [`ArgumentStructureSpec`]
//! (arguments *configure*) and [`SlotStructureSpec`] (slots hold *content regions*) —
//! with [`StdCallableSpec`] as the standard implementation. Both are Phase 4 skeletons
//! that grow with their consumers (`ArgumentsParser`/`SlotsParser`, Phase 6), as does the
//! `invocation_parser()` full-takeover escape hatch (DESIGN_RATIONALE.md §3.4).
//!
//! Definition lookup — [`Library`](crate::library::Library) and friends — lives in the
//! neighboring [`library`](crate::library) topic.

mod callable;
mod structure;

pub use callable::{CallableSpec, CallableTypeId, StdCallableSpec};
pub use structure::{
    ArgumentKind, ArgumentSpec, ArgumentStructureSpec, SlotSpec, SlotStructureSpec,
};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::Lang;
    use crate::token::GroupTypeId;
    use alloc::boxed::Box;
    use alloc::string::String;

    #[derive(Debug, Clone, Copy)]
    struct PlainLang;
    impl Lang for PlainLang {
        type StateExt = ();
        type Event = ();
        type SourceOrigin = Option<String>;
        type NodeExts = ();
    }

    #[test]
    fn default_trait_methods_are_the_neutral_callable() {
        #[derive(Debug)]
        struct Bare;
        impl CallableSpec<PlainLang> for Bare {}

        let spec = Bare;
        assert!(CallableSpec::<PlainLang>::arguments(&spec).arguments.is_empty());
        assert!(CallableSpec::<PlainLang>::slots(&spec).slots.is_empty());
    }

    #[test]
    fn std_callable_spec_exposes_its_structures() {
        let braces = GroupTypeId::new(0);
        let brackets = GroupTypeId::new(1);
        let spec = StdCallableSpec::new(
            ArgumentStructureSpec {
                arguments: vec![
                    ArgumentSpec::new(ArgumentKind::Star { marker: "*".into() }),
                    ArgumentSpec::new(ArgumentKind::Optional { group_type: brackets })
                        .named("placement"),
                    ArgumentSpec::new(ArgumentKind::Mandatory { group_type: braces }),
                ],
            },
            SlotStructureSpec { slots: vec![SlotSpec { name: Some(Box::from("body")) }] },
        );

        let dyn_spec: &dyn CallableSpec<PlainLang> = &spec;
        assert_eq!(dyn_spec.arguments().arguments.len(), 3);
        assert_eq!(dyn_spec.arguments().arguments[1].name.as_deref(), Some("placement"));
        assert_eq!(dyn_spec.slots().slots.len(), 1);
    }
}
