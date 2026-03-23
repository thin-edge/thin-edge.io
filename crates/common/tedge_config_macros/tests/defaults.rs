use camino::Utf8PathBuf;
use tedge_config_macros::*;

#[derive(thiserror::Error, Debug)]
pub enum ReadError {
    #[error(transparent)]
    ConfigNotSet(#[from] ConfigNotSet),
}

pub trait AppendRemoveItem {
    type Item;

    fn append(current_value: Option<Self::Item>, new_value: Self::Item) -> Option<Self::Item>;

    fn remove(current_value: Option<Self::Item>, remove_value: Self::Item) -> Option<Self::Item>;
}

impl<T> AppendRemoveItem for T {
    type Item = T;

    fn append(_current_value: Option<Self::Item>, _new_value: Self::Item) -> Option<Self::Item> {
        unimplemented!()
    }

    fn remove(_current_value: Option<Self::Item>, _remove_value: Self::Item) -> Option<Self::Item> {
        unimplemented!()
    }
}

define_tedge_config! {
    #[tedge_config(deprecated_name = "azure")]
    az: {
        mapper: {
            timestamp: bool,
        }
    },
    device: {
        #[tedge_config(rename = "type")]
        ty: bool,
    }
}

#[test]
fn root_cert_path_default() {
    const DEFAULT_ROOT_CERT_PATH: &str = "/etc/ssl/certs";

    define_tedge_config! {
        az: {
            #[tedge_config(default(variable = "DEFAULT_ROOT_CERT_PATH"))]
            #[doku(as = "std::path::PathBuf")]
            root_cert_path: Utf8PathBuf,
        }
    }

    let dto = TEdgeConfigDto::default();
    let reader = TEdgeConfigReader::from_dto(&dto, &TEdgeConfigLocation);
    assert_eq!(reader.az.root_cert_path, "/etc/ssl/certs");
}

#[test]
fn default_from_key_uses_the_correct_default() {
    #![allow(unused_variables)]
    define_tedge_config! {
        test: {
            #[tedge_config(default(value = "DEFAULT_VALUE_FOR_ONE"))]
            one: String,
            #[tedge_config(default(from_key = "test.one"))]
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
fn default_from_key_uses_the_value_of_other_field_if_set() {
    #![allow(unused_variables)]
    define_tedge_config! {
        test: {
            #[tedge_config(default(value = "DEFAULT_VALUE_FOR_ONE"))]
            one: String,
            #[tedge_config(default(from_key = "test.one"))]
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
fn default_from_key_uses_its_own_value_if_both_are_set() {
    #![allow(unused_variables)]
    define_tedge_config! {
        test: {
            #[tedge_config(default(value = "DEFAULT_VALUE_FOR_ONE"))]
            one: String,
            #[tedge_config(default(from_key = "test.one"))]
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

#[test]
fn reader_with_lazy_field_serializes_to_toml() {
    #![allow(unused_variables)]
    define_tedge_config! {
        test: {
            #[tedge_config(default(value = "hello"))]
            name: String,

            #[tedge_config(reader(function = "compute_derived"))]
            #[doku(as = "String")]
            derived: Result<String, ReadError>,
        }
    }

    fn compute_derived(
        _reader: &TEdgeConfigReaderTest,
        _dto: &OptionalConfig<String>,
    ) -> Result<String, ReadError> {
        Ok("derived-value".to_owned())
    }

    let dto = TEdgeConfigDto::default();
    let reader = TEdgeConfigReader::from_dto(&dto, &TEdgeConfigLocation);

    // Serialization must succeed and must compute lazy reader fields
    let toml_str = toml::to_string_pretty(&reader.test).expect("reader should serialize to TOML");
    let table: toml::Table = toml::from_str(&toml_str).unwrap();

    // Regular config value appears in the output
    assert_eq!(table["name"].as_str(), Some("hello"));
    // Lazy reader fields are computed and included in serialized output
    assert_eq!(table["derived"].as_str(), Some("derived-value"));
}

#[test]
fn reader_with_failing_lazy_field_omits_it_from_toml() {
    #![allow(unused_variables)]
    define_tedge_config! {
        test: {
            #[tedge_config(default(value = "hello"))]
            name: String,

            #[tedge_config(reader(function = "always_fails"))]
            #[doku(as = "String")]
            broken: Result<String, ReadError>,
        }
    }

    fn always_fails(
        _reader: &TEdgeConfigReaderTest,
        _dto: &OptionalConfig<String>,
    ) -> Result<String, ReadError> {
        Err(ReadError::ConfigNotSet(ConfigNotSet {
            key: std::borrow::Cow::Borrowed("test.broken"),
        }))
    }

    let dto = TEdgeConfigDto::default();
    let reader = TEdgeConfigReader::from_dto(&dto, &TEdgeConfigLocation);

    let toml_str = toml::to_string_pretty(&reader.test)
        .expect("reader should serialize to TOML even when a lazy field fails");
    let table: toml::Table = toml::from_str(&toml_str).unwrap();

    assert_eq!(table["name"].as_str(), Some("hello"));
    // Failed lazy reader fields are omitted from TOML output (not an error)
    assert!(
        !table.contains_key("broken"),
        "failed lazy reader fields must be omitted from TOML"
    );
}

#[test]
fn default_from_key_uses_its_own_value_if_only_it_is_set() {
    #![allow(unused_variables)]
    define_tedge_config! {
        test: {
            #[tedge_config(default(value = "DEFAULT_VALUE_FOR_ONE"))]
            one: String,
            #[tedge_config(default(from_key = "test.one"))]
            two: String,
        }
    }
    let mut dto = TEdgeConfigDto::default();
    dto.test.two = Some("OWN_VALUE".into());
    assert_eq!(
        TEdgeConfigReader::from_dto(&dto, &TEdgeConfigLocation)
            .test
            .two,
        "OWN_VALUE"
    );
}
