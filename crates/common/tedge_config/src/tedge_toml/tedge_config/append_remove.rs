use super::*;

#[diagnostic::on_unimplemented(
    message = "To use `{Self}` as a tedge config type, it must implement the `AppendRemoveItem` trait",
    note = "This can be done using impl_append_remove_for_single_value! macro"
)]
pub trait AppendRemoveItem {
    type Item;

    fn append(current_value: Option<Self::Item>, new_value: Self::Item) -> Option<Self::Item>;

    fn remove(current_value: Option<Self::Item>, remove_value: Self::Item) -> Option<Self::Item>;
}

macro_rules! impl_append_remove_for_single_value {
    ($($type:ty),*) => {
        $(
            impl AppendRemoveItem for $type {
                type Item = $type;

                fn append(_current_value: Option<Self::Item>, new_value: Self::Item) -> Option<Self::Item> {
                    Some(new_value)
                }

                fn remove(current_value: Option<Self::Item>, remove_value: Self::Item) -> Option<Self::Item> {
                    match current_value {
                        Some(current) if current == remove_value => None,
                        _ => current_value
                    }
                }
            }
        )*
    }
}

impl_append_remove_for_single_value!(
    Utf8PathBuf,
    AbsolutePath,
    String,
    ConnectUrl,
    HostPort<HTTPS_PORT>,
    HostPort<MQTT_TLS_PORT>,
    bool,
    IpAddr,
    u16,
    Arc<str>,
    Arc<Utf8Path>,
    AutoFlag,
    TopicPrefix,
    SoftwareManagementApiFlag,
    AutoLogUpload,
    TimeFormat,
    NonZeroU16,
    SecondsOrHumanTime,
    u32,
    AptConfig,
    MqttPayloadLimit,
    AuthMethod,
    Cryptoki
);

impl AppendRemoveItem for TemplatesSet {
    type Item = TemplatesSet;

    fn append(current_value: Option<Self::Item>, new_value: Self::Item) -> Option<Self::Item> {
        if let Some(current_value) = current_value {
            Some(TemplatesSet(
                current_value
                    .0
                    .into_iter()
                    .chain(new_value.0)
                    .collect::<std::collections::BTreeSet<String>>()
                    .into_iter()
                    .collect(),
            ))
        } else {
            Some(new_value)
        }
    }

    fn remove(current_value: Option<Self::Item>, remove_value: Self::Item) -> Option<Self::Item> {
        let mut current_value = current_value;

        if let Some(ref mut current_value) = current_value {
            let to_remove = std::collections::BTreeSet::from_iter(remove_value.0);

            current_value.0.retain(|value| !to_remove.contains(value));
        }

        current_value
    }
}
