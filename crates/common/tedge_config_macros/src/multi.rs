use anyhow::ensure;
use itertools::Either;
use std::borrow::Borrow;
use std::collections::HashMap;
use std::ffi::OsStr;
use std::fmt;
use std::ops::Deref;
use std::str::FromStr;

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(untagged)]
pub enum MultiDto<T> {
    Multi(::std::collections::HashMap<ProfileName, T>),
    Single(T),
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize, Hash)]
#[serde(try_from = "String")]
pub struct ProfileName(String);

impl TryFrom<String> for ProfileName {
    type Error = anyhow::Error;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        validate_profile_name(&value)?;
        Ok(Self(value))
    }
}

impl fmt::Display for ProfileName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl AsRef<OsStr> for ProfileName {
    fn as_ref(&self) -> &OsStr {
        self.0.as_ref()
    }
}

fn validate_profile_name(value: &str) -> Result<(), anyhow::Error> {
    ensure!(value.starts_with("@"), "Profile names must start with `@`");
    ensure!(
        value[1..]
            .chars()
            .all(|c| c.is_alphanumeric() || ['-', '_'].contains(&c)),
        "Profile names can only contain letters, numbers, `-` or `_` after the `@`"
    );
    Ok(())
}

impl FromStr for ProfileName {
    type Err = anyhow::Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        validate_profile_name(&s)?;
        Ok(Self(s.to_owned()))
    }
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(untagged)]
pub enum MultiReader<T> {
    Multi {
        map: ::std::collections::HashMap<ProfileName, T>,
        parent: &'static str,
    },
    Single {
        value: T,
        parent: &'static str,
    },
}

impl<T: Default> Default for MultiDto<T> {
    fn default() -> Self {
        Self::Single(T::default())
    }
}

impl<T: doku::Document> doku::Document for MultiDto<T> {
    fn ty() -> doku::Type {
        T::ty()
    }
}

impl<T: doku::Document> doku::Document for MultiReader<T> {
    fn ty() -> doku::Type {
        T::ty()
    }
}

impl<T: Default + PartialEq> MultiDto<T> {
    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }
}

impl Borrow<str> for ProfileName {
    fn borrow(&self) -> &str {
        &self.0
    }
}

impl Deref for ProfileName {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

#[derive(Debug, thiserror::Error)]
pub enum MultiError {
    #[error(
        "You are trying to access a profile `{1}` of {0}, but profiles are not enabled for {0}"
    )]
    SingleNotMulti(String, String),
    #[error("A profile is required for the multi-profile property {0}")]
    MultiNotSingle(String),
    #[error("Unknown profile `{1}` for the multi-profile property {0}")]
    MultiKeyNotFound(String, String),
}

impl<T: Default + PartialEq> MultiDto<T> {
    pub fn try_get(&self, key: Option<&str>, parent: &str) -> Result<&T, MultiError> {
        match (self, key) {
            (Self::Single(val), None) => Ok(val),
            (Self::Multi(map), Some(key)) => map
                .get(key)
                .ok_or_else(|| MultiError::MultiKeyNotFound(parent.to_owned(), key.to_owned())),
            (Self::Multi(_), None) => Err(MultiError::MultiNotSingle(parent.to_owned())),
            (Self::Single(_), Some(key)) => {
                Err(MultiError::SingleNotMulti(parent.into(), key.into()))
            }
        }
    }

    pub fn try_get_mut(&mut self, key: Option<&str>, parent: &str) -> Result<&mut T, MultiError> {
        match (self, key) {
            (Self::Single(val), None) => Ok(val),
            (Self::Multi(map), Some(key)) => {
                Ok(map.entry(ProfileName((*key).to_owned())).or_default())
            }
            (Self::Multi(map), None) if map.values().any(|v| *v != T::default()) => {
                Err(MultiError::MultiNotSingle(parent.to_owned()))
            }
            (multi @ Self::Multi(_), None) => {
                *multi = Self::Single(T::default());
                let Self::Single(value) = multi else {
                    unreachable!()
                };
                Ok(value)
            }
            (Self::Single(t), Some(key)) if *t != T::default() => {
                Err(MultiError::SingleNotMulti(parent.into(), key.into()))
            }
            (multi @ Self::Single(_), Some(key)) => {
                *multi = Self::Multi(HashMap::new());
                let Self::Multi(map) = multi else {
                    unreachable!()
                };
                Ok(map.entry(ProfileName((*key).to_owned())).or_default())
            }
        }
    }

    pub fn keys(&self) -> impl Iterator<Item = Option<&str>> {
        match self {
            Self::Single(_) => itertools::Either::Left(std::iter::once(None)),
            Self::Multi(map) => {
                itertools::Either::Right(map.keys().map(|k| k.0.as_str()).map(Some))
            }
        }
    }
}

impl<T> MultiReader<T> {
    pub fn try_get<K: Borrow<str> + ?Sized>(&self, key: Option<&K>) -> Result<&T, MultiError> {
        match (self, key.map(|k| k.borrow())) {
            (Self::Single { value, .. }, None) => Ok(value),
            (Self::Multi { map, parent }, Some(key)) => map
                .get(key)
                .ok_or_else(|| MultiError::MultiKeyNotFound((*parent).into(), key.into())),
            (Self::Multi { parent, .. }, None) => Err(MultiError::MultiNotSingle((*parent).into())),
            (Self::Single { parent, .. }, Some(key)) => {
                Err(MultiError::SingleNotMulti((*parent).into(), key.into()))
            }
        }
    }

    pub fn keys_str(&self) -> impl Iterator<Item = Option<&str>> {
        match self {
            Self::Single { .. } => itertools::Either::Left(std::iter::once(None)),
            Self::Multi { map, .. } => {
                itertools::Either::Right(map.keys().map(|k| k.0.as_str()).map(Some))
            }
        }
    }

    pub fn keys(&self) -> impl Iterator<Item = Option<&ProfileName>> {
        match self {
            Self::Single { .. } => itertools::Either::Left(std::iter::once(None)),
            Self::Multi { map, .. } => {
                // itertools::Either::Right(map.keys().map(|k| k.0.as_str()).map(Some))
                itertools::Either::Right(map.keys().map(Some))
            }
        }
    }

    pub fn entries(&self) -> impl Iterator<Item = (Option<&str>, &T)> {
        match self {
            Self::Single { value, .. } => Either::Left(std::iter::once((None, value))),
            Self::Multi { map, .. } => {
                Either::Right(map.iter().map(|(k, v)| (Some(k.0.as_str()), v)))
            }
        }
    }

    pub fn values(&self) -> impl Iterator<Item = &T> {
        match self {
            Self::Single { value, .. } => Either::Left(std::iter::once(value)),
            Self::Multi { map, .. } => Either::Right(map.iter().map(|(_, v)| v)),
        }
    }
}

impl<T> MultiDto<T> {
    pub fn map_keys<U>(
        &self,
        f: impl Fn(Option<&str>) -> U,
        parent: &'static str,
    ) -> MultiReader<U> {
        match self {
            Self::Single(_) => MultiReader::Single {
                value: f(None),
                parent,
            },
            Self::Multi(map) => MultiReader::Multi {
                map: map
                    .keys()
                    .map(|key| (key.to_owned(), f(Some(&key.0))))
                    .collect(),
                parent,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Deserialize;
    use serde_json::json;

    #[derive(Deserialize, Debug, PartialEq, Eq)]
    struct TEdgeConfigDto {
        c8y: MultiDto<C8y>,
    }

    #[derive(Deserialize, Debug, PartialEq, Eq, Default)]
    #[serde(default)]
    struct C8y {
        url: Option<String>,
        availability: Availability,
    }

    #[derive(Deserialize, PartialEq, Eq, Default, Debug)]
    #[serde(default)]
    struct Availability {
        interval: Option<u16>,
    }

    #[test]
    fn multi_can_deser_unnamed_group() {
        let val: TEdgeConfigDto = serde_json::from_value(json!({
            "c8y": { "url": "https://example.com" }
        }))
        .unwrap();

        assert_eq!(
            val.c8y,
            MultiDto::Single(C8y {
                url: Some("https://example.com".into()),
                availability: <_>::default(),
            })
        );
    }

    #[test]
    fn multi_can_deser_named_group() {
        let val: TEdgeConfigDto = serde_json::from_value(json!({
            "c8y": { "@cloud": { "url": "https://example.com" } }
        }))
        .unwrap();

        assert_eq!(
            val.c8y,
            MultiDto::Multi(
                [(
                    "@cloud".parse().unwrap(),
                    C8y {
                        url: Some("https://example.com".into()),
                        availability: <_>::default(),
                    }
                )]
                .into(),
            )
        );
    }

    #[test]
    fn multi_can_retrieve_field_from_single() {
        let val = MultiDto::Single("value");

        assert_eq!(*val.try_get(None, "c8y").unwrap(), "value");
    }

    #[test]
    fn multi_reader_can_retrieve_field_from_single() {
        let val = MultiReader::Single {
            value: "value",
            parent: "c8y",
        };

        assert_eq!(*val.try_get::<str>(None).unwrap(), "value");
    }

    #[test]
    fn multi_reader_can_retrieve_field_from_multi() {
        let val = MultiReader::Multi {
            map: [("@key".parse().unwrap(), "value")].into(),
            parent: "c8y",
        };

        assert_eq!(*val.try_get(Some("@key")).unwrap(), "value");
    }

    #[test]
    fn multi_can_retrieve_field_from_multi() {
        let val = MultiDto::Multi([("@key".parse().unwrap(), "value")].into());

        assert_eq!(*val.try_get(Some("@key"), "c8y").unwrap(), "value");
    }

    #[test]
    fn multi_dto_gives_appropriate_error_retrieving_keyed_field_from_single() {
        let val = MultiDto::Single("value");

        assert_eq!(
            val.try_get(Some("@unknown"), "c8y").unwrap_err().to_string(),
            "You are trying to access a profile `@unknown` of c8y, but profiles are not enabled for c8y"
        );
    }

    #[test]
    fn multi_reader_gives_appropriate_error_retrieving_keyed_field_from_single() {
        let val = MultiReader::Single {
            value: "value",
            parent: "c8y",
        };

        assert_eq!(
            val.try_get(Some("@unknown")).unwrap_err().to_string(),
            "You are trying to access a profile `@unknown` of c8y, but profiles are not enabled for c8y"
        );
    }

    #[test]
    fn multi_dto_gives_appropriate_error_retrieving_no_profile_from_multi() {
        let val = MultiDto::Multi([("@key".parse().unwrap(), "value")].into());

        assert_eq!(
            val.try_get(None, "c8y").unwrap_err().to_string(),
            "A profile is required for the multi-profile property c8y"
        );
    }

    #[test]
    fn multi_reader_gives_appropriate_error_retrieving_no_profile_from_multi() {
        let val = MultiReader::Multi {
            map: [("@key".parse().unwrap(), "value")].into(),
            parent: "c8y",
        };

        assert_eq!(
            val.try_get::<&str>(None).unwrap_err().to_string(),
            "A profile is required for the multi-profile property c8y"
        );
    }

    #[test]
    fn multi_dto_gives_appropriate_error_retrieving_unknown_profile_from_multi() {
        let val = MultiDto::Multi([("@key".parse().unwrap(), "value")].into());

        assert_eq!(
            val.try_get(Some("unknown"), "c8y").unwrap_err().to_string(),
            "Unknown profile `unknown` for the multi-profile property c8y"
        );
    }

    #[test]
    fn multi_reader_gives_appropriate_error_retrieving_unknown_profile_from_multi() {
        let val = MultiReader::Multi {
            map: [("@profile".parse().unwrap(), "value")].into(),
            parent: "c8y",
        };

        assert_eq!(
            val.try_get(Some("@unknown")).unwrap_err().to_string(),
            "Unknown profile `@unknown` for the multi-profile property c8y"
        );
    }

    #[test]
    fn multi_dto_inserts_into_map_retrieving_unknown_mutable_profile() {
        let mut val = MultiDto::Multi([("@profile".parse().unwrap(), "value")].into());

        assert_eq!(*val.try_get_mut(Some("@new_profile"), "c8y").unwrap(), "");
        let MultiDto::Multi(map) = val else {
            unreachable!()
        };
        assert_eq!(map.len(), 2);
    }

    #[test]
    fn multi_dto_can_convert_default_single_config_to_multi() {
        let mut val = MultiDto::Single("");

        assert_eq!(*val.try_get_mut(Some("new_key"), "c8y").unwrap(), "");
        let MultiDto::Multi(map) = val else {
            unreachable!()
        };
        assert_eq!(map.len(), 1);
    }

    #[test]
    fn multi_dto_can_convert_default_multi_config_to_single() {
        let mut val = MultiDto::Multi(
            [
                ("@profile".parse().unwrap(), ""),
                ("@profile2".parse().unwrap(), ""),
            ]
            .into(),
        );

        assert_eq!(*val.try_get_mut(None, "c8y").unwrap(), "");
        assert_eq!(val, MultiDto::Single(""));
    }

    #[test]
    fn multi_dto_refuses_to_convert_non_default_multi_config_to_single() {
        let mut val = MultiDto::Multi(
            [
                ("@profile".parse().unwrap(), "non default"),
                ("@profile2".parse().unwrap(), ""),
            ]
            .into(),
        );

        assert_eq!(
            val.try_get_mut(None, "c8y").unwrap_err().to_string(),
            "A profile is required for the multi-profile property c8y"
        );
    }

    #[test]
    fn multi_dto_deserializes_nested_struct_keys_correctly() {
        let val: TEdgeConfigDto = serde_json::from_value(json!({
            "c8y": {"availability": {
                "interval": 3600,
            }}
        }))
        .unwrap();

        assert_eq!(
            val.c8y,
            MultiDto::Single(C8y {
                url: None,
                availability: Availability {
                    interval: Some(3600)
                }
            })
        );
    }
}
