use facet::Facet;
use facet_config_runtime::*;

mod schemas {
    use facet_config_runtime::*;

    facet_config_macro::define_config! {
        Test {
            mqtt: {
                /// MQTT broker port
                #[tedge_config(readonly, default(value = "1883"))]
                port: u16,

                /// External MQTT port for legacy use
                #[tedge_config(deprecated_key = "mqtt.external.port")]
                external_port: u16,
            },
            device: {
                /// Device identifier
                #[tedge_config(example = "my-device", example = "AINA123")]
                id: String,

                /// Human-readable name
                name: String,
            },
        }
    }
}

fn shape() -> &'static facet::Shape {
    <schemas::TestConfigDto as Facet>::SHAPE
}

mod readonly {
    use super::*;

    #[test]
    fn readonly_field_is_detected() {
        assert!(check_read_only(shape(), "mqtt.port").is_err());
    }

    #[test]
    fn non_readonly_field_passes() {
        assert!(check_read_only(shape(), "mqtt.external_port").is_ok());
    }

    #[test]
    fn non_readonly_in_other_group_passes() {
        assert!(check_read_only(shape(), "device.id").is_ok());
    }
}

mod aliases {
    use super::*;

    #[test]
    fn deprecated_key_is_collected() {
        let aliases = KeyAliases::from_shape(shape());
        assert_eq!(
            aliases.resolve("mqtt.external.port"),
            ("mqtt.external_port".to_owned(), Some("mqtt.external.port"))
        );
    }

    #[test]
    fn non_deprecated_key_resolves_to_itself() {
        let aliases = KeyAliases::from_shape(shape());
        assert_eq!(
            aliases.resolve("mqtt.port"),
            ("mqtt.port".to_owned(), None)
        );
    }
}

mod examples {
    use super::*;

    #[test]
    fn examples_are_read_from_facet_attrs() {
        let entries = list_key_entries(shape(), "");
        let id = entries.iter().find(|e| e.key == "device.id").unwrap();
        assert_eq!(id.examples, vec!["my-device", "AINA123"]);
    }

    #[test]
    fn field_without_examples_has_empty_list() {
        let entries = list_key_entries(shape(), "");
        let name = entries.iter().find(|e| e.key == "device.name").unwrap();
        assert!(name.examples.is_empty());
    }
}

mod external_schema {
    use facet_config_runtime::*;

    mod inner {
        use facet_config_runtime::*;

        facet_config_macro::define_config! {
            Ext {
                /// The external readonly port
                #[tedge_config(readonly, default(value = "443"))]
                port: u16,

                /// The external name
                #[tedge_config(deprecated_key = "identifier", example = "your-tenant.cumulocity.com")]
                url: String,
            }
        }
    }

    facet_config_macro::define_config! {
        Host {
            device: extern inner::ExtConfig,
        }
    }

    fn host_shape() -> &'static facet::Shape {
        <HostConfigDto as facet::Facet>::SHAPE
    }

    #[test]
    fn readonly_is_detected_via_shape_tree() {
        assert!(check_read_only(host_shape(), "device.port").is_err());
        assert!(check_read_only(host_shape(), "device.url").is_ok());
    }

    #[test]
    fn aliases_are_found_via_shape_tree() {
        let aliases = KeyAliases::from_shape(host_shape());
        assert_eq!(
            aliases.resolve("device.identifier"),
            ("device.url".to_owned(), Some("device.identifier"))
        );
    }

    #[test]
    fn examples_are_found_via_shape_tree() {
        let entries = list_key_entries(host_shape(), "");
        let url = entries.iter().find(|e| e.key == "device.url").unwrap();
        assert_eq!(url.examples, vec!["your-tenant.cumulocity.com"]);
    }
}
