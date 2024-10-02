#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(untagged)]
pub enum MultiDto<T> {
    Multi(::std::collections::HashMap<String, T>),
    Single(T),
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(untagged)]
pub enum MultiReader<T> {
    Multi {
        map: ::std::collections::HashMap<String, T>,
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

#[derive(Debug, thiserror::Error)]
pub enum MultiError {
    #[error(
        "You are trying to access a profile `{1}` of {0}, but profiles are not enabled for {0}"
    )]
    SingleNotMulti(String, String),
    #[error("A profile is required for the multi-profile property {0}")]
    MultiNotSingle(String),
    #[error("Unknown profile {1} for the multi-profile property {0}")]
    MultiKeyNotFound(String, String),
}

impl<T: Default> MultiDto<T> {
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
            (Self::Multi(map), Some(key)) => Ok(map.entry((*key).to_owned()).or_default()),
            (Self::Multi(_), None) => Err(MultiError::MultiNotSingle(parent.to_owned())),
            (Self::Single(_), Some(key)) => {
                Err(MultiError::SingleNotMulti(parent.into(), key.into()))
            }
        }
    }

    pub fn keys(&self) -> impl Iterator<Item = Option<&str>> {
        match self {
            Self::Single(_) => itertools::Either::Left(std::iter::once(None)),
            Self::Multi(map) => itertools::Either::Right(map.keys().map(String::as_str).map(Some)),
        }
    }
}

impl<T> MultiReader<T> {
    pub fn try_get(&self, key: Option<&str>) -> Result<&T, MultiError> {
        match (self, key) {
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

    pub fn keys(&self) -> impl Iterator<Item = Option<&str>> {
        match self {
            Self::Single { .. } => itertools::Either::Left(std::iter::once(None)),
            Self::Multi { map, .. } => {
                itertools::Either::Right(map.keys().map(String::as_str).map(Some))
            }
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
                    .map(|key| (key.to_owned(), f(Some(key))))
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

    #[derive(Deserialize, Debug, PartialEq, Eq)]
    struct C8y {
        url: String,
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
                url: "https://example.com".into()
            })
        );
    }

    #[test]
    fn multi_can_deser_named_group() {
        let val: TEdgeConfigDto = serde_json::from_value(json!({
            "c8y": { "cloud": { "url": "https://example.com" } }
        }))
        .unwrap();

        assert_eq!(
            val.c8y,
            MultiDto::Multi(
                [(
                    "cloud".to_owned(),
                    C8y {
                        url: "https://example.com".into()
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

        assert_eq!(*val.try_get(None).unwrap(), "value");
    }

    #[test]
    fn multi_reader_can_retrieve_field_from_multi() {
        let val = MultiReader::Multi {
            map: [("key".to_owned(), "value")].into(),
            parent: "c8y",
        };

        assert_eq!(*val.try_get(Some("key")).unwrap(), "value");
    }

    #[test]
    fn multi_can_retrieve_field_from_multi() {
        let val = MultiDto::Multi([("key".to_owned(), "value")].into());

        assert_eq!(*val.try_get(Some("key"), "c8y").unwrap(), "value");
    }

    #[test]
    fn multi_dto_gives_appropriate_error_retrieving_keyed_field_from_single() {
        let val = MultiDto::Single("value");

        assert_eq!(
            val.try_get(Some("unknown"), "c8y").unwrap_err().to_string(),
            "You are trying to access a profile `unknown` of c8y, but profiles are not enabled for c8y"
        );
    }

    #[test]
    fn multi_reader_gives_appropriate_error_retrieving_keyed_field_from_single() {
        let val = MultiReader::Single {
            value: "value",
            parent: "c8y",
        };

        assert_eq!(
            val.try_get(Some("unknown")).unwrap_err().to_string(),
            "You are trying to access a profile `unknown` of c8y, but profiles are not enabled for c8y"
        );
    }

    #[test]
    fn multi_dto_gives_appropriate_error_retrieving_no_profile_from_multi() {
        let val = MultiDto::Multi([("key".to_owned(), "value")].into());

        assert_eq!(
            val.try_get(None, "c8y").unwrap_err().to_string(),
            "A profile is required for the multi-profile property c8y"
        );
    }

    #[test]
    fn multi_reader_gives_appropriate_error_retrieving_no_profile_from_multi() {
        let val = MultiReader::Multi {
            map: [("key".to_owned(), "value")].into(),
            parent: "c8y",
        };

        assert_eq!(
            val.try_get(None).unwrap_err().to_string(),
            "A profile is required for the multi-profile property c8y"
        );
    }

    #[test]
    fn multi_dto_gives_appropriate_error_retrieving_unknown_profile_from_multi() {
        let val = MultiDto::Multi([("key".to_owned(), "value")].into());

        assert_eq!(
            val.try_get(Some("unknown"), "c8y").unwrap_err().to_string(),
            "Unknown profile unknown for the multi-profile property c8y"
        );
    }

    #[test]
    fn multi_reader_gives_appropriate_error_retrieving_unknown_profile_from_multi() {
        let val = MultiReader::Multi {
            map: [("key".to_owned(), "value")].into(),
            parent: "c8y",
        };

        assert_eq!(
            val.try_get(Some("unknown")).unwrap_err().to_string(),
            "Unknown profile unknown for the multi-profile property c8y"
        );
    }

    #[test]
    fn multi_dto_inserts_into_map_retrieving_unknown_mutable_profile() {
        let mut val = MultiDto::Multi([("key".to_owned(), "value")].into());

        assert_eq!(*val.try_get_mut(Some("new_key"), "c8y").unwrap(), "");
        let MultiDto::Multi(map) = val else {
            unreachable!()
        };
        assert_eq!(map.len(), 2);
    }
}
