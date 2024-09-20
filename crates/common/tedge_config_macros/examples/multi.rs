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
        #[tedge_config(reader(private))]
        url: String,
    },
}

fn url_for<'a>(reader: &'a TEdgeConfigReader, o: Option<&str>) -> &'a str {
    reader.c8y.try_get(o).unwrap().url.or_config_not_set().unwrap()
}

fn main() {
    let single_c8y_toml = "c8y.url = \"https://example.com\"";
    let single_c8y_dto = toml::from_str(single_c8y_toml).unwrap();
    let single_c8y_reader = TEdgeConfigReader::from_dto(&single_c8y_dto, &TEdgeConfigLocation);
    assert_eq!(url_for(&single_c8y_reader, None), "https://example.com");

    let multi_c8y_toml = "c8y.cloud.url = \"https://cloud.example.com\"\nc8y.edge.url = \"https://edge.example.com\"";
    let multi_c8y_dto = toml::from_str(multi_c8y_toml).unwrap();
    let multi_c8y_reader = TEdgeConfigReader::from_dto(&multi_c8y_dto, &TEdgeConfigLocation);
    assert_eq!(
        url_for(&multi_c8y_reader, Some("cloud")),
        "https://cloud.example.com"
    );
    assert_eq!(
        url_for(&multi_c8y_reader, Some("edge")),
        "https://edge.example.com"
    );

    assert!(matches!(
        single_c8y_reader.c8y.try_get(Some("cloud")),
        Err(MultiError::SingleNotMulti)
    ));
    assert!(matches!(
        multi_c8y_reader.c8y.try_get(Some("unknown")),
        Err(MultiError::MultiKeyNotFound)
    ));
    assert!(matches!(
        multi_c8y_reader.c8y.try_get(None),
        Err(MultiError::MultiNotSingle)
    ));
}
