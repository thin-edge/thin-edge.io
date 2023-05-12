use camino::Utf8PathBuf;
use tedge_config_macros::*;

#[derive(thiserror::Error, Debug)]
pub enum ReadError {
    #[error(transparent)]
    ConfigNotSet(#[from] ConfigNotSet),
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
