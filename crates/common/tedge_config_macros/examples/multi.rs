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
        #[tedge_config(example = "your-tenant.cumulocity.com")]
        #[tedge_config(reader(private))]
        url: String,

        #[tedge_config(default(from_optional_key = "c8y.url"))]
        http: String,

        smartrest: {
            /// Switch using 501-503 (without operation ID) or 504-506 (with operation ID) SmartREST messages for operation status update
            #[tedge_config(example = "true", default(value = true))]
            use_operation_id: bool,
        },
    },
}

fn url_for<'a>(reader: &'a TEdgeConfigReader, o: Option<&str>) -> &'a str {
    reader
        .c8y
        .try_get(o)
        .unwrap()
        .url
        .or_config_not_set()
        .unwrap()
}

fn main() {
    let c8y_toml = "c8y.url = \"https://example.com\"\nc8y.profiles.cloud.url = \"https://cloud.example.com\"\nc8y.profiles.edge.url = \"https://edge.example.com\"";
    let c8y_dto = toml::from_str(c8y_toml).unwrap();
    let c8y_reader = TEdgeConfigReader::from_dto(&c8y_dto, &TEdgeConfigLocation);
    assert_eq!(
        url_for(&c8y_reader, Some("cloud")),
        "https://cloud.example.com"
    );
    assert_eq!(
        url_for(&c8y_reader, Some("edge")),
        "https://edge.example.com"
    );

    assert_eq!(
        c8y_reader
            .c8y
            .try_get(Some("unknown"))
            .unwrap_err()
            .to_string(),
        "Unknown profile `unknown` for the multi-profile property c8y"
    );
    assert_eq!(url_for(&c8y_reader, None), "https://example.com",);

    assert_eq!(
        "c8y.url".parse::<ReadableKey>().unwrap(),
        ReadableKey::C8yUrl(None)
    );
    assert_eq!(
        "c8y.profiles.cloud.url".parse::<ReadableKey>().unwrap(),
        ReadableKey::C8yUrl(Some("cloud".to_owned()))
    );
    assert_eq!(
        "c8y.profiles.cloud.not_a_real_key"
            .parse::<ReadableKey>()
            .unwrap_err()
            .to_string(),
        "Unknown key: 'c8y.profiles.cloud.not_a_real_key'"
    );
    assert_eq!(
        "c8y.url1".parse::<ReadableKey>().unwrap_err().to_string(),
        "Unknown key: 'c8y.url1'"
    );

    let mut keys = c8y_reader
        .readable_keys()
        .map(|r| r.to_string())
        .collect::<Vec<_>>();
    // We need to sort the keys as the map iteration doesn't produce a consistent ordering
    keys.sort();
    assert_eq!(
        keys,
        [
            "c8y.http",
            "c8y.profiles.cloud.http",
            "c8y.profiles.cloud.smartrest.use_operation_id",
            "c8y.profiles.cloud.url",
            "c8y.profiles.edge.http",
            "c8y.profiles.edge.smartrest.use_operation_id",
            "c8y.profiles.edge.url",
            "c8y.smartrest.use_operation_id",
            "c8y.url",
        ]
    );
}
