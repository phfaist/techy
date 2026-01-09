
#[macro_export]
macro_rules! parsing_state_data_trait {
    (
        $(#[$struct_meta:meta])*
        pub struct $StructName:ident {
            $(
                $field:ident : $field_ty:ty = $default:expr
            ),* $(,)?
        }
    ) => {
        ::paste::paste! {
            pub trait [<$StructName Trait>] {
                $(
                    fn $field(&self) -> config_trait!(@return_type $field_ty);
                )*
            }

            $(#[$struct_meta])*
            pub struct $StructName {
                $(pub $field: $field_ty,)*
            }

            impl [<$StructName Trait>] for $StructName {
                $(
                    #[inline]
                    fn $field(&self) -> config_trait!(@return_type $field_ty) {
                        config_trait!(@return_value self.$field, $field_ty)
                    }
                )*
            }

            impl Default for $StructName {
                fn default() -> Self {
                    Self {
                        $($field: $default,)*
                    }
                }
            }
        }
    };
    
    (@return_type bool) => { bool };
    (@return_type u8) => { u8 };
    (@return_type u16) => { u16 };
    (@return_type u32) => { u32 };
    (@return_type u64) => { u64 };
    (@return_type usize) => { usize };
    (@return_type i8) => { i8 };
    (@return_type i16) => { i16 };
    (@return_type i32) => { i32 };
    (@return_type i64) => { i64 };
    (@return_type isize) => { isize };
    (@return_type f32) => { f32 };
    (@return_type f64) => { f64 };
    (@return_type $ty:ty) => { &$ty };
    
    (@return_value $expr:expr, bool) => { $expr };
    (@return_value $expr:expr, u8) => { $expr };
    (@return_value $expr:expr, u16) => { $expr };
    (@return_value $expr:expr, u32) => { $expr };
    (@return_value $expr:expr, u64) => { $expr };
    (@return_value $expr:expr, usize) => { $expr };
    (@return_value $expr:expr, i8) => { $expr };
    (@return_value $expr:expr, i16) => { $expr };
    (@return_value $expr:expr, i32) => { $expr };
    (@return_value $expr:expr, i64) => { $expr };
    (@return_value $expr:expr, isize) => { $expr };
    (@return_value $expr:expr, f32) => { $expr };
    (@return_value $expr:expr, f64) => { $expr };
    (@return_value $expr:expr, $ty:ty) => { &$expr };
}
