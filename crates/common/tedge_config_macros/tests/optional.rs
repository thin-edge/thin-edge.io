use tedge_config_macros::*;

#[derive(thiserror::Error, Debug)]
pub enum ReadError {
    #[error(transparent)]
    ConfigNotSet(#[from] ConfigNotSet),
}

#[test]
fn vacant_optional_configurations_contain_the_relevant_key() {
    define_tedge_config! {
        mqtt: {
            external: {
                bind: {
                    port: u16,
                }
            }
        }
    }

    let dto = TEdgeConfigDto::default();
    let config = TEdgeConfigReader::from_dto(&dto, &TEdgeConfigLocation);

    assert_eq!(
        config.mqtt.external.bind.port,
        OptionalConfig::Empty("mqtt.external.bind.port")
    );
}
