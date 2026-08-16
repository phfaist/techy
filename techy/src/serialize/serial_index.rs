//! The [`serial_index!`] macro: the one sanctioned way to define a typed table
//! position type (a [`SerialIndex`](crate::serialize::SerialIndex) implementer). The
//! macro is exported at the crate root (as every `macro_rules!` macro is) and
//! documented at its canonical path, `techy::serialize::serial_index`.

/// Define a typed table position type — the `Index` type of an
/// [`ObjectSerdeDriver`](crate::serialize::ObjectSerdeDriver): a `Copy` value
/// carrying the [`TableId`](crate::serialize::TableId) of its table and the `u32`
/// position within it, implementing [`SerialIndex`](crate::serialize::SerialIndex).
///
/// ```
/// use techy::serialize::{SerialIndex, TableId};
///
/// techy::serial_index! {
///     /// A position in the table of leaves.
///     pub struct LeafIndex;
/// }
///
/// // Positions come from the session (`SerializeContext::intern`); code that holds
/// // one reads its two parts through the `SerialIndex` trait.
/// fn describe(position: LeafIndex) -> (TableId, u32) {
///     (position.table(), position.index())
/// }
/// ```
///
/// The macro defines the struct with the given attributes (documentation included)
/// and visibility, derives `Copy`, `Clone`, `Debug`, `PartialEq`, `Eq`, `Hash`,
/// `PartialOrd`, and `Ord`, implements `SerialIndex`, the crate's own conversions to
/// and from a [`SerialValue::Index`](crate::serialize::SerialValue::Index) (so the
/// type can be a field of the crate's serialized structures), and — with the `serde`
/// cargo feature — serde's `Serialize` and `Deserialize`, through which the type
/// converts to and from a `SerialValue::Index` in the bridge (`to_value` /
/// `from_value`) and renders as the two-integer pair `(table ordinal, index)` in any
/// other format.
/// A position type has no other constructor than
/// [`SerialIndex::from_parts`](crate::serialize::SerialIndex::from_parts); the
/// session mints positions when objects are interned.
#[doc(hidden)]
#[macro_export]
macro_rules! serial_index {
    ($(#[$meta:meta])* $vis:vis struct $name:ident;) => {
        $(#[$meta])*
        #[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
        $vis struct $name {
            table: $crate::serialize::TableId,
            index: u32,
        }

        impl $crate::serialize::SerialIndex for $name {
            fn from_parts(table: $crate::serialize::TableId, index: u32) -> Self {
                $name { table, index }
            }

            fn table(self) -> $crate::serialize::TableId {
                self.table
            }

            fn index(self) -> u32 {
                self.index
            }
        }

        impl $crate::__private::ToSerialValue for $name {
            fn to_serial_value(
                &self,
            ) -> ::core::result::Result<$crate::serialize::SerialValue, $crate::serialize::SerialValueError> {
                ::core::result::Result::Ok($crate::serialize::SerialValue::Index {
                    table: self.table,
                    index: self.index,
                })
            }
        }

        impl $crate::__private::FromSerialValue for $name {
            fn from_serial_value(
                value: &$crate::serialize::SerialValue,
            ) -> ::core::result::Result<Self, $crate::serialize::SerialValueError> {
                let (table, index) = $crate::__private::index_from_serial_value(value)?;
                ::core::result::Result::Ok($name { table, index })
            }
        }

        $crate::__serial_index_serde_impls!($name);
    };
}

/// The serde half of [`serial_index!`]: present exactly when techy is built with the
/// `serde` cargo feature (the macro is defined under that feature, so a downstream
/// crate's own feature set does not matter). Not public API.
#[cfg(feature = "serde")]
#[doc(hidden)]
#[macro_export]
macro_rules! __serial_index_serde_impls {
    ($name:ident) => {
        impl $crate::__private::serde::Serialize for $name {
            fn serialize<S: $crate::__private::serde::Serializer>(
                &self,
                serializer: S,
            ) -> ::core::result::Result<S::Ok, S::Error> {
                $crate::__private::serialize_index(self.table, self.index, serializer)
            }
        }

        impl<'de> $crate::__private::serde::Deserialize<'de> for $name {
            fn deserialize<D: $crate::__private::serde::Deserializer<'de>>(
                deserializer: D,
            ) -> ::core::result::Result<Self, D::Error> {
                let (table, index) = $crate::__private::deserialize_index(deserializer)?;
                ::core::result::Result::Ok($name { table, index })
            }
        }
    };
}

/// The serde half of [`serial_index!`] when techy is built without the `serde` cargo
/// feature: nothing. Not public API.
#[cfg(not(feature = "serde"))]
#[doc(hidden)]
#[macro_export]
macro_rules! __serial_index_serde_impls {
    ($name:ident) => {};
}
