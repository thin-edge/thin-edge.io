use anyhow::ensure;
use serde::Deserialize;
use serde::Serialize;
use std::borrow::Borrow;
use std::collections::HashMap;
use std::ffi::OsStr;
use std::fmt;
use std::iter::once;
use std::ops::Deref;
use std::str::FromStr;

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(bound(serialize = "T: Serialize + Default + PartialEq"), default)]
pub struct MultiDto<T> {
    #[serde(skip_serializing_if = "is_default")]
    pub profiles: ::std::collections::HashMap<ProfileName, T>,
    #[serde(flatten)]
    pub non_profile: T,
}

fn is_default<T: Default + PartialEq>(map: &HashMap<ProfileName, T>) -> bool {
    let default = T::default();
    map.values().all(|v| *v == default)
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize, Hash)]
#[serde(try_from = "String")]
pub struct ProfileName(String);

impl TryFrom<String> for ProfileName {
    type Error = anyhow::Error;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        value.parse()
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

impl AsRef<str> for ProfileName {
    fn as_ref(&self) -> &str {
        self.0.as_ref()
    }
}

fn validate_profile_name(value: &str) -> Result<(), anyhow::Error> {
    ensure!(
        value
            .chars()
            .all(|c| c.is_alphanumeric() || ['-', '_'].contains(&c)),
        "Profile names can only contain letters, numbers, `-` or `_`"
    );
    ensure!(!value.is_empty(), "Profile names cannot be empty");
    ensure!(
        value.chars().any(|c| c.is_alphanumeric()),
        "Profile names must contain at least one letter or number"
    );
    Ok(())
}

impl FromStr for ProfileName {
    type Err = anyhow::Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        validate_profile_name(s)?;
        Ok(Self(s.to_lowercase()))
    }
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
pub struct MultiReader<T> {
    pub profiles: ::std::collections::HashMap<ProfileName, T>,
    pub non_profile: T,
    parent: &'static str,
}

impl<T: Default + PartialEq> Default for MultiDto<T> {
    fn default() -> Self {
        Self {
            profiles: <_>::default(),
            non_profile: <_>::default(),
        }
    }
}

impl<T: doku::Document + Default + PartialEq> doku::Document for MultiDto<T> {
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

impl From<ProfileName> for String {
    fn from(value: ProfileName) -> Self {
        value.0
    }
}

#[derive(Debug, thiserror::Error)]
pub enum MultiError {
    #[error("Unknown profile `{1}` for the multi-profile property {0}")]
    MultiKeyNotFound(String, String),
    #[error("Invalid profile name `{1}` for the multi-profile property {0}")]
    InvalidProfileName(String, String, #[source] anyhow::Error),
    #[error(transparent)]
    Anyhow(#[from] anyhow::Error),
}

fn parse_profile_name(name: &str, parent: &str) -> Result<ProfileName, MultiError> {
    name.parse()
        .map_err(|e| MultiError::InvalidProfileName(parent.to_owned(), name.to_owned(), e))
}

impl<T: Default + PartialEq> MultiDto<T> {
    pub fn try_get(&self, key: Option<&str>, parent: &str) -> Result<&T, MultiError> {
        match key {
            None => Ok(&self.non_profile),
            Some(key) => self
                .profiles
                .get(&parse_profile_name(key, parent)?)
                .ok_or_else(|| MultiError::MultiKeyNotFound(parent.to_owned(), key.to_owned())),
        }
    }

    pub fn try_get_mut(&mut self, key: Option<&str>, parent: &str) -> Result<&mut T, MultiError> {
        match key {
            None => Ok(&mut self.non_profile),
            Some(key) => Ok(self
                .profiles
                .entry(parse_profile_name(key, parent)?)
                .or_default()),
        }
    }

    pub fn keys(&self) -> impl Iterator<Item = Option<&str>> {
        once(None).chain(self.profiles.keys().map(|k| k.0.as_str()).map(Some))
    }

    /// Remove the key from the map if it is empty
    pub fn remove_if_empty(&mut self, key: Option<&str>) {
        if let Some(k) = key {
            if let Ok(val) = self.try_get(key, "") {
                if *val == T::default() {
                    self.profiles.remove(k);
                }
            }
        }
    }
}

impl<T> MultiReader<T> {
    pub fn try_get<K: Borrow<str> + ?Sized>(&self, key: Option<&K>) -> Result<&T, MultiError> {
        match key.map(|k| k.borrow()) {
            None => Ok(&self.non_profile),
            Some(key) => self
                .profiles
                .get(&parse_profile_name(key, self.parent)?)
                .ok_or_else(|| MultiError::MultiKeyNotFound((*self.parent).into(), key.into())),
        }
    }

    pub fn keys_str(&self) -> impl Iterator<Item = Option<&str>> {
        once(None).chain(self.profiles.keys().map(|k| k.0.as_str()).map(Some))
    }

    pub fn keys(&self) -> impl Iterator<Item = Option<&ProfileName>> {
        once(None).chain(self.profiles.keys().map(Some))
    }

    pub fn entries(&self) -> impl Iterator<Item = (Option<&str>, &T)> {
        once((None, &self.non_profile))
            .chain(self.profiles.iter().map(|(k, v)| (Some(k.0.as_str()), v)))
    }

    pub fn values(&self) -> impl Iterator<Item = &T> {
        once(&self.non_profile).chain(self.profiles.values())
    }
}

impl<T: Default + PartialEq> MultiDto<T> {
    pub fn map_keys<U>(
        &self,
        f: impl Fn(Option<&str>) -> U,
        parent: &'static str,
    ) -> MultiReader<U> {
        MultiReader {
            profiles: self
                .profiles
                .keys()
                .map(|key| (key.to_owned(), f(Some(&key.0))))
                .collect(),
            non_profile: f(None),
            parent,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Deserialize;
    use serde_json::json;
    use tedge_config_macros_macro::define_tedge_config;

    #[derive(Deserialize, Serialize, Debug, PartialEq, Eq)]
    struct TEdgeConfigDto {
        c8y: MultiDto<C8y>,
    }

    #[derive(Deserialize, Serialize, Debug, PartialEq, Eq, Default)]
    #[serde(default)]
    struct C8y {
        #[serde(skip_serializing_if = "Option::is_none")]
        url: Option<String>,
        #[serde(skip_serializing_if = "is_default")]
        availability: Availability,
    }

    fn is_default(a: &Availability) -> bool {
        a.interval.is_none()
    }

    #[derive(Deserialize, Serialize, PartialEq, Eq, Default, Debug)]
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
            MultiDto {
                profiles: HashMap::new(),
                non_profile: C8y {
                    url: Some("https://example.com".into()),
                    availability: <_>::default(),
                }
            }
        );
    }

    #[test]
    fn multi_can_deser_named_group() {
        let val: TEdgeConfigDto = serde_json::from_value(json!({
            "c8y": { "profiles": { "cloud": { "url": "https://example.com" } }}
        }))
        .unwrap();

        assert_eq!(
            val.c8y,
            MultiDto {
                profiles: [(
                    "cloud".parse().unwrap(),
                    C8y {
                        url: Some("https://example.com".into()),
                        availability: <_>::default(),
                    }
                )]
                .into(),
                non_profile: <_>::default(),
            },
        );
    }

    #[test]
    fn multi_can_retrieve_field_from_single() {
        let val = MultiDto {
            profiles: HashMap::new(),
            non_profile: "value",
        };

        assert_eq!(*val.try_get(None, "c8y").unwrap(), "value");
    }

    #[test]
    fn multi_reader_can_retrieve_field_from_single() {
        let val = MultiReader {
            profiles: HashMap::new(),
            non_profile: "value",
            parent: "c8y",
        };

        assert_eq!(*val.try_get::<str>(None).unwrap(), "value");
    }

    #[test]
    fn multi_reader_can_retrieve_field_from_multi() {
        let val = MultiReader {
            profiles: [("key".parse().unwrap(), "value")].into(),
            non_profile: "non_profile",
            parent: "c8y",
        };

        assert_eq!(*val.try_get(Some("key")).unwrap(), "value");
    }

    #[test]
    fn multi_can_retrieve_field_from_multi() {
        let val = MultiDto {
            profiles: [("key".parse().unwrap(), "value")].into(),
            non_profile: "non_profile",
        };

        assert_eq!(*val.try_get(Some("key"), "c8y").unwrap(), "value");
    }

    #[test]
    fn multi_dto_allows_retrieving_non_profiled_value() {
        let val = MultiDto {
            profiles: [("key".parse().unwrap(), "value")].into(),
            non_profile: "non_profile",
        };

        assert_eq!(*val.try_get(None, "c8y").unwrap(), "non_profile");
    }

    #[test]
    fn multi_reader_allows_retrieving_non_profiled_value() {
        let val = MultiReader {
            profiles: [("key".parse().unwrap(), "value")].into(),
            non_profile: "non_profile",
            parent: "c8y",
        };

        assert_eq!(*val.try_get::<&str>(None).unwrap(), "non_profile");
    }

    #[test]
    fn multi_dto_gives_appropriate_error_retrieving_unknown_profile_from_multi() {
        let val = MultiDto {
            profiles: [("profile".parse().unwrap(), "value")].into(),
            non_profile: <_>::default(),
        };

        assert_eq!(
            val.try_get(Some("unknown"), "c8y").unwrap_err().to_string(),
            "Unknown profile `unknown` for the multi-profile property c8y"
        );
    }

    #[test]
    fn multi_reader_gives_appropriate_error_retrieving_unknown_profile_from_multi() {
        let val = MultiReader {
            profiles: [("profile".parse().unwrap(), "value")].into(),
            non_profile: <_>::default(),
            parent: "c8y",
        };

        assert_eq!(
            val.try_get(Some("unknown")).unwrap_err().to_string(),
            "Unknown profile `unknown` for the multi-profile property c8y"
        );
    }

    #[test]
    fn multi_dto_inserts_into_map_retrieving_unknown_mutable_profile() {
        let mut val = MultiDto {
            profiles: [("profile".parse().unwrap(), "value")].into(),
            non_profile: "non_profile",
        };

        assert_eq!(*val.try_get_mut(Some("new_profile"), "c8y").unwrap(), "");
        assert_eq!(val.profiles.len(), 2);
    }

    #[test]
    fn multi_dto_can_convert_default_single_config_to_multi() {
        let mut val = MultiDto {
            profiles: HashMap::new(),
            non_profile: "non_profile",
        };

        assert_eq!(*val.try_get_mut(Some("new_key"), "c8y").unwrap(), "");
        assert_eq!(val.profiles.len(), 1);
    }

    #[test]
    fn multi_dto_serialize() {
        let val = json!({
            "c8y": {
                "availability": {
                    "interval": 3600,
                }
            }
        });
        let dto: TEdgeConfigDto = serde_json::from_value(val.clone()).unwrap();

        assert_eq!(serde_json::to_value(dto).unwrap(), val);
    }

    #[test]
    fn profiled_multi_dto_serialize() {
        let val = json!({
            "c8y": {
                "profiles": {"test": {
                "availability": {
                    "interval": 3600
                }}}
            }
        });
        let dto: TEdgeConfigDto = serde_json::from_value(val.clone()).unwrap();

        assert_eq!(serde_json::to_value(dto).unwrap(), val);
    }

    #[test]
    fn multi_dto_deserializes_nested_struct_keys_correctly() {
        let val: TEdgeConfigDto = serde_json::from_value(json!({
            "c8y": {
                "availability": {
                    "interval": 3600,
                }
            }
        }))
        .unwrap();

        assert_eq!(
            val.c8y,
            MultiDto {
                non_profile: C8y {
                    url: None,
                    availability: Availability {
                        interval: Some(3600)
                    }
                },
                profiles: HashMap::new(),
            }
        );
    }

    mod cleanup_on_unset {
        use super::*;
        use crate::*;

        define_tedge_config! {
            #[tedge_config(multi)]
            c8y: {
                url: String,
                availability: {
                    interval: String,
                },
            }
        }

        #[test]
        fn multi_dto_is_cleaned_up_if_default_value() {
            let mut config: TEdgeConfigDto =
                toml::from_str("c8y.profiles.test.url = \"example.com\"").unwrap();
            config
                .try_unset_key(&WritableKey::C8yUrl(Some("test".into())))
                .unwrap();
            assert_eq!(config.c8y.profiles.len(), 0);
        }

        #[test]
        fn multi_dto_is_not_cleaned_up_if_not_default_value() {
            let mut config: TEdgeConfigDto = toml::from_str(
                "[c8y.profiles.test]\nurl = \"example.com\"\navailability.interval = \"60m\"",
            )
            .unwrap();
            config
                .try_unset_key(&WritableKey::C8yUrl(Some("test".into())))
                .unwrap();
            assert_eq!(config.c8y.profiles.len(), 1);
        }

        #[derive(Debug, thiserror::Error)]
        #[allow(unused)]
        enum ReadError {
            #[error(transparent)]
            ConfigNotSet(#[from] ConfigNotSet),
            #[error(transparent)]
            Multi(#[from] MultiError),
        }
        #[allow(unused)]
        trait AppendRemoveItem {
            type Item;
            fn append(
                current_value: Option<Self::Item>,
                new_value: Self::Item,
            ) -> Option<Self::Item>;
            fn remove(
                current_value: Option<Self::Item>,
                remove_value: Self::Item,
            ) -> Option<Self::Item>;
        }
        impl AppendRemoveItem for String {
            type Item = Self;
            fn append(_: Option<Self::Item>, _: Self::Item) -> Option<Self::Item> {
                unimplemented!()
            }
            fn remove(_: Option<Self::Item>, _: Self::Item) -> Option<Self::Item> {
                unimplemented!()
            }
        }
    }
}
