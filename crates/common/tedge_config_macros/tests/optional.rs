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
        OptionalConfig::Empty("mqtt.external.bind.port".into())
    );
}
