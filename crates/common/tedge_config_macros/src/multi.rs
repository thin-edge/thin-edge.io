#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(untagged)]
pub enum Multi<T> {
    Multi(::std::collections::HashMap<String, T>),
    Single(T),
}

impl<T: Default> Default for Multi<T> {
    fn default() -> Self {
        Self::Single(T::default())
    }
}
impl<T: doku::Document> doku::Document for Multi<T> {
    fn ty() -> doku::Type {
        T::ty()
    }
}

impl<T: Default + PartialEq> Multi<T> {
    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }
}

#[derive(Debug, thiserror::Error)]
pub enum MultiError {
    #[error("You are trying to access a named field, but the fields are not named")]
    SingleNotMulti,
    #[error("You need a name for this field")]
    MultiNotSingle,
    #[error("Key not found in multi-value group")]
    MultiKeyNotFound,
}

impl<T> Multi<T> {
    pub fn try_get(&self, key: Option<&str>) -> Result<&T, MultiError> {
        match (self, key) {
            (Self::Single(val), None) => Ok(val),
            (Self::Multi(map), Some(key)) => map.get(key).ok_or(MultiError::MultiKeyNotFound),
            (Self::Multi(_), None) => Err(MultiError::MultiNotSingle),
            (Self::Single(_), Some(_key)) => Err(MultiError::SingleNotMulti),
        }
    }

    pub fn try_get_mut(&mut self, key: Option<&str>) -> Result<&mut T, MultiError> {
        match (self, key) {
            (Self::Single(val), None) => Ok(val),
            (Self::Multi(map), Some(key)) => map.get_mut(key).ok_or(MultiError::MultiKeyNotFound),
            (Self::Multi(_), None) => Err(MultiError::MultiNotSingle),
            (Self::Single(_), Some(_key)) => Err(MultiError::SingleNotMulti),
        }
    }

    pub fn keys(&self) -> impl Iterator<Item = Option<&str>> {
        match self {
            Self::Single(_) => itertools::Either::Left(std::iter::once(None)),
            Self::Multi(map) => itertools::Either::Right(map.keys().map(String::as_str).map(Some)),
        }
    }

    pub fn map_keys<U>(&self, f: impl Fn(Option<&str>) -> U) -> Multi<U> {
        match self {
            Self::Single(_) => Multi::Single(f(None)),
            Self::Multi(map) => Multi::Multi(
                map.keys()
                    .map(|key| (key.to_owned(), f(Some(key))))
                    .collect(),
            ),
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
        c8y: Multi<C8y>,
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
            Multi::Single(C8y {
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
            Multi::Multi(
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
        let val = Multi::Single("value");

        assert_eq!(*val.try_get(None).unwrap(), "value");
    }

    #[test]
    fn multi_can_retrieve_field_from_multi() {
        let val = Multi::Multi([("key".to_owned(), "value")].into());

        assert_eq!(*val.try_get(Some("key")).unwrap(), "value");
    }

    #[test]
    fn multi_gives_appropriate_error_retrieving_keyed_field_from_single() {
        let val = Multi::Single("value");

        assert_eq!(
            val.try_get(Some("unknown")).unwrap_err().to_string(),
            "You are trying to access a named field, but the fields are not named"
        );
    }
}
