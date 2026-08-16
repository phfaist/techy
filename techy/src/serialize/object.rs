//! The capability traits: [`SerializableObject`] / [`DeserializableObject`] for objects
//! (table entries referred to by position), [`SerializableValue`] /
//! [`DeserializableValue`] for values (data embedded inline in an entry), and the
//! [`SerializableLang`] declaration that makes all four usable for a language.

use alloc::borrow::Cow;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

use crate::state::{Lang, NodeExtTypes};

use super::engine::{DeserializeContext, SerializeContext};
use super::error::{DeserializeError, SerialValueError, SerializeError};
use super::value::{SerialEntry, SerialValue};

/// A language that supports serialization. Implementing this trait for a [`Lang`]
/// is what makes serialization and deserialization available for that language: a
/// [`SerializeContext`] or [`DeserializeContext`] can only exist for a
/// `SerializableLang`, so the serialization methods on the traits below — bounded
/// `where L: SerializableLang` — can be called exactly for such languages.
///
/// The trait has no items of its own. Its bounds require every type the language
/// plugs into the parse — its closed vocabularies ([`ModeId`](Lang::ModeId),
/// [`CallableTypeId`](Lang::CallableTypeId), [`GroupTypeId`](Lang::GroupTypeId),
/// [`Event`](Lang::Event)), its extension types ([`StateExt`](Lang::StateExt),
/// [`SessionExt`](Lang::SessionExt), the node-ext bundle [`NodeExts`](Lang::NodeExts)),
/// its [`SourceOrigin`](Lang::SourceOrigin), and its
/// [`InvocationSyntax`](Lang::InvocationSyntax) — to implement [`SerializableValue`]
/// and [`DeserializableValue`] for the language: those are the conversions the
/// crate's own serialization uses whenever such a value is embedded in an entry (a
/// state's mode and ext, a group rule's group type, a source's origin, …). The
/// implementation of every such type is what its owner writes; the impl of this
/// trait itself is empty. A language built on the crate's defaults (`()` for the
/// exts and events, `u32` ids, `Option<String>` origin — what
/// [`TrivialLang`](crate::state::TrivialLang) supplies) opts in with an empty impl
/// block, the core impls covering every type; a language with its own vocabulary or
/// ext types implements the two value traits for those types first.
pub trait SerializableLang:
    Lang<
        ModeId: SerializableValue<Self> + DeserializableValue<Self>,
        CallableTypeId: SerializableValue<Self> + DeserializableValue<Self>,
        GroupTypeId: SerializableValue<Self> + DeserializableValue<Self>,
        Event: SerializableValue<Self> + DeserializableValue<Self>,
        StateExt: SerializableValue<Self> + DeserializableValue<Self>,
        SessionExt: SerializableValue<Self> + DeserializableValue<Self>,
        SourceOrigin: SerializableValue<Self> + DeserializableValue<Self>,
        NodeExts: NodeExtTypes<
            NodeExt: SerializableValue<Self> + DeserializableValue<Self>,
            ArgumentExt: SerializableValue<Self> + DeserializableValue<Self>,
            SlotExt: SerializableValue<Self> + DeserializableValue<Self>,
        >,
        InvocationSyntax: SerializableValue<Self> + DeserializableValue<Self>,
    >
{
}

/// The write side of the serialization capability: an object that can produce its
/// serialized form.
///
/// Every [`CallableSpec`](crate::spec::CallableSpec) and every
/// [`SpecsProvider`](crate::scopes::SpecsProvider) implements this trait — it is a
/// supertrait of both, so that the method is callable through their trait objects
/// (`dyn CallableSpec<L>`, `dyn SpecsProvider<L>`), where the concrete type is
/// unknown. The method is defaulted to "unsupported", so a type that does not
/// participate in serialization writes exactly one line:
///
/// ```
/// use techy::core::specs::CallableSpec;
/// use techy::latexlike::Latexlike;
/// use techy::serialize::SerializableObject;
///
/// #[derive(Debug)]
/// struct MySpec;
///
/// // `MySpec` does not participate in serialization: the empty impl says so.
/// impl SerializableObject<Latexlike> for MySpec {}
///
/// impl CallableSpec<Latexlike> for MySpec {}
/// ```
///
/// A participating type overrides [`serialize_object`](Self::serialize_object). The
/// method is available only when the language is a [`SerializableLang`]: for any
/// other language it cannot be called (and no context value exists to call it with).
pub trait SerializableObject<L: Lang> {
    /// Produce this object's serialized form: its identifier and its data as a
    /// [`SerialEntry`]. `cx` gives the call access to the state of the serialization
    /// in progress.
    ///
    /// # Errors
    ///
    /// The default reports [`SerializeError::Unsupported`]: the type does not
    /// participate in serialization. An implementation returns an error when it
    /// cannot produce its serialized form.
    fn serialize_object(
        &self,
        cx: &mut SerializeContext<'_, L>,
    ) -> Result<SerialEntry, SerializeError>
    where
        L: SerializableLang,
    {
        let _ = cx;
        Err(SerializeError::unsupported())
    }
}

/// The read side of the serialization capability: a type that can rebuild an object
/// from its serialized data. Opt-in and implemented by concrete types only: it is
/// never a supertrait (its associated type and its constructor — a function without a
/// `self` argument — would make the spec/provider traits unusable as trait objects),
/// and a type that does not participate implements nothing.
///
/// [`Output`](Self::Output) is what the read produces: the type itself for a type
/// rebuilt from a self-contained description, or a shared handle to an already
/// existing object (`Arc<dyn …>`) for a type that is looked up in the *reading
/// environment* — the live objects the deserializing program already holds (see the
/// [module documentation](crate::serialize)) — rather than rebuilt.
pub trait DeserializableObject<L: SerializableLang>: Sized {
    /// What [`deserialize_object`](Self::deserialize_object) produces.
    type Output;

    /// Rebuild an object from its serialized `value`. `cx` gives the call access to
    /// the state of the deserialization in progress.
    ///
    /// # Errors
    ///
    /// `value` is untrusted input: a value of the wrong shape, an index out of range,
    /// or a reference to an object the reading environment (the live objects the
    /// deserializing program already holds) lacks is an error, never a panic.
    fn deserialize_object(
        value: &SerialValue,
        cx: &mut DeserializeContext<'_, L>,
    ) -> Result<Self::Output, DeserializeError>;
}

/// The write side of the serialization capability for *values*: data embedded
/// inline in an entry, as opposed to an *object*, which is a table entry referred to
/// by position (see the [module documentation](crate::serialize)). A language's mode,
/// its state ext, a group rule's group type, a source's origin are values: they are
/// written into the entry of the state or source that carries them, not interned in a
/// table of their own.
///
/// Implemented by the owner of the type, for every language: the crate implements it
/// for `()`, `bool`, the integer types, `String`, `Option<T>` and `Vec<T>` (over an
/// implementing `T`) — the types a language built on the crate's defaults plugs in;
/// a language implements it for its own vocabulary and ext types (typically as
/// `impl<L: Lang> SerializableValue<L> for MyMode`, so that any language reusing the
/// type gets the conversion). [`SerializableLang`] requires it of every type the
/// language plugs into the parse. The method is available only when the language is
/// a [`SerializableLang`], like [`SerializableObject::serialize_object`].
pub trait SerializableValue<L: Lang> {
    /// Produce this value's serialized form. `cx` gives the call access to the state
    /// of the serialization in progress — a value that refers to a table object
    /// (a span's source, say) interns it through `cx` and embeds the position.
    ///
    /// # Errors
    ///
    /// The value cannot be represented in the serialized form (an integer outside
    /// `i64`, an implementation's own reason).
    fn serialize_value(&self, cx: &mut SerializeContext<'_, L>) -> Result<SerialValue, SerializeError>
    where
        L: SerializableLang;
}

/// The read side of the serialization capability for *values*: rebuilding a value
/// embedded inline in an entry (see [`SerializableValue`] for what a value is, and
/// which types the crate implements this for).
pub trait DeserializableValue<L: Lang>: Sized {
    /// Rebuild the value from its serialized form. `cx` gives the call access to the
    /// state of the deserialization in progress.
    ///
    /// # Errors
    ///
    /// `value` is untrusted input: a value of the wrong kind or shape, an integer out
    /// of range, or a reference that cannot be read is an error, never a panic.
    fn deserialize_value(
        value: &SerialValue,
        cx: &mut DeserializeContext<'_, L>,
    ) -> Result<Self, DeserializeError>
    where
        L: SerializableLang;
}

// --- core value impls -----------------------------------------------------------------

fn value_mismatch(expected: &'static str, found: &SerialValue) -> DeserializeError {
    DeserializeError::Value(SerialValueError::TypeMismatch {
        expected: Cow::Borrowed(expected),
        found: found.kind_name(),
    })
}

/// `()` is [`Null`](SerialValue::Null).
impl<L: Lang> SerializableValue<L> for () {
    fn serialize_value(&self, _cx: &mut SerializeContext<'_, L>) -> Result<SerialValue, SerializeError>
    where
        L: SerializableLang,
    {
        Ok(SerialValue::Null)
    }
}

/// `()` is [`Null`](SerialValue::Null); any other value is an error.
impl<L: Lang> DeserializableValue<L> for () {
    fn deserialize_value(
        value: &SerialValue,
        _cx: &mut DeserializeContext<'_, L>,
    ) -> Result<Self, DeserializeError>
    where
        L: SerializableLang,
    {
        match value {
            SerialValue::Null => Ok(()),
            other => Err(value_mismatch("null (the unit value)", other)),
        }
    }
}

/// A `bool` is a [`Bool`](SerialValue::Bool).
impl<L: Lang> SerializableValue<L> for bool {
    fn serialize_value(&self, _cx: &mut SerializeContext<'_, L>) -> Result<SerialValue, SerializeError>
    where
        L: SerializableLang,
    {
        Ok(SerialValue::Bool(*self))
    }
}

/// A `bool` is a [`Bool`](SerialValue::Bool); any other value is an error.
impl<L: Lang> DeserializableValue<L> for bool {
    fn deserialize_value(
        value: &SerialValue,
        _cx: &mut DeserializeContext<'_, L>,
    ) -> Result<Self, DeserializeError>
    where
        L: SerializableLang,
    {
        match value {
            SerialValue::Bool(b) => Ok(*b),
            other => Err(value_mismatch("a boolean", other)),
        }
    }
}

/// Integers of every width are an [`Int`](SerialValue::Int): written when the value
/// fits `i64` (an error otherwise — never a truncation), read with a range check
/// against the target type.
macro_rules! int_value_impls {
    ($($t:ty),* $(,)?) => {$(
        impl<L: Lang> SerializableValue<L> for $t {
            fn serialize_value(
                &self,
                _cx: &mut SerializeContext<'_, L>,
            ) -> Result<SerialValue, SerializeError>
            where
                L: SerializableLang,
            {
                i64::try_from(*self).map(SerialValue::Int).map_err(|_| {
                    SerializeError::Value(SerialValueError::IntegerOutOfRange {
                        value: self.to_string(),
                        target: "i64",
                    })
                })
            }
        }

        impl<L: Lang> DeserializableValue<L> for $t {
            fn deserialize_value(
                value: &SerialValue,
                _cx: &mut DeserializeContext<'_, L>,
            ) -> Result<Self, DeserializeError>
            where
                L: SerializableLang,
            {
                match value {
                    SerialValue::Int(i) => <$t>::try_from(*i).map_err(|_| {
                        DeserializeError::Value(SerialValueError::IntegerOutOfRange {
                            value: i.to_string(),
                            target: stringify!($t),
                        })
                    }),
                    other => Err(value_mismatch("an integer", other)),
                }
            }
        }
    )*};
}
int_value_impls!(i8, i16, i32, i64, i128, isize, u8, u16, u32, u64, u128, usize);

/// A `String` is a [`Str`](SerialValue::Str).
impl<L: Lang> SerializableValue<L> for String {
    fn serialize_value(&self, _cx: &mut SerializeContext<'_, L>) -> Result<SerialValue, SerializeError>
    where
        L: SerializableLang,
    {
        Ok(SerialValue::Str(self.clone()))
    }
}

/// A `String` is a [`Str`](SerialValue::Str); any other value is an error.
impl<L: Lang> DeserializableValue<L> for String {
    fn deserialize_value(
        value: &SerialValue,
        _cx: &mut DeserializeContext<'_, L>,
    ) -> Result<Self, DeserializeError>
    where
        L: SerializableLang,
    {
        match value {
            SerialValue::Str(s) => Ok(s.clone()),
            other => Err(value_mismatch("a string", other)),
        }
    }
}

/// `None` is [`Null`](SerialValue::Null); `Some(value)` is the value's own form. (So
/// `Some(())` and `None` have the same form and read back as `None`; the same holds
/// for a `Some(None)`.) This is what serializes the default source origin,
/// `Option<String>`.
impl<L: Lang, T: SerializableValue<L>> SerializableValue<L> for Option<T> {
    fn serialize_value(&self, cx: &mut SerializeContext<'_, L>) -> Result<SerialValue, SerializeError>
    where
        L: SerializableLang,
    {
        match self {
            Some(value) => value.serialize_value(cx),
            None => Ok(SerialValue::Null),
        }
    }
}

/// [`Null`](SerialValue::Null) is `None`; any other value is read as `T`.
impl<L: Lang, T: DeserializableValue<L>> DeserializableValue<L> for Option<T> {
    fn deserialize_value(
        value: &SerialValue,
        cx: &mut DeserializeContext<'_, L>,
    ) -> Result<Self, DeserializeError>
    where
        L: SerializableLang,
    {
        match value {
            SerialValue::Null => Ok(None),
            other => T::deserialize_value(other, cx).map(Some),
        }
    }
}

/// A `Vec<T>` is a [`List`](SerialValue::List) of the elements' forms, in order.
impl<L: Lang, T: SerializableValue<L>> SerializableValue<L> for Vec<T> {
    fn serialize_value(&self, cx: &mut SerializeContext<'_, L>) -> Result<SerialValue, SerializeError>
    where
        L: SerializableLang,
    {
        self.iter()
            .map(|item| item.serialize_value(cx))
            .collect::<Result<Vec<_>, _>>()
            .map(SerialValue::List)
    }
}

/// A [`List`](SerialValue::List) is read element by element; any other value is an
/// error.
impl<L: Lang, T: DeserializableValue<L>> DeserializableValue<L> for Vec<T> {
    fn deserialize_value(
        value: &SerialValue,
        cx: &mut DeserializeContext<'_, L>,
    ) -> Result<Self, DeserializeError>
    where
        L: SerializableLang,
    {
        match value {
            SerialValue::List(items) => items.iter().map(|item| T::deserialize_value(item, cx)).collect(),
            other => Err(value_mismatch("a list", other)),
        }
    }
}
