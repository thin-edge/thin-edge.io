use std::str::FromStr;

use facet::Facet;

use crate::type_action::erase_str_action;
use crate::type_action::ErasedStrAction;
use crate::type_action::TypeActionRegistry;

/// Typed semantics behind `config add` and `config remove`.
///
/// Implementations define what incremental updates mean for a field type.
/// The engine calls these through a type-erased string interface so the CLI
/// can operate without knowing the concrete field type at compile time.
pub trait AppendRemoveItem: Sized {
    /// Combines an optional current value with the value supplied by the user.
    fn append(current: Option<Self>, new_value: Self) -> Option<Self>;

    /// Removes the user-supplied value from the optional current value.
    fn remove(current: Option<Self>, remove_value: Self) -> Option<Self>;
}

pub struct ErasedAppendRemove {
    pub append_str: ErasedStrAction,
    pub remove_str: ErasedStrAction,
}

pub type AppendRemoveRegistry = TypeActionRegistry<ErasedAppendRemove>;

/// Registers a type's `add`/`remove` implementation for use through the string-based config API.
pub fn register_append_remove<T>(registry: &mut AppendRemoveRegistry)
where
    T: for<'a> Facet<'a> + AppendRemoveItem + FromStr + std::fmt::Display + 'static,
    <T as FromStr>::Err: std::fmt::Display,
{
    registry.insert(
        T::SHAPE.id,
        ErasedAppendRemove {
            append_str: erase_str_action!(T, T::append),
            remove_str: erase_str_action!(T, T::remove),
        },
    );
}

macro_rules! impl_single_value {
    ($($ty:ty),* $(,)?) => {
        $(
            impl AppendRemoveItem for $ty {
                fn append(_current: Option<Self>, new_value: Self) -> Option<Self> {
                    Some(new_value)
                }

                fn remove(current: Option<Self>, remove_value: Self) -> Option<Self> {
                    match current {
                        Some(v) if v == remove_value => None,
                        other => other,
                    }
                }
            }
        )*
    };
}

impl_single_value!(
    String,
    u16,
    u64,
    std::net::IpAddr,
    std::num::NonZeroU16,
    camino::Utf8PathBuf,
    bool,
);
