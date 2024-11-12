use anyhow::ensure;
use serde::de::Error as _;
use serde::de::Visitor;
use serde::ser;
use serde::ser::Error as _;
use serde::ser::Impossible;
use serde::ser::SerializeMap;
use serde::Deserialize;
use serde::Serialize;
use serde::Serializer;
use std::borrow::Borrow;
use std::collections::HashMap;
use std::ffi::OsStr;
use std::fmt;
use std::iter::once;
use std::marker::PhantomData;
use std::ops::Deref;
use std::str::FromStr;
use toml::map::Map;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MultiDto<T> {
    profiles: ::std::collections::HashMap<ProfileName, T>,
    non_profile: T,
}

fn is_default<T: Default + PartialEq>(map: &HashMap<ProfileName, T>) -> bool {
    let default = T::default();
    map.values().all(|v| *v == default)
}

struct MultiDtoVisitor<T>(PhantomData<T>);

impl<'de, T> Visitor<'de> for MultiDtoVisitor<T>
where
    T: Deserialize<'de>,
{
    type Value = MultiDto<T>;

    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str("a map")
    }

    fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
    where
        A: serde::de::MapAccess<'de>,
    {
        let mut profiles = HashMap::new();
        let mut toml_map = toml::map::Map::new();

        while let Some(key) = map.next_key()? {
            if key == "profiles" {
                let value = map.next_value()?;
                profiles = value;
            } else {
                let value = map.next_value()?;
                toml_map.insert(key, value);
            }
        }

        Ok(MultiDto {
            profiles,
            non_profile: T::deserialize(toml_map).map_err(|e| A::Error::custom(e))?,
        })
    }
}

impl<'de, T> Deserialize<'de> for MultiDto<T>
where
    T: Deserialize<'de>,
{
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        deserializer.deserialize_map(MultiDtoVisitor(<_>::default()))
    }
}

impl<T> Serialize for MultiDto<T>
where
    T: Serialize + Default + PartialEq,
{
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let mut map = serializer.serialize_map(None)?;
        // TODO handle errors
        let m = self.non_profile.serialize(TomlValueSer).unwrap();
        // TODO error handling
        let toml::Value::Table(m) = m else { panic!() };
        for (k, v) in m {
            map.serialize_entry(&k, &v)?;
        }
        if !is_default(&self.profiles) {
            map.serialize_entry("profiles", &self.profiles)?;
        }
        map.end()
    }
}

struct TomlValueSer;
struct TomlMapSer(Map<String, toml::Value>);

impl Serializer for TomlValueSer {
    type Ok = toml::Value;
    type Error = toml::ser::Error;

    // Serializer struct.
    type SerializeSeq = Impossible<Self::Ok, Self::Error>;
    type SerializeTuple = Impossible<Self::Ok, Self::Error>;
    type SerializeTupleStruct = Impossible<Self::Ok, Self::Error>;
    type SerializeTupleVariant = Impossible<Self::Ok, Self::Error>;
    type SerializeMap = Impossible<Self::Ok, Self::Error>;
    type SerializeStruct = TomlMapSer;
    type SerializeStructVariant = Impossible<Self::Ok, Self::Error>;

    fn serialize_bool(self, v: bool) -> Result<Self::Ok, Self::Error> {
        Ok(toml::Value::Boolean(v))
    }

    fn serialize_i8(self, v: i8) -> Result<Self::Ok, Self::Error> {
        Ok(toml::Value::Integer(v as i64))
    }

    fn serialize_i16(self, v: i16) -> Result<Self::Ok, Self::Error> {
        Ok(toml::Value::Integer(v as i64))
    }

    fn serialize_i32(self, v: i32) -> Result<Self::Ok, Self::Error> {
        Ok(toml::Value::Integer(v as i64))
    }

    fn serialize_i64(self, v: i64) -> Result<Self::Ok, Self::Error> {
        Ok(toml::Value::Integer(v))
    }

    fn serialize_u8(self, v: u8) -> Result<Self::Ok, Self::Error> {
        Ok(toml::Value::Integer(v as i64))
    }

    fn serialize_u16(self, v: u16) -> Result<Self::Ok, Self::Error> {
        Ok(toml::Value::Integer(v as i64))
    }

    fn serialize_u32(self, v: u32) -> Result<Self::Ok, Self::Error> {
        Ok(toml::Value::Integer(v as i64))
    }

    fn serialize_u64(self, v: u64) -> Result<Self::Ok, Self::Error> {
        Ok(toml::Value::Integer(v as i64))
    }

    fn serialize_f32(self, v: f32) -> Result<Self::Ok, Self::Error> {
        Ok(toml::Value::Float(v as f64))
    }

    fn serialize_f64(self, v: f64) -> Result<Self::Ok, Self::Error> {
        Ok(toml::Value::Float(v))
    }

    fn serialize_char(self, v: char) -> Result<Self::Ok, Self::Error> {
        Ok(toml::Value::String(v.to_string()))
    }

    fn serialize_str(self, v: &str) -> Result<Self::Ok, Self::Error> {
        Ok(toml::Value::String(v.to_string()))
    }

    fn serialize_bytes(self, v: &[u8]) -> Result<Self::Ok, Self::Error> {
        todo!()
    }

    fn serialize_none(self) -> Result<Self::Ok, Self::Error> {
        Err(toml::ser::Error::custom("Cannot serialize none"))
    }

    fn serialize_some<T: ?Sized>(self, value: &T) -> Result<Self::Ok, Self::Error>
    where
        T: Serialize,
    {
        value.serialize(self)
    }

    fn serialize_unit(self) -> Result<Self::Ok, Self::Error> {
        todo!()
    }

    fn serialize_unit_struct(self, name: &'static str) -> Result<Self::Ok, Self::Error> {
        todo!()
    }

    fn serialize_unit_variant(
        self,
        name: &'static str,
        variant_index: u32,
        variant: &'static str,
    ) -> Result<Self::Ok, Self::Error> {
        todo!()
    }

    fn serialize_newtype_struct<T: ?Sized>(
        self,
        name: &'static str,
        value: &T,
    ) -> Result<Self::Ok, Self::Error>
    where
        T: Serialize,
    {
        todo!()
    }

    fn serialize_newtype_variant<T: ?Sized>(
        self,
        name: &'static str,
        variant_index: u32,
        variant: &'static str,
        value: &T,
    ) -> Result<Self::Ok, Self::Error>
    where
        T: Serialize,
    {
        todo!()
    }

    fn serialize_seq(self, len: Option<usize>) -> Result<Self::SerializeSeq, Self::Error> {
        todo!()
    }

    fn serialize_tuple(self, len: usize) -> Result<Self::SerializeTuple, Self::Error> {
        todo!()
    }

    fn serialize_tuple_struct(
        self,
        name: &'static str,
        len: usize,
    ) -> Result<Self::SerializeTupleStruct, Self::Error> {
        todo!()
    }

    fn serialize_tuple_variant(
        self,
        name: &'static str,
        variant_index: u32,
        variant: &'static str,
        len: usize,
    ) -> Result<Self::SerializeTupleVariant, Self::Error> {
        todo!()
    }

    fn serialize_map(self, len: Option<usize>) -> Result<Self::SerializeMap, Self::Error> {
        todo!()
    }

    fn serialize_struct(
        self,
        _name: &'static str,
        _len: usize,
    ) -> Result<Self::SerializeStruct, Self::Error> {
        Ok(TomlMapSer(<_>::default()))
    }

    fn serialize_struct_variant(
        self,
        name: &'static str,
        variant_index: u32,
        variant: &'static str,
        len: usize,
    ) -> Result<Self::SerializeStructVariant, Self::Error> {
        todo!()
    }
}

impl<'a> ser::SerializeStruct for TomlMapSer {
    type Ok = toml::Value;
    type Error = toml::ser::Error;

    fn serialize_field<T: ?Sized>(
        &mut self,
        key: &'static str,
        value: &T,
    ) -> Result<(), Self::Error>
    where
        T: Serialize,
    {
        // TODO we probably shouldn't ignore *all* errors
        if let Ok(value) = value.serialize(TomlValueSer) {
            self.0.insert(key.to_owned(), value);
        }
        Ok(())
    }

    fn end(self) -> Result<Self::Ok, Self::Error> {
        Ok(toml::Value::Table(self.0))
    }
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
    ensure!(
        value
            .chars()
            .all(|c| c.is_alphanumeric() || ['-', '_'].contains(&c)),
        "Profile names can only contain letters, numbers, `-` or `_`"
    );
    Ok(())
}

impl FromStr for ProfileName {
    type Err = anyhow::Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        validate_profile_name(s)?;
        Ok(Self(s.to_owned()))
    }
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
pub struct MultiReader<T> {
    profiles: ::std::collections::HashMap<ProfileName, T>,
    non_profile: T,
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
    #[error("Invalid profile name `{1}` for the multi-profile property {0}")]
    InvalidProfileName(String, String, #[source] anyhow::Error),
}

fn try_profile_name<'a>(key: &'a str, parent: &str) -> Result<&'a str, MultiError> {
    validate_profile_name(key)
        .map_err(|e| MultiError::InvalidProfileName(parent.to_owned(), key.to_owned(), e))?;
    Ok(key)
}

impl<T: Default + PartialEq> MultiDto<T> {
    pub fn try_get(&self, key: Option<&str>, parent: &str) -> Result<&T, MultiError> {
        match key {
            None => Ok(&self.non_profile),
            Some(key) => self
                .profiles
                .get(try_profile_name(key, parent)?)
                .ok_or_else(|| MultiError::MultiKeyNotFound(parent.to_owned(), key.to_owned())),
        }
    }

    pub fn try_get_mut(&mut self, key: Option<&str>, parent: &str) -> Result<&mut T, MultiError> {
        match key {
            None => Ok(&mut self.non_profile),
            Some(key) => Ok(self
                .profiles
                .entry(key.parse().map_err(|e| {
                    MultiError::InvalidProfileName(parent.to_owned(), key.to_owned(), e)
                })?)
                .or_default()),
        }
    }

    pub fn keys(&self) -> impl Iterator<Item = Option<&str>> {
        once(None).chain(self.profiles.keys().map(|k| k.0.as_str()).map(Some))
    }
}

impl<T> MultiReader<T> {
    pub fn try_get<K: Borrow<str> + ?Sized>(&self, key: Option<&K>) -> Result<&T, MultiError> {
        match key.map(|k| k.borrow()) {
            None => Ok(&self.non_profile),
            Some(key) => self
                .profiles
                .get(try_profile_name(key, self.parent)?)
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

        assert_eq!(serde_json::to_value(&dto).unwrap(), val);
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

        assert_eq!(serde_json::to_value(&dto).unwrap(), val);
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
}
