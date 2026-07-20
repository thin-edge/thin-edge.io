//! Infrastructure for defining custom config CLI actions.
//!
//! The config system operates on string keys (`"mqtt.port"`) and string values, but
//! the underlying DTO fields are strongly typed (`u16`, list types, etc.). Each
//! action (set, add, remove, ...) needs type-aware logic -- for instance, `add` on a
//! list type merges into a set, while `add` on a `u16` just overwrites.
//!
//! Writing that dispatch by hand for every action x every type is repetitive: parse
//! the string, call the typed method, format the result back. This module provides
//! the shared machinery so that adding a new action only requires defining the trait
//! and registering it -- the string conversion boilerplate is handled once.
//!
//! See [`AppendRemoveRegistry`](crate::AppendRemoveRegistry) for the primary consumer of this
//! infrastructure, which implements `config add` and `config remove`. The tests in
//! this module demonstrate a minimal `Toggle` action as a worked example.

use std::collections::HashMap;

use facet::Shape;

use crate::reflect::find_leaf_shape;
use crate::reflect::ConfigError;

/// A type-erased config action operating on string representations.
///
/// Takes the current value (if set) and a string argument, returns the new value
/// (or `None` to unset the field). This is the common signature that all actions
/// are erased to, regardless of the underlying typed operation.
pub type ErasedStrAction = fn(Option<&str>, &str) -> Result<Option<String>, ConfigError>;

/// Erases a typed action into an [`ErasedStrAction`] fn pointer.
///
/// Handles the `&str` <-> `T` conversion via `FromStr`/`Display` so that action
/// implementations only deal with typed values.
///
/// **Binary** -- action takes the current value and a new value:
/// ```ignore
/// erase_str_action!(MyType, MyTrait::my_method)
/// // Wraps: fn(Option<MyType>, MyType) -> Option<MyType>
/// ```
///
/// **Unary** -- action only uses the current value (the string argument is ignored):
/// ```ignore
/// erase_str_action!(unary MyType, MyTrait::my_method)
/// // Wraps: fn(Option<MyType>) -> Option<MyType>
/// ```
macro_rules! erase_str_action {
    ($T:ty, $method:expr) => {
        |current: Option<&str>, value: &str| {
            let current: Option<$T> = current.map(|s| s.parse()).transpose().map_err(
                |e: <$T as ::std::str::FromStr>::Err| {
                    $crate::reflect::ConfigError::ParseError(e.to_string())
                },
            )?;
            let new_value: $T = value
                .parse()
                .map_err(|e: <$T as ::std::str::FromStr>::Err| {
                    $crate::reflect::ConfigError::ParseError(e.to_string())
                })?;
            Ok($method(current, new_value).map(|v| v.to_string()))
        }
    };
    (unary $T:ty, $method:expr) => {
        |current: Option<&str>, _value: &str| {
            let current: Option<$T> = current.map(|s| s.parse()).transpose().map_err(
                |e: <$T as ::std::str::FromStr>::Err| {
                    $crate::reflect::ConfigError::ParseError(e.to_string())
                },
            )?;
            Ok($method(current).map(|v| v.to_string()))
        }
    };
}

pub(crate) use erase_str_action;

/// Maps Facet type ids to type-erased actions, resolved by dotted config key.
///
/// The config CLI receives a key like `"mqtt.port"` and needs to dispatch to the
/// right typed operation without knowing the concrete type at the call site.
/// `TypeActionRegistry` bridges that gap: at startup each field type registers a
/// vtable `V` (a struct of [`ErasedStrAction`] fields), and at runtime
/// [`get_for_key`](Self::get_for_key) walks the Facet type data to find the field
/// type, then returns the matching vtable.
///
/// `V` is the vtable type -- one struct per action kind. For the real-world example,
/// see [`AppendRemoveRegistry`](crate::AppendRemoveRegistry).
///
/// # Adding a new action
///
/// 1. Define a trait with the typed operation.
/// 2. Define a vtable struct holding one [`ErasedStrAction`] per method.
/// 3. Create a `type MyRegistry = TypeActionRegistry<MyVtable>` alias and a
///    registration function that uses the local `erase_str_action!` macro to fill the vtable.
/// 4. Implement the trait on the types that support the action and register them.
///
/// The test module below demonstrates this end-to-end with a `Toggle` action.
#[derive(Default)]
pub struct TypeActionRegistry<V> {
    entries: HashMap<facet::ConstTypeId, V>,
}

impl<V> TypeActionRegistry<V> {
    /// Creates an empty registry.
    pub fn new() -> Self {
        Self {
            entries: HashMap::new(),
        }
    }

    /// Registers a vtable for one concrete Facet type id.
    pub fn insert(&mut self, id: facet::ConstTypeId, vtable: V) {
        self.entries.insert(id, vtable);
    }

    /// Resolves a dotted config key to the field type's registered vtable.
    pub fn get_for_key(&self, root_shape: &'static Shape, key: &str) -> Option<&V> {
        let leaf_shape = find_leaf_shape(root_shape, key)?;
        self.entries.get(&leaf_shape.id)
    }
}

#[cfg(test)]
mod tests {
    //! Worked example: a `Toggle` action that flips a bool field via a dotted key.

    use super::*;
    use facet::Facet;

    // 1. Define the trait

    trait ToggleItem: Sized {
        fn toggle(current: Option<Self>) -> Option<Self>;
    }

    // 2. Define the erased vtable

    struct ErasedToggle {
        toggle_str: ErasedStrAction,
    }

    // 3. Registry alias + registration function

    type ToggleRegistry = TypeActionRegistry<ErasedToggle>;

    fn register_toggle<T>(registry: &mut ToggleRegistry)
    where
        T: for<'a> Facet<'a> + ToggleItem + std::str::FromStr + std::fmt::Display + 'static,
        <T as std::str::FromStr>::Err: std::fmt::Display,
    {
        registry.insert(
            T::SHAPE.id,
            ErasedToggle {
                toggle_str: erase_str_action!(unary T, T::toggle),
            },
        );
    }

    // 4. Implement on a concrete type

    impl ToggleItem for bool {
        fn toggle(current: Option<Self>) -> Option<Self> {
            Some(!current.unwrap_or(false))
        }
    }

    // A minimal DTO to resolve keys against

    #[derive(Default, Facet)]
    #[facet(type_tag = "config_group")]
    struct ToggleDto {
        enabled: Option<bool>,
    }

    #[test]
    fn toggle_unset_bool_becomes_true() {
        let mut registry = ToggleRegistry::new();
        register_toggle::<bool>(&mut registry);

        let vtable = registry.get_for_key(ToggleDto::SHAPE, "enabled").unwrap();
        let result = (vtable.toggle_str)(None, "").unwrap();
        assert_eq!(result.as_deref(), Some("true"));
    }

    #[test]
    fn toggle_true_becomes_false() {
        let mut registry = ToggleRegistry::new();
        register_toggle::<bool>(&mut registry);

        let vtable = registry.get_for_key(ToggleDto::SHAPE, "enabled").unwrap();
        let result = (vtable.toggle_str)(Some("true"), "").unwrap();
        assert_eq!(result.as_deref(), Some("false"));
    }

    #[test]
    fn toggle_false_becomes_true() {
        let mut registry = ToggleRegistry::new();
        register_toggle::<bool>(&mut registry);

        let vtable = registry.get_for_key(ToggleDto::SHAPE, "enabled").unwrap();
        let result = (vtable.toggle_str)(Some("false"), "").unwrap();
        assert_eq!(result.as_deref(), Some("true"));
    }

    #[test]
    fn unknown_key_returns_none() {
        let mut registry = ToggleRegistry::new();
        register_toggle::<bool>(&mut registry);

        assert!(registry
            .get_for_key(ToggleDto::SHAPE, "nonexistent")
            .is_none());
    }
}
