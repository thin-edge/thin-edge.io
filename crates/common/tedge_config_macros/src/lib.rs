#[doc(inline)]
pub use tedge_config_macros_macro::define_tedge_config;

mod default;
mod doku_aliases;
use default::{TEdgeConfigDefault, TEdgeConfigLocation};
use doku_aliases::*;

#[derive(serde::Deserialize, serde::Serialize, Clone, Copy)]
#[serde(
    from = "Option<T>",
    into = "Option<T>",
    bound = "T: Clone + serde::Serialize + serde::de::DeserializeOwned"
)]
pub enum OptionalConfig<T> {
    Present(T),
    Empty(&'static str),
}

impl<T> From<Option<T>> for OptionalConfig<T> {
    fn from(value: Option<T>) -> Self {
        value.map_or(Self::Empty(""), Self::Present)
    }
}

impl<T> From<OptionalConfig<T>> for Option<T> {
    fn from(value: OptionalConfig<T>) -> Self {
        match value {
            OptionalConfig::Present(t) => Some(t),
            OptionalConfig::Empty(_key_name) => None,
        }
    }
}

pub enum OptionalConfigGroup<T> {
    Present(T),
    Empty(&'static str),
    Partial(String),
}

#[derive(thiserror::Error, Debug)]
#[error(
    r#"A value for '{key}' is missing.\n\
    A value can be set with `tedge config set {key} <value>`"#
)]
pub struct ConfigNotSet {
    key: &'static str,
}

impl<T> OptionalConfig<T> {
    pub fn or_none(&self) -> Option<&T> {
        match self {
            Self::Present(value) => Some(value),
            Self::Empty(_) => None,
        }
    }

    pub fn or_err(&self) -> Result<&T, ConfigNotSet> {
        match self {
            Self::Present(value) => Ok(value),
            Self::Empty(key) => Err(ConfigNotSet { key }),
        }
    }
}

impl<T: doku::Document> doku::Document for OptionalConfig<T> {
    fn ty() -> doku::Type {
        Option::<T>::ty()
    }
}

impl<T: doku::Document> doku::Document for OptionalConfigGroup<T> {
    fn ty() -> doku::Type {
        Option::<T>::ty()
    }
}

use std::num::NonZeroU16;

define_tedge_config! {
    device: {
        #[serde(alias = "alias")]
        nested_thing: {
            /// A doc comment
            #[tedge_config(example = "2", example = "12345")]
            id: i32,
            #[tedge_config(default(value = 1883_u16))]
            test: u16,
            #[tedge_config(default(function = "default_port"))]
            #[serde(alias = "test222")]
            #[doku(as = "u16")]
            test2: NonZeroU16,
        },
        // #[tedge_config(reader(all_or_nothing))]
        other_thing: {
            // #[tedge_config(readonly)]
            test2: u16,
        }
    }
}

fn default_port(_: &TEdgeConfigDto) -> NonZeroU16 {
    NonZeroU16::try_from(183).unwrap()
}

#[test]
fn writable_keys_can_be_parsed_from_aliases() {
    let _: WritableKey = "device.alias.test222".parse().unwrap();
    let _: WritableKey = "device.nested_thing.test222".parse().unwrap();
    let _: WritableKey = "device.nested_thing.test2".parse().unwrap();
    let _: WritableKey = "device.alias.test2".parse().unwrap();
}

#[test]
fn readable_keys_can_be_parsed_from_aliases() {
    let _: ReadableKey = "device.alias.test222".parse().unwrap();
    let _: ReadableKey = "device.nested_thing.test222".parse().unwrap();
    let _: ReadableKey = "device.nested_thing.test2".parse().unwrap();
    let _: ReadableKey = "device.alias.test2".parse().unwrap();
}
#[test]
fn default_from_path_uses_the_correct_default() {
    #![allow(unused_variables)]
    define_tedge_config! {
        test: {
            #[tedge_config(default(value = "DEFAULT_VALUE_FOR_ONE"))]
            one: String,
            #[tedge_config(default(from_path = "test.one"))]
            two: String,
        }
    }
    let dto = TEdgeConfigDto::default();
    assert_eq!(
        TEdgeConfigReader::from_dto(&dto, &TEdgeConfigLocation)
            .test
            .two,
        "DEFAULT_VALUE_FOR_ONE"
    );
}

#[test]
fn default_from_path_uses_the_value_of_other_field_if_set() {
    #![allow(unused_variables)]
    define_tedge_config! {
        test: {
            #[tedge_config(default(value = "DEFAULT_VALUE_FOR_ONE"))]
            one: String,
            #[tedge_config(default(from_path = "test.one"))]
            two: String,
        }
    }
    let mut dto = TEdgeConfigDto::default();
    dto.test.one = Some("UPDATED_VALUE".into());
    assert_eq!(
        TEdgeConfigReader::from_dto(&dto, &TEdgeConfigLocation)
            .test
            .two,
        "UPDATED_VALUE"
    );
}

#[test]
fn default_from_path_uses_its_own_value_if_both_are_set() {
    #![allow(unused_variables)]
    define_tedge_config! {
        test: {
            #[tedge_config(default(value = "DEFAULT_VALUE_FOR_ONE"))]
            one: String,
            #[tedge_config(default(from_path = "test.one"))]
            two: String,
        }
    }
    let mut dto = TEdgeConfigDto::default();
    dto.test.one = Some("UPDATED_VALUE".into());
    dto.test.two = Some("OWN_VALUE".into());
    assert_eq!(
        TEdgeConfigReader::from_dto(&dto, &TEdgeConfigLocation)
            .test
            .two,
        "OWN_VALUE"
    );
}
