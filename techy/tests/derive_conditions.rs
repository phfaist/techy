//! End-to-end exercise of `#[derive(DiagnosticInfo)]` and `#[derive(ToDiagnosticValue)]`
//! from a consumer crate (ARCHITECTURE.md "Condition declaration via derive";
//! DESIGN_RATIONALE.md [§dd-dr:errors]). This file plays the third-party role: conditions declared
//! here go through exactly the same surface a downstream language crate would use.

use core::fmt;
use std::sync::Arc;

use techy::error::{Diagnostic, DiagnosticInfo, DiagnosticValue, ToDiagnosticValue};
use techy::source::{Source, SourceSpan};

/// A payload enum, serialized as its kebab-cased variant name.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ToDiagnosticValue)]
enum SampleKind {
    EndOfInput,
    StrayGroupClose,
    EOFMarker,
}

/// The common case: message format string, generated constructor, mixed field types.
#[derive(Debug, Clone, PartialEq, Eq, DiagnosticInfo)]
#[non_exhaustive]
#[diagnostic(
    id = "test.derive.sample",
    message = "sample ‘{name}’ marker {marker} count {count:04}, ‘{name}’ again {{literal}}"
)]
struct Sample {
    name: String,
    marker: char,
    count: u32,
    note: Option<String>,
    tags: Vec<String>,
}

/// Wording needs a match on the enum field → hand-written `Display`, bespoke
/// constructor via `no_constructor`.
#[derive(Debug, Clone, PartialEq, Eq, DiagnosticInfo)]
#[non_exhaustive]
#[diagnostic(id = "test.derive.kinded", no_constructor)]
struct Kinded {
    kind: SampleKind,
}

impl Kinded {
    fn for_kind(kind: SampleKind) -> Kinded {
        Kinded { kind }
    }
}

impl fmt::Display for Kinded {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.kind {
            SampleKind::EndOfInput => write!(f, "ended at end of input"),
            SampleKind::StrayGroupClose => write!(f, "ended at a stray group close"),
            SampleKind::EOFMarker => write!(f, "ended at an EOF marker"),
        }
    }
}

/// A payload-free condition.
#[derive(Debug, Clone, Copy, PartialEq, Eq, DiagnosticInfo)]
#[non_exhaustive]
#[diagnostic(id = "test.derive.unit", message = "a fixed wording")]
struct UnitCondition;

fn sample() -> Sample {
    Sample::new(
        "alpha",
        '!',
        5u32,
        Some(String::from("hint")),
        vec![String::from("a"), String::from("b")],
    )
}

#[test]
fn generated_identifier() {
    assert_eq!(Sample::IDENTIFIER, "test.derive.sample");
    assert_eq!(Kinded::IDENTIFIER, "test.derive.kinded");
    assert_eq!(UnitCondition::IDENTIFIER, "test.derive.unit");
}

#[test]
fn generated_display_interpolates_named_fields() {
    assert_eq!(
        sample().to_string(),
        "sample ‘alpha’ marker ! count 0005, ‘alpha’ again {literal}"
    );
    assert_eq!(UnitCondition.to_string(), "a fixed wording");
}

#[test]
fn hand_written_display_coexists() {
    assert_eq!(Kinded::for_kind(SampleKind::StrayGroupClose).to_string(),
        "ended at a stray group close");
}

#[test]
fn generated_constructor_accepts_into() {
    // `impl Into<FieldType>` params: &str for String, plain values elsewhere, `None`
    // for an Option field.
    let condition = Sample::new("x", '?', 1u32, None, Vec::<String>::new());
    assert_eq!(condition.name, "x");
    assert_eq!(condition.note, None);
    let unit = UnitCondition::new();
    assert_eq!(unit, UnitCondition);
}

#[test]
fn generated_serializable_data_maps_every_field() {
    assert_eq!(
        DiagnosticInfo::serializable_data(&sample()),
        DiagnosticValue::Map(vec![
            ("name".into(), DiagnosticValue::Str("alpha".into())),
            ("marker".into(), DiagnosticValue::Str("!".into())),
            ("count".into(), DiagnosticValue::Int(5)),
            ("note".into(), DiagnosticValue::Str("hint".into())),
            (
                "tags".into(),
                DiagnosticValue::List(vec![
                    DiagnosticValue::Str("a".into()),
                    DiagnosticValue::Str("b".into()),
                ])
            ),
        ])
    );
    // `None` serializes as Null (the key stays, schema-stable).
    let bare = Sample::new("x", '?', 1u32, None, Vec::<String>::new());
    match DiagnosticInfo::serializable_data(&bare) {
        DiagnosticValue::Map(entries) => {
            assert_eq!(entries[3], ("note".into(), DiagnosticValue::Null));
        }
        other => panic!("expected a map, got {other:?}"),
    }
    assert_eq!(
        DiagnosticInfo::serializable_data(&UnitCondition),
        DiagnosticValue::empty_map()
    );
}

#[test]
fn enum_derive_serializes_kebab_cased_variant_names() {
    assert_eq!(
        SampleKind::EndOfInput.to_diagnostic_value(),
        DiagnosticValue::Str("end-of-input".into())
    );
    assert_eq!(
        SampleKind::StrayGroupClose.to_diagnostic_value(),
        DiagnosticValue::Str("stray-group-close".into())
    );
    assert_eq!(
        SampleKind::EOFMarker.to_diagnostic_value(),
        DiagnosticValue::Str("eof-marker".into())
    );
    assert_eq!(
        DiagnosticInfo::serializable_data(&Kinded::for_kind(SampleKind::EOFMarker)),
        DiagnosticValue::Map(vec![(
            "kind".into(),
            DiagnosticValue::Str("eof-marker".into())
        )])
    );
}

#[test]
fn migrated_core_conditions_serialize_their_payload() {
    // The built-in conditions use the derive (migration, July 2026): their
    // `serializable_data()` now emits every payload field instead of the empty-map
    // default — including payload enums via `#[derive(ToDiagnosticValue)]`.
    let unresolvable = techy::UnresolvableCommand::new("frac", '\\', None);
    assert_eq!(
        DiagnosticInfo::serializable_data(&unresolvable),
        DiagnosticValue::Map(vec![
            ("name".into(), DiagnosticValue::Str("frac".into())),
            ("escape_char".into(), DiagnosticValue::Str("\\".into())),
            ("detail".into(), DiagnosticValue::Null),
        ])
    );

    let unresolvable =
        techy::UnresolvableCommand::new("frac", '\\', "searched libraries x, y".to_string());
    assert_eq!(
        DiagnosticInfo::serializable_data(&unresolvable),
        DiagnosticValue::Map(vec![
            ("name".into(), DiagnosticValue::Str("frac".into())),
            ("escape_char".into(), DiagnosticValue::Str("\\".into())),
            ("detail".into(), DiagnosticValue::Str("searched libraries x, y".into())),
        ])
    );

    let missing = techy::MissingEnvironmentTerminator::new(
        "itemize",
        techy::MissingTerminatorFound::EndOfInput,
    );
    assert_eq!(
        DiagnosticInfo::serializable_data(&missing),
        DiagnosticValue::Map(vec![
            ("environment".into(), DiagnosticValue::Str("itemize".into())),
            ("found".into(), DiagnosticValue::Str("end-of-input".into())),
        ])
    );
}

#[test]
fn derived_condition_flows_through_diagnostic_carriers() {
    let source: Arc<Source> = Arc::new(Source::new("hello"));
    let diagnostic = Diagnostic::error(sample(), SourceSpan::new(&source, 0..2));
    assert_eq!(diagnostic.identifier(), Sample::IDENTIFIER);
    let payload = diagnostic.data().downcast_ref::<Sample>().unwrap();
    assert_eq!(payload.count, 5);
}
