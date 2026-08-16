//! The state driver: [`StateSerdeDriver`], the driver of the states table, and
//! [`StateIndex`], its position type; the [`SerializableObject`] /
//! [`DeserializableObject`] impls of [`ParsingState`], which the driver delegates to.

use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::fmt;
use core::marker::PhantomData;

use crate::scopes::ScopeStack;
use crate::state::{FeaturePresence, Lang, LangFeatures, ParsingState, StateData};
use crate::token::{
    CommandRule, CommandRules, CommentRule, CommentRules, ForbiddenCharsRules, GroupRule,
    GroupRules, ParagraphRules, SpecialsRules, TokenRules, WhitespaceRules,
};

use super::super::engine::{DeserializeContext, ObjectSerdeDriver, SerializeContext};
use super::super::error::{DeserializeError, SerializeError};
use super::super::object::{
    DeserializableObject, DeserializableValue, SerializableLang, SerializableObject,
    SerializableValue,
};
use super::super::value::{SerialEntry, SerialValue};
use super::super::wire::state::{
    WireCommandRule, WireCommandRules, WireCommentRule, WireCommentRules,
    WireForbiddenCharsRules, WireGroupRule, WireGroupRules, WireParagraphRules,
    WireSpecialsRules, WireState, WireTokenRules, WireWhitespaceRules,
};
use super::super::wire::{FromSerialValue, ToSerialValue};
use super::standard::{StandardTableInterning, StandardTableReading};
use super::{STATES_TABLE, STATE_IDENTIFIER};

crate::serial_index! {
    /// A position in the states table — the `Index` type of [`StateSerdeDriver`]:
    /// the serialized reference to a [`ParsingState`].
    pub struct StateIndex;
}

/// The driver of the states table (table name `states`, one kind of object —
/// identifier `core.state`): how a [`ParsingState`] is serialized and rebuilt.
///
/// A state's entry carries its token rules in full — one section per feature the
/// language declares present (whitespace, paragraphs, groups, commands, comments,
/// specials, forbidden characters), each with its gate and its rules data; a feature
/// the language declares absent has no section — its mode and its ext (through the
/// language's `ModeId` and `StateExt` value conversions), and its scope stack as the
/// positions of its providers in the providers table, outermost first. The derived
/// caches of a state (the delimiter prefix table, the specials trigger characters)
/// are not written: the reading side rebuilds them when it freezes the state.
///
/// Reading refuses what the language cannot hold: a section for a feature the
/// reading language declares absent, or a non-empty scope stack for a language
/// without the scope stack, is [`DeserializeError::FeatureAbsent`]; a missing section
/// for a present feature reads as that feature's empty rules (every section is
/// optional in the serialized form).
/// A state is written once however many nodes refer to it, and read back as one
/// `Arc<ParsingState>` — sharing survives the round trip; a state serialized on its
/// own, without any tree, is an ordinary entry.
///
/// The driver delegates to `ParsingState`'s own
/// [`SerializableObject`] and [`DeserializableObject`] impls. Registered by
/// [`SerdeSession::new`](crate::serialize::SerdeSession::new).
pub struct StateSerdeDriver<L: Lang> {
    lang: PhantomData<fn() -> L>,
}

impl<L: Lang> StateSerdeDriver<L> {
    /// The driver (it has no configuration).
    pub fn new() -> StateSerdeDriver<L> {
        StateSerdeDriver { lang: PhantomData }
    }
}

impl<L: Lang> Default for StateSerdeDriver<L> {
    fn default() -> Self {
        StateSerdeDriver::new()
    }
}

impl<L: Lang> fmt::Debug for StateSerdeDriver<L> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("StateSerdeDriver").finish()
    }
}

impl<L: SerializableLang> ObjectSerdeDriver<L> for StateSerdeDriver<L> {
    type Object = ParsingState<L>;
    type Index = StateIndex;

    fn table_name(&self) -> &'static str {
        STATES_TABLE
    }

    fn homogeneous_identifier(&self) -> Option<&'static str> {
        Some(STATE_IDENTIFIER)
    }

    fn serialize_object(
        &self,
        state: &Arc<ParsingState<L>>,
        cx: &mut SerializeContext<'_, L>,
    ) -> Result<SerialEntry, SerializeError> {
        state.serialize_object(cx)
    }

    fn deserialize_object(
        &self,
        entry: &SerialEntry,
        cx: &mut DeserializeContext<'_, L>,
    ) -> Result<Arc<ParsingState<L>>, DeserializeError> {
        ParsingState::<L>::deserialize_object(&entry.data, cx).map(Arc::new)
    }
}

// --- writing ----------------------------------------------------------------------------

/// The store of feature `P` as a wire section, when the feature is present.
fn section<P: FeaturePresence, B: Clone + fmt::Debug + Send + Sync, W>(
    store: &P::Store<B>,
    write: impl FnOnce(&B) -> Result<W, SerializeError>,
) -> Result<Option<W>, SerializeError> {
    P::store_get(store).map(write).transpose()
}

fn wire_group_rule<L: SerializableLang>(
    rule: &GroupRule<L>,
    cx: &mut SerializeContext<'_, L>,
) -> Result<WireGroupRule, SerializeError> {
    Ok(WireGroupRule {
        group_type: rule.group_type.serialize_value(cx)?,
        open: rule.open.clone(),
        close: rule.close.clone(),
    })
}

fn wire_group_rules<L: SerializableLang>(
    rules: &[Arc<GroupRule<L>>],
    cx: &mut SerializeContext<'_, L>,
) -> Result<Vec<WireGroupRule>, SerializeError> {
    rules.iter().map(|rule| wire_group_rule(rule, cx)).collect()
}

/// A state's serialized form: its token rules (one optional section per feature),
/// mode, ext, and scope stack (its providers interned into the providers table, in
/// stack order) — see [`StateSerdeDriver`]. Identifier `core.state`.
impl<L: Lang> SerializableObject<L> for ParsingState<L> {
    fn serialize_object(&self, cx: &mut SerializeContext<'_, L>) -> Result<SerialEntry, SerializeError>
    where
        L: SerializableLang,
    {
        type Features<L> = <L as Lang>::Features;
        let rules = self.rules();
        let wire_rules = WireTokenRules {
            whitespace: section::<<Features<L> as LangFeatures>::Whitespace, _, _>(&rules.whitespace, |block| {
                Ok(WireWhitespaceRules { enabled: block.enabled, chars: String::from(&*block.chars) })
            })?,
            paragraphs: section::<<Features<L> as LangFeatures>::Paragraphs, _, _>(&rules.paragraphs, |block| {
                Ok(WireParagraphRules { enabled: block.enabled })
            })?,
            groups: section::<<Features<L> as LangFeatures>::Groups, _, _>(&rules.groups, |block| {
                Ok(WireGroupRules {
                    enabled: block.enabled,
                    rules: wire_group_rules(&block.rules, cx)?,
                    temporary: wire_group_rules(&block.temporary, cx)?,
                    expecting_close: block
                        .expecting_close
                        .as_ref()
                        .map(|rule| wire_group_rule(rule, cx))
                        .transpose()?,
                })
            })?,
            commands: section::<<Features<L> as LangFeatures>::Commands, _, _>(&rules.commands, |block| {
                Ok(WireCommandRules {
                    enabled: block.enabled,
                    rules: block
                        .rules
                        .iter()
                        .map(|rule| WireCommandRule {
                            escape_char: rule.escape_char,
                            name_chars: rule.name_chars.clone(),
                        })
                        .collect(),
                })
            })?,
            comments: section::<<Features<L> as LangFeatures>::Comments, _, _>(&rules.comments, |block| {
                Ok(WireCommentRules {
                    enabled: block.enabled,
                    rules: block.rules.iter().map(|rule| WireCommentRule { start: rule.start.clone() }).collect(),
                })
            })?,
            specials: section::<<Features<L> as LangFeatures>::Specials, _, _>(&rules.specials, |block| {
                Ok(WireSpecialsRules { enabled: block.enabled })
            })?,
            forbidden_chars: section::<<Features<L> as LangFeatures>::ForbiddenChars, _, _>(
                &rules.forbidden_chars,
                |block| Ok(WireForbiddenCharsRules { chars: String::from(&*block.chars) }),
            )?,
        };
        let scopes = self
            .scopes()
            .providers()
            .iter()
            .map(|provider| cx.intern_provider(provider))
            .collect::<Result<Vec<_>, _>>()?;
        let wire = WireState {
            rules: wire_rules,
            mode: self.mode().serialize_value(cx)?,
            ext: self.ext().serialize_value(cx)?,
            scopes,
        };
        Ok(SerialEntry { identifier: STATE_IDENTIFIER.into(), data: wire.to_serial_value()? })
    }
}

// --- reading ----------------------------------------------------------------------------

/// Install the wire section `wire` of feature `P` into `store`: a present feature
/// takes the section (or keeps its empty rules when the section is missing); an
/// absent feature refuses a section ([`DeserializeError::FeatureAbsent`]).
fn install<P: FeaturePresence, B: Clone + fmt::Debug + Send + Sync, W>(
    store: &mut P::Store<B>,
    wire: Option<W>,
    feature: &'static str,
    read: impl FnOnce(W) -> Result<B, DeserializeError>,
) -> Result<(), DeserializeError> {
    match (P::store_get_mut(store), wire) {
        (Some(block), Some(wire)) => {
            *block = read(wire)?;
            Ok(())
        }
        (Some(_), None) | (None, None) => Ok(()),
        (None, Some(_)) => Err(DeserializeError::FeatureAbsent { feature }),
    }
}

fn read_group_rule<L: SerializableLang>(
    wire: WireGroupRule,
    cx: &mut DeserializeContext<'_, L>,
) -> Result<GroupRule<L>, DeserializeError> {
    Ok(GroupRule {
        group_type: <L::GroupTypeId as DeserializableValue<L>>::deserialize_value(&wire.group_type, cx)?,
        open: wire.open,
        close: wire.close,
    })
}

fn read_group_rules<L: SerializableLang>(
    wire: Vec<WireGroupRule>,
    cx: &mut DeserializeContext<'_, L>,
) -> Result<Vec<Arc<GroupRule<L>>>, DeserializeError> {
    wire.into_iter().map(|rule| read_group_rule(rule, cx).map(Arc::new)).collect()
}

/// A state is rebuilt from its serialized form (see [`StateSerdeDriver`]) and frozen
/// — its derived caches are recomputed; the language's transition customizer does
/// not run again (the serialized data is a state that already passed it).
///
/// The expected group close, when the entry has one, is the same shared handle as
/// the value-equal rule of the rebuilt `temporary` or `rules` list when there is one
/// (the scoped-lifecycle check of [`ParsingState::derived`] compares by handle
/// identity), and a fresh handle otherwise.
impl<L: SerializableLang> DeserializableObject<L> for ParsingState<L> {
    type Output = ParsingState<L>;

    fn deserialize_object(
        value: &SerialValue,
        cx: &mut DeserializeContext<'_, L>,
    ) -> Result<ParsingState<L>, DeserializeError> {
        type Features<L> = <L as Lang>::Features;
        let wire = WireState::from_serial_value(value)?;
        let WireTokenRules { whitespace, paragraphs, groups, commands, comments, specials, forbidden_chars } =
            wire.rules;

        let mut rules = TokenRules::<L>::empty();
        install::<<Features<L> as LangFeatures>::Whitespace, _, _>(
            &mut rules.whitespace,
            whitespace,
            "whitespace",
            |wire| Ok(WhitespaceRules { enabled: wire.enabled, chars: wire.chars.into() }),
        )?;
        install::<<Features<L> as LangFeatures>::Paragraphs, _, _>(
            &mut rules.paragraphs,
            paragraphs,
            "paragraphs",
            |wire| Ok(ParagraphRules { enabled: wire.enabled }),
        )?;
        install::<<Features<L> as LangFeatures>::Groups, _, _>(&mut rules.groups, groups, "groups", |wire| {
            let group_rules = read_group_rules(wire.rules, cx)?;
            let temporary = read_group_rules(wire.temporary, cx)?;
            let expecting_close = match wire.expecting_close {
                None => None,
                Some(wire) => {
                    let rule = read_group_rule(wire, cx)?;
                    // Reuse the value-equal shared handle when the state has one — the
                    // temporary list first (the scoped-lifecycle check keys on it), then
                    // the delimiter table.
                    let shared = temporary.iter().chain(group_rules.iter()).find(|candidate| ***candidate == rule);
                    Some(shared.cloned().unwrap_or_else(|| Arc::new(rule)))
                }
            };
            Ok(GroupRules { enabled: wire.enabled, rules: group_rules, temporary, expecting_close })
        })?;
        install::<<Features<L> as LangFeatures>::Commands, _, _>(&mut rules.commands, commands, "commands", |wire| {
            Ok(CommandRules {
                enabled: wire.enabled,
                rules: wire
                    .rules
                    .into_iter()
                    .map(|rule| Arc::new(CommandRule { escape_char: rule.escape_char, name_chars: rule.name_chars }))
                    .collect(),
            })
        })?;
        install::<<Features<L> as LangFeatures>::Comments, _, _>(&mut rules.comments, comments, "comments", |wire| {
            Ok(CommentRules {
                enabled: wire.enabled,
                rules: wire.rules.into_iter().map(|rule| Arc::new(CommentRule { start: rule.start })).collect(),
            })
        })?;
        install::<<Features<L> as LangFeatures>::Specials, _, _>(
            &mut rules.specials,
            specials,
            "specials",
            |wire| Ok(SpecialsRules { enabled: wire.enabled }),
        )?;
        install::<<Features<L> as LangFeatures>::ForbiddenChars, _, _>(
            &mut rules.forbidden_chars,
            forbidden_chars,
            "forbidden_chars",
            |wire| Ok(ForbiddenCharsRules { chars: wire.chars.into() }),
        )?;

        let providers = wire
            .scopes
            .iter()
            .map(|position| cx.provider(*position))
            .collect::<Result<Vec<_>, _>>()?;
        let scopes = ScopeStack::from_providers(providers)
            .ok_or(DeserializeError::FeatureAbsent { feature: "scopes" })?;
        let mode = <L::ModeId as DeserializableValue<L>>::deserialize_value(&wire.mode, cx)?;
        let ext = <L::StateExt as DeserializableValue<L>>::deserialize_value(&wire.ext, cx)?;
        Ok(ParsingState::new(StateData { rules, scopes, mode, ext }))
    }
}
