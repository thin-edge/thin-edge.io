use tedge_config_macros::*;

#[derive(thiserror::Error, Debug)]
pub enum ReadError {
    #[error(transparent)]
    ConfigNotSet(#[from] ConfigNotSet),
    #[error("Something went wrong: {0}")]
    GenericError(String),
    #[error(transparent)]
    Multi(#[from] tedge_config_macros::MultiError),
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
    #[tedge_config(multi)]
    c8y: {
        #[tedge_config(reader(skip))]
        #[serde(skip)]
        read_from: camino::Utf8PathBuf,
    },
}

fn main() {}
