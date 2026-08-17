//! The preset's serialization support (see [`techy::serialize`](crate::serialize)):
//! [`Latexlike`] is a [`SerializableLang`]; the preset's vocabulary types
//! ([`CallableType`], [`GroupType`], [`MathGroupForm`], [`Mode`], [`Event`]), its
//! slot ext ([`BodyMarker`]), and its invocation-syntax payload
//! ([`InvocationSyntaxData`], [`StdEnvironmentSyntax`], [`StdEnvironmentSideSyntax`])
//! convert to and from serialized values ([`SerializableValue`] /
//! [`DeserializableValue`], implemented for every language, so a family member
//! ([`LatexlikeLang`]) reusing the types gets the conversions); the preset's callable
//! spec types serialize as objects; and [`register`] prepares a reading session.
//!
//! **How the spec types serialize.** A spec whose data has no serialized form (its
//! argument parsers, its body behavior) — [`MacroSpec`], [`SpecialsSpec`],
//! [`EnvironmentSpec`] — serializes by *identity*: a reference to the provider that
//! defined it plus its key, through the [`SpecProvenance`](crate::core::specs::SpecProvenance)
//! stamp a shared package
//! hands out ([`Package::new_shared`](crate::scopes::Package::new_shared);
//! [`Package::define_macro`](crate::scopes::Package::define_macro) and
//! [`define_environment`](crate::scopes::Package::define_environment) stamp
//! automatically; [`builtin_package`] and the [`minidefs`](super::minidefs) packages
//! are built that way). An unstamped spec of these types cannot be serialized
//! ([`SerializeError::MissingProvenance`] names the type). A spec whose data is
//! plain — [`BeginSpec`] (the terminator command's name), [`InputMacroSpec`] (its
//! two constructor choices), and the stateless [`EndSpec`] and
//! [`ParagraphBreakSpec`] — has a *self-contained* form the reading side rebuilds
//! an equivalent spec from; when a `BeginSpec` or an `InputMacroSpec` carries a
//! stamp it serializes by identity instead, so that reading yields the very instance
//! the reading side's package holds. Reading a stamped spec resolves it in the
//! reading environment's package of that name ([`KnownProviders`]) — the instance
//! the parse got, never a lookup re-run.
//!
//! **Preparing a reading session.** [`register`] registers, on the session's specs
//! and providers tables, the readers of the crate's own types (through
//! [`register_core_readers`]) and of the preset's self-contained spec forms;
//! [`register_package_recipes`] adds the recipe of the [`builtin_package`] to a
//! [`KnownProviders`] (the [`minidefs`](super::minidefs) packages have their own,
//! [`minidefs::register_package_recipes`](super::minidefs::register_package_recipes)).
//! Writing needs no preparation.
//!
//! ```
//! use std::sync::Arc;
//! use techy::core::specs::Package;
//! use techy::core::{Language, ParsingState};
//! use techy::error::Recovery;
//! use techy::latexlike::{self, Latexlike, LatexlikeDriver};
//! use techy::serialize::{KnownProviders, SerdeSession, SerialIndex, TreeSerialization};
//!
//! // A shared package: its definitions are stamped, so they serialize by identity.
//! let defs = Package::<Latexlike>::new_shared("mydefs", |package| {
//!     package.define_macro("emph", ["m"]).unwrap();
//!     package.define_environment("quote", ["o"]).unwrap();
//! });
//! let language: Language<Latexlike> = Language::new(
//!     LatexlikeDriver::new(Recovery::Strict),
//!     ParsingState::lang_initial_with_packages([Arc::clone(&defs)]).expect("seed state"),
//! );
//! let result = language.parse(r"a \emph{b} \begin{quote} c \end{quote}").unwrap();
//!
//! // Write.
//! let mut writer = SerdeSession::<Latexlike>::new();
//! let position = writer.serialize_tree(&result.tree).unwrap();
//! let segment = writer.take_segment();
//!
//! // Read: the environment holds `mydefs` (the very package the parse used) and knows
//! // how to build the builtin package; the readers of the crate's and the preset's
//! // types are registered.
//! let mut known = KnownProviders::<Latexlike>::new();
//! known.insert(Arc::clone(&defs));
//! latexlike::serialize::register_package_recipes(&mut known);
//! let mut reader = SerdeSession::<Latexlike>::new();
//! reader.set_user_data(known);
//! latexlike::serialize::register(&mut reader).unwrap();
//! reader.push_segment(segment).unwrap();
//! let trees = reader.standard_tables().unwrap().trees;
//! let tree = reader.tree::<()>(trees.position(position.index())).unwrap();
//!
//! // The `\emph` node's spec is the instance `mydefs` holds — identity, not a copy.
//! let emph = tree.root().child(1).unwrap();
//! assert_eq!(emph.macro_name(), Some("emph"));
//! assert!(Arc::ptr_eq(emph.spec().unwrap(), defs.get(latexlike::CallableType::Macro, "emph").unwrap()));
//! ```
//!
//! The preset's wire names (not yet frozen — see "Stability of the serialized form"
//! in the [`techy::serialize`](crate::serialize) documentation): callable types
//! `macro` / `environment` / `specials`; group
//! types `content` / `{math: inline | display}` / `verbatim`; modes `text` / `math`;
//! the event `exit-math-context`; the slot ext `{body: bool}`; the invocation
//! syntax `{macro: {escape_char, post_space}}` / `{environment: {begin, end?}}` /
//! `specials`, an environment side `{escape_char, command_word, post_space,
//! name_group_rule: {group_type, open, close}}`; the spec identifiers
//! `latexlike.begin` (`{end_command_name}`), `latexlike.end` (`{}`),
//! `latexlike.paragraph-break` (`{}`), `latexlike.input` (`{persist_state,
//! attached_slot_ext}`).

use alloc::string::String;
use alloc::sync::Arc;

use crate::node::{ArgumentExt, BodySlotExt, SlotExt};
use crate::serialize::wire::{
    data_variant, expect_data_variant, expect_unit_variant, read_variant, FromSerialValue,
    ToSerialValue,
};
use crate::serialize::{
    read_unit_recipe, register_core_readers, serialize_stamped_spec, unit_recipe_value,
    DeserializableObject, DeserializableValue, DeserializeContext, DeserializeError,
    KnownProviders, RegistrationError, SerdeSession, SerialEntry, SerialValue,
    SerializableLang, SerializableObject, SerializableValue, SerializeContext, SerializeError,
};
use crate::source::TextContent;
use crate::spec::CallableSpec;
use crate::state::Lang;
use crate::token::GroupRule;

use super::{
    builtin_package, input_macro_spec, BeginSpec, BodyMarker, CallableType, EndSpec,
    EnvironmentSpec, Event, GroupType, InputMacroSpec, InvocationSyntaxData, Latexlike,
    LatexlikeLang, MacroSpec, MathGroupForm, Mode, ParagraphBreakSpec, SpecialsSpec,
    StdEnvironmentSideSyntax, StdEnvironmentSyntax,
};

// --- the language opts in --------------------------------------------------------------

/// The preset supports serialization: every type it supplies to the parse has its
/// value conversions (below, and the crate's for `()` and `Option<String>`).
impl SerializableLang for Latexlike {}

// --- the vocabulary types ---------------------------------------------------------------

/// The serialized names of the vocabulary values (not yet frozen; see the module docs).
/// The parity of these strings with the serde renames on the enums (under the
/// `serde` feature) is pinned by a test.
mod names {
    pub(super) const MACRO: &str = "macro";
    pub(super) const ENVIRONMENT: &str = "environment";
    pub(super) const SPECIALS: &str = "specials";
    pub(super) const CALLABLE_TYPES: &[&str] = &[MACRO, ENVIRONMENT, SPECIALS];

    pub(super) const CONTENT: &str = "content";
    pub(super) const MATH: &str = "math";
    pub(super) const VERBATIM: &str = "verbatim";
    pub(super) const GROUP_TYPES: &[&str] = &[CONTENT, MATH, VERBATIM];

    pub(super) const INLINE: &str = "inline";
    pub(super) const DISPLAY: &str = "display";
    pub(super) const MATH_GROUP_FORMS: &[&str] = &[INLINE, DISPLAY];

    pub(super) const TEXT: &str = "text";
    pub(super) const MODES: &[&str] = &[TEXT, MATH];

    pub(super) const EXIT_MATH_CONTEXT: &str = "exit-math-context";
    pub(super) const EVENTS: &[&str] = &[EXIT_MATH_CONTEXT];

    pub(super) const MACRO_SYNTAX: &str = "macro";
    pub(super) const ENVIRONMENT_SYNTAX: &str = "environment";
    pub(super) const SPECIALS_SYNTAX: &str = "specials";
    pub(super) const INVOCATION_SYNTAXES: &[&str] = &[MACRO_SYNTAX, ENVIRONMENT_SYNTAX, SPECIALS_SYNTAX];
}

/// A callable type is `"macro"`, `"environment"`, or `"specials"`.
impl<L: Lang> SerializableValue<L> for CallableType {
    fn serialize_value(&self, _cx: &mut SerializeContext<'_, L>) -> Result<SerialValue, SerializeError>
    where
        L: SerializableLang,
    {
        Ok(SerialValue::Str(String::from(match self {
            CallableType::Macro => names::MACRO,
            CallableType::Environment => names::ENVIRONMENT,
            CallableType::Specials => names::SPECIALS,
        })))
    }
}

impl<L: Lang> DeserializableValue<L> for CallableType {
    fn deserialize_value(value: &SerialValue, _cx: &mut DeserializeContext<'_, L>) -> Result<Self, DeserializeError>
    where
        L: SerializableLang,
    {
        let (name, payload) = read_variant(value, "CallableType", names::CALLABLE_TYPES)?;
        expect_unit_variant(name, payload)?;
        Ok(match name {
            names::MACRO => CallableType::Macro,
            names::ENVIRONMENT => CallableType::Environment,
            _ => CallableType::Specials,
        })
    }
}

/// A math group form is `"inline"` or `"display"`.
impl<L: Lang> SerializableValue<L> for MathGroupForm {
    fn serialize_value(&self, _cx: &mut SerializeContext<'_, L>) -> Result<SerialValue, SerializeError>
    where
        L: SerializableLang,
    {
        Ok(SerialValue::Str(String::from(match self {
            MathGroupForm::Inline => names::INLINE,
            MathGroupForm::Display => names::DISPLAY,
        })))
    }
}

impl<L: Lang> DeserializableValue<L> for MathGroupForm {
    fn deserialize_value(value: &SerialValue, _cx: &mut DeserializeContext<'_, L>) -> Result<Self, DeserializeError>
    where
        L: SerializableLang,
    {
        let (name, payload) = read_variant(value, "MathGroupForm", names::MATH_GROUP_FORMS)?;
        expect_unit_variant(name, payload)?;
        Ok(if name == names::INLINE { MathGroupForm::Inline } else { MathGroupForm::Display })
    }
}

/// A group type is `"content"`, `{"math": <form>}`, or `"verbatim"`.
impl<L: Lang> SerializableValue<L> for GroupType {
    fn serialize_value(&self, cx: &mut SerializeContext<'_, L>) -> Result<SerialValue, SerializeError>
    where
        L: SerializableLang,
    {
        Ok(match self {
            GroupType::Content => SerialValue::Str(String::from(names::CONTENT)),
            GroupType::Math(form) => data_variant(names::MATH, form.serialize_value(cx)?),
            GroupType::Verbatim => SerialValue::Str(String::from(names::VERBATIM)),
        })
    }
}

impl<L: Lang> DeserializableValue<L> for GroupType {
    fn deserialize_value(value: &SerialValue, cx: &mut DeserializeContext<'_, L>) -> Result<Self, DeserializeError>
    where
        L: SerializableLang,
    {
        let (name, payload) = read_variant(value, "GroupType", names::GROUP_TYPES)?;
        Ok(match name {
            names::MATH => {
                let form = expect_data_variant(names::MATH, payload)?;
                GroupType::Math(MathGroupForm::deserialize_value(form, cx)?)
            }
            names::CONTENT => {
                expect_unit_variant(names::CONTENT, payload)?;
                GroupType::Content
            }
            _ => {
                expect_unit_variant(names::VERBATIM, payload)?;
                GroupType::Verbatim
            }
        })
    }
}

/// A mode is `"text"` or `"math"`.
impl<L: Lang> SerializableValue<L> for Mode {
    fn serialize_value(&self, _cx: &mut SerializeContext<'_, L>) -> Result<SerialValue, SerializeError>
    where
        L: SerializableLang,
    {
        Ok(SerialValue::Str(String::from(match self {
            Mode::Text => names::TEXT,
            Mode::Math => names::MATH,
        })))
    }
}

impl<L: Lang> DeserializableValue<L> for Mode {
    fn deserialize_value(value: &SerialValue, _cx: &mut DeserializeContext<'_, L>) -> Result<Self, DeserializeError>
    where
        L: SerializableLang,
    {
        let (name, payload) = read_variant(value, "Mode", names::MODES)?;
        expect_unit_variant(name, payload)?;
        Ok(if name == names::TEXT { Mode::Text } else { Mode::Math })
    }
}

/// An event is `"exit-math-context"`.
impl<L: Lang> SerializableValue<L> for Event {
    fn serialize_value(&self, _cx: &mut SerializeContext<'_, L>) -> Result<SerialValue, SerializeError>
    where
        L: SerializableLang,
    {
        Ok(SerialValue::Str(String::from(match self {
            Event::ExitMathContext => names::EXIT_MATH_CONTEXT,
        })))
    }
}

impl<L: Lang> DeserializableValue<L> for Event {
    fn deserialize_value(value: &SerialValue, _cx: &mut DeserializeContext<'_, L>) -> Result<Self, DeserializeError>
    where
        L: SerializableLang,
    {
        let (_, payload) = read_variant(value, "Event", names::EVENTS)?;
        expect_unit_variant(names::EXIT_MATH_CONTEXT, payload)?;
        Ok(Event::ExitMathContext)
    }
}

// --- the slot ext -----------------------------------------------------------------------

/// The body marker's serialized shape.
#[derive(ToSerialValue, FromSerialValue)]
struct WireBodyMarker {
    #[serial(name = "body")]
    body: bool,
}

/// A body marker is `{"body": bool}`.
impl<L: Lang> SerializableValue<L> for BodyMarker {
    fn serialize_value(&self, _cx: &mut SerializeContext<'_, L>) -> Result<SerialValue, SerializeError>
    where
        L: SerializableLang,
    {
        Ok(WireBodyMarker { body: self.is_body() }.to_serial_value()?)
    }
}

impl<L: Lang> DeserializableValue<L> for BodyMarker {
    fn deserialize_value(value: &SerialValue, _cx: &mut DeserializeContext<'_, L>) -> Result<Self, DeserializeError>
    where
        L: SerializableLang,
    {
        let wire = WireBodyMarker::from_serial_value(value)?;
        Ok(if wire.body { BodyMarker::make_body() } else { BodyMarker::not_body() })
    }
}

// --- the invocation syntax --------------------------------------------------------------

/// The macro-formed invocation's spelling facts.
#[derive(ToSerialValue, FromSerialValue)]
struct WireMacroSyntax {
    #[serial(name = "escape_char")]
    escape_char: char,
    /// The post-space, through `TextContent`'s (owned-only) value conversion.
    #[serial(name = "post_space")]
    post_space: SerialValue,
}

/// An invocation syntax is `{"macro": {escape_char, post_space}}`,
/// `{"environment": <the record's own form>}`, or `"specials"`. The post-space is
/// owned text on the wire — the tree writer materializes the invocation syntax
/// against the node's source before converting it (see
/// [`TreeSerdeDriver`](crate::serialize::TreeSerdeDriver)).
impl<L: Lang, Env: SerializableValue<L>> SerializableValue<L> for InvocationSyntaxData<Env> {
    fn serialize_value(&self, cx: &mut SerializeContext<'_, L>) -> Result<SerialValue, SerializeError>
    where
        L: SerializableLang,
    {
        Ok(match self {
            InvocationSyntaxData::Macro { escape_char, post_space } => {
                let wire = WireMacroSyntax { escape_char: *escape_char, post_space: post_space.serialize_value(cx)? };
                data_variant(names::MACRO_SYNTAX, wire.to_serial_value()?)
            }
            InvocationSyntaxData::Environment(env) => data_variant(names::ENVIRONMENT_SYNTAX, env.serialize_value(cx)?),
            InvocationSyntaxData::Specials => SerialValue::Str(String::from(names::SPECIALS_SYNTAX)),
        })
    }
}

impl<L: Lang, Env: DeserializableValue<L>> DeserializableValue<L> for InvocationSyntaxData<Env> {
    fn deserialize_value(value: &SerialValue, cx: &mut DeserializeContext<'_, L>) -> Result<Self, DeserializeError>
    where
        L: SerializableLang,
    {
        let (name, payload) = read_variant(value, "InvocationSyntaxData", names::INVOCATION_SYNTAXES)?;
        Ok(match name {
            names::MACRO_SYNTAX => {
                let wire = WireMacroSyntax::from_serial_value(expect_data_variant(names::MACRO_SYNTAX, payload)?)?;
                InvocationSyntaxData::Macro {
                    escape_char: wire.escape_char,
                    post_space: TextContent::deserialize_value(&wire.post_space, cx)?,
                }
            }
            names::ENVIRONMENT_SYNTAX => {
                let env = expect_data_variant(names::ENVIRONMENT_SYNTAX, payload)?;
                InvocationSyntaxData::Environment(Env::deserialize_value(env, cx)?)
            }
            _ => {
                expect_unit_variant(names::SPECIALS_SYNTAX, payload)?;
                InvocationSyntaxData::Specials
            }
        })
    }
}

/// The standard environment record's serialized shape: both sides through their own
/// conversion, the end side absent when the body closed without a terminator.
#[derive(ToSerialValue, FromSerialValue)]
struct WireEnvironmentSyntax {
    #[serial(name = "begin")]
    begin: SerialValue,
    #[serial(name = "end")]
    end: Option<SerialValue>,
}

/// A standard environment record is `{begin, end?}` — each side a
/// [`StdEnvironmentSideSyntax`] value, `end` omitted when the body closed without
/// consuming a terminator.
impl<L: Lang> SerializableValue<L> for StdEnvironmentSyntax<L> {
    fn serialize_value(&self, cx: &mut SerializeContext<'_, L>) -> Result<SerialValue, SerializeError>
    where
        L: SerializableLang,
    {
        let wire = WireEnvironmentSyntax {
            begin: self.begin.serialize_value(cx)?,
            end: self.end.as_ref().map(|end| end.serialize_value(cx)).transpose()?,
        };
        Ok(wire.to_serial_value()?)
    }
}

impl<L: Lang> DeserializableValue<L> for StdEnvironmentSyntax<L> {
    fn deserialize_value(value: &SerialValue, cx: &mut DeserializeContext<'_, L>) -> Result<Self, DeserializeError>
    where
        L: SerializableLang,
    {
        let wire = WireEnvironmentSyntax::from_serial_value(value)?;
        Ok(StdEnvironmentSyntax {
            begin: StdEnvironmentSideSyntax::deserialize_value(&wire.begin, cx)?,
            end: wire.end.as_ref().map(|end| StdEnvironmentSideSyntax::deserialize_value(end, cx)).transpose()?,
        })
    }
}

/// One side's serialized shape: the text fields through `TextContent`'s (owned-only)
/// value conversion, the name-group rule inlined through `GroupRule`'s.
#[derive(ToSerialValue, FromSerialValue)]
struct WireEnvironmentSideSyntax {
    #[serial(name = "escape_char")]
    escape_char: char,
    #[serial(name = "command_word")]
    command_word: SerialValue,
    #[serial(name = "post_space")]
    post_space: SerialValue,
    #[serial(name = "name_group_rule")]
    name_group_rule: SerialValue,
}

/// One side of a standard environment record is `{escape_char, command_word,
/// post_space, name_group_rule}` — the two texts owned on the wire (materialized by
/// the tree writer), the name-group rule written in full ([`GroupRule`]'s value
/// conversion: `{group_type, open, close}`) and read back into a fresh `Arc` (the
/// rule is source-independent data; sharing with the state's rules is not recorded).
impl<L: Lang> SerializableValue<L> for StdEnvironmentSideSyntax<L> {
    fn serialize_value(&self, cx: &mut SerializeContext<'_, L>) -> Result<SerialValue, SerializeError>
    where
        L: SerializableLang,
    {
        let wire = WireEnvironmentSideSyntax {
            escape_char: self.escape_char,
            command_word: self.command_word.serialize_value(cx)?,
            post_space: self.post_space.serialize_value(cx)?,
            name_group_rule: self.name_group_rule.serialize_value(cx)?,
        };
        Ok(wire.to_serial_value()?)
    }
}

impl<L: Lang> DeserializableValue<L> for StdEnvironmentSideSyntax<L> {
    fn deserialize_value(value: &SerialValue, cx: &mut DeserializeContext<'_, L>) -> Result<Self, DeserializeError>
    where
        L: SerializableLang,
    {
        let wire = WireEnvironmentSideSyntax::from_serial_value(value)?;
        Ok(StdEnvironmentSideSyntax {
            escape_char: wire.escape_char,
            command_word: TextContent::deserialize_value(&wire.command_word, cx)?,
            post_space: TextContent::deserialize_value(&wire.post_space, cx)?,
            name_group_rule: Arc::new(GroupRule::<L>::deserialize_value(&wire.name_group_rule, cx)?),
        })
    }
}

// --- the spec types -----------------------------------------------------------------------

/// The identifier of a `BeginSpec`'s self-contained form.
const BEGIN_IDENTIFIER: &str = "latexlike.begin";
/// The identifier of an `EndSpec`'s self-contained form.
const END_IDENTIFIER: &str = "latexlike.end";
/// The identifier of a `ParagraphBreakSpec`'s self-contained form.
const PARAGRAPH_BREAK_IDENTIFIER: &str = "latexlike.paragraph-break";
/// The identifier of an `InputMacroSpec`'s self-contained form.
const INPUT_IDENTIFIER: &str = "latexlike.input";

/// By identity through its provenance stamp; unstamped is
/// [`SerializeError::MissingProvenance`].
impl<LLL: LatexlikeLang> SerializableObject<LLL> for MacroSpec<LLL> {
    fn serialize_object(&self, cx: &mut SerializeContext<'_, LLL>) -> Result<SerialEntry, SerializeError>
    where
        LLL: SerializableLang,
    {
        serialize_stamped_spec(self.provenance(), "MacroSpec", cx)
    }
}

/// By identity through its provenance stamp; unstamped is
/// [`SerializeError::MissingProvenance`].
impl<LLL: LatexlikeLang> SerializableObject<LLL> for SpecialsSpec<LLL> {
    fn serialize_object(&self, cx: &mut SerializeContext<'_, LLL>) -> Result<SerialEntry, SerializeError>
    where
        LLL: SerializableLang,
    {
        serialize_stamped_spec(self.provenance(), "SpecialsSpec", cx)
    }
}

/// By identity through its provenance stamp; unstamped is
/// [`SerializeError::MissingProvenance`].
impl<LLL: LatexlikeLang> SerializableObject<LLL> for EnvironmentSpec<LLL> {
    fn serialize_object(&self, cx: &mut SerializeContext<'_, LLL>) -> Result<SerialEntry, SerializeError>
    where
        LLL: SerializableLang,
    {
        serialize_stamped_spec(self.provenance(), "EnvironmentSpec", cx)
    }
}

/// A `BeginSpec`'s self-contained form.
#[derive(ToSerialValue, FromSerialValue)]
struct WireBeginSpec {
    #[serial(name = "end_command_name")]
    end_command_name: String,
}

/// By identity when stamped; otherwise the self-contained form
/// `{end_command_name}` under `latexlike.begin`.
impl<LLL: LatexlikeLang> SerializableObject<LLL> for BeginSpec<LLL> {
    fn serialize_object(&self, cx: &mut SerializeContext<'_, LLL>) -> Result<SerialEntry, SerializeError>
    where
        LLL: SerializableLang,
    {
        if let Some(provenance) = self.provenance() {
            return provenance.serialize_object(cx);
        }
        let wire = WireBeginSpec { end_command_name: String::from(self.end_command_name()) };
        Ok(SerialEntry { identifier: BEGIN_IDENTIFIER.into(), data: wire.to_serial_value()? })
    }
}

/// Rebuilt with [`BeginSpec::new`] from the terminator command's name.
impl<LLL: LatexlikeLang + SerializableLang> DeserializableObject<LLL> for BeginSpec<LLL> {
    type Output = BeginSpec<LLL>;

    fn deserialize_object(
        value: &SerialValue,
        _cx: &mut DeserializeContext<'_, LLL>,
    ) -> Result<Self::Output, DeserializeError> {
        let wire = WireBeginSpec::from_serial_value(value)?;
        Ok(BeginSpec::new(wire.end_command_name))
    }
}

/// The empty self-contained form `{}` under `latexlike.end`.
impl<LLL: LatexlikeLang> SerializableObject<LLL> for EndSpec<LLL> {
    fn serialize_object(&self, _cx: &mut SerializeContext<'_, LLL>) -> Result<SerialEntry, SerializeError>
    where
        LLL: SerializableLang,
    {
        Ok(SerialEntry { identifier: END_IDENTIFIER.into(), data: unit_recipe_value() })
    }
}

/// Rebuilt with [`EndSpec::new`].
impl<LLL: LatexlikeLang + SerializableLang> DeserializableObject<LLL> for EndSpec<LLL> {
    type Output = EndSpec<LLL>;

    fn deserialize_object(
        value: &SerialValue,
        _cx: &mut DeserializeContext<'_, LLL>,
    ) -> Result<Self::Output, DeserializeError> {
        read_unit_recipe(value, "EndSpec")?;
        Ok(EndSpec::new())
    }
}

/// The empty self-contained form `{}` under `latexlike.paragraph-break`.
impl<LLL: LatexlikeLang> SerializableObject<LLL> for ParagraphBreakSpec {
    fn serialize_object(&self, _cx: &mut SerializeContext<'_, LLL>) -> Result<SerialEntry, SerializeError>
    where
        LLL: SerializableLang,
    {
        Ok(SerialEntry { identifier: PARAGRAPH_BREAK_IDENTIFIER.into(), data: unit_recipe_value() })
    }
}

/// Rebuilt as the unit value.
impl<LLL: LatexlikeLang + SerializableLang> DeserializableObject<LLL> for ParagraphBreakSpec {
    type Output = ParagraphBreakSpec;

    fn deserialize_object(
        value: &SerialValue,
        _cx: &mut DeserializeContext<'_, LLL>,
    ) -> Result<Self::Output, DeserializeError> {
        read_unit_recipe(value, "ParagraphBreakSpec")?;
        Ok(ParagraphBreakSpec)
    }
}

/// An `InputMacroSpec`'s self-contained form: its two constructor choices.
#[derive(ToSerialValue, FromSerialValue)]
struct WireInputMacroSpec {
    #[serial(name = "persist_state")]
    persist_state: bool,
    /// The attached slot's ext, through the language's slot-ext value conversion.
    #[serial(name = "attached_slot_ext")]
    attached_slot_ext: SerialValue,
}

/// By identity when stamped; otherwise the self-contained form `{persist_state,
/// attached_slot_ext}` under `latexlike.input`.
impl<LLL: LatexlikeLang> SerializableObject<LLL> for InputMacroSpec<LLL> {
    fn serialize_object(&self, cx: &mut SerializeContext<'_, LLL>) -> Result<SerialEntry, SerializeError>
    where
        LLL: SerializableLang,
    {
        if let Some(provenance) = self.provenance() {
            return provenance.serialize_object(cx);
        }
        let wire = WireInputMacroSpec {
            persist_state: self.persist_state(),
            attached_slot_ext: self.attached_slot_ext().serialize_value(cx)?,
        };
        Ok(SerialEntry { identifier: INPUT_IDENTIFIER.into(), data: wire.to_serial_value()? })
    }
}

/// Rebuilt with [`input_macro_spec`] from the two choices.
impl<LLL> DeserializableObject<LLL> for InputMacroSpec<LLL>
where
    LLL: LatexlikeLang + SerializableLang,
    ArgumentExt<LLL>: Default,
{
    type Output = InputMacroSpec<LLL>;

    fn deserialize_object(
        value: &SerialValue,
        cx: &mut DeserializeContext<'_, LLL>,
    ) -> Result<Self::Output, DeserializeError> {
        let wire = WireInputMacroSpec::from_serial_value(value)?;
        let ext = <SlotExt<LLL> as DeserializableValue<LLL>>::deserialize_value(&wire.attached_slot_ext, cx)?;
        Ok(input_macro_spec(wire.persist_state, ext))
    }
}

// --- registration -----------------------------------------------------------------------

/// Prepare a reading session for latexlike data: register, on the session's specs
/// and providers tables, the readers of the crate's own spec and provider types
/// ([`register_core_readers`] — the identity form of stamped specs, packages, scopes,
/// fallback providers, the error spec) and of the preset's self-contained spec forms
/// (`latexlike.begin`, `latexlike.end`, `latexlike.paragraph-break`,
/// `latexlike.input`). Call it once per reading session, after
/// [`SerdeSession::new`]; a writing session needs nothing. The reading environment's
/// providers are a separate matter — a [`KnownProviders`] set as the session's user
/// data (see [`register_package_recipes`]).
///
/// Generic over the language family (`LLL`, [`LatexlikeLang`]) — a family member
/// that opted into serialization prepares its sessions the same way; the bounds are
/// those of the preset's spec types' `CallableSpec` impls (`BeginSpec` marks body
/// slots, `InputMacroSpec` builds argument specs).
///
/// # Errors
///
/// [`RegistrationError::UnknownTableName`] when the session lacks the specs or the
/// providers table; [`RegistrationError::DuplicateIdentifier`] when the readers are
/// already registered (the function, or [`register_core_readers`], was called before
/// on this session).
pub fn register<LLL>(session: &mut SerdeSession<LLL>) -> Result<(), RegistrationError>
where
    LLL: LatexlikeLang + SerializableLang,
    SlotExt<LLL>: BodySlotExt,
    ArgumentExt<LLL>: Default,
{
    register_core_readers(session)?;
    let (specs, _providers) = crate::serialize::spec_and_provider_tables(session)?;
    specs.register_type::<BeginSpec<LLL>>(session, BEGIN_IDENTIFIER, |spec| {
        Arc::new(spec) as Arc<dyn CallableSpec<LLL>>
    })?;
    specs.register_type::<EndSpec<LLL>>(session, END_IDENTIFIER, |spec| {
        Arc::new(spec) as Arc<dyn CallableSpec<LLL>>
    })?;
    specs.register_type::<ParagraphBreakSpec>(session, PARAGRAPH_BREAK_IDENTIFIER, |spec| {
        Arc::new(spec) as Arc<dyn CallableSpec<LLL>>
    })?;
    specs.register_type::<InputMacroSpec<LLL>>(session, INPUT_IDENTIFIER, |spec| {
        Arc::new(spec) as Arc<dyn CallableSpec<LLL>>
    })?;
    Ok(())
}

/// Register the recipe of the preset's seed package on `known`: a serialized
/// reference to `_builtin` then resolves to a package built by [`builtin_package`]
/// (once per reading session and serialized entry) — unless a provider of that name
/// was inserted, which takes precedence. A reading program whose own parses use a
/// seed state can insert that state's builtin package instead, so that read data
/// shares the very instances its parses use. The [`minidefs`](super::minidefs)
/// packages have their own [`minidefs::register_package_recipes`](super::minidefs::register_package_recipes).
///
/// The name `_builtin` (like `minilatex` and `minilatex.item`) is part of the
/// preset's serialized vocabulary and is kept stable like an identifier: a serialized
/// package refers to its package by name.
pub fn register_package_recipes<LLL>(known: &mut KnownProviders<LLL>)
where
    LLL: LatexlikeLang,
    SlotExt<LLL>: BodySlotExt,
{
    known.register_recipe("_builtin", builtin_package::<LLL>);
}
