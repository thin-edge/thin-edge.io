use std::sync::Arc;
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
    device: {
        #[tedge_config(rename = "type", example = "thin-edge.io")]
        ty: Arc<str>,

        #[tedge_config(from = "String", example = "some value")]
        manual_from: Arc<str>,
    }
}

#[test]
fn arc_str_in_config_can_be_updated() {
    let mut config = TEdgeConfigDto::default();
    config
        .try_update_str(WritableKey::DeviceType, "new value")
        .unwrap();
    config
        .try_update_str(WritableKey::DeviceManualFrom, "different value")
        .unwrap();

    assert_eq!(config.device.ty, Some(Arc::from("new value")));
    assert_eq!(
        config.device.manual_from,
        Some(Arc::from("different value"))
    );
}
