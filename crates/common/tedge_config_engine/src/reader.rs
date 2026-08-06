//! Builds the application-facing config type from the file-facing DTO.

use facet::Facet;

use crate::defaults::DefaultsRegistry;
use crate::defaults::RootResolver;
use crate::reflect::ConfigError;

/// Trait implemented by generated reader types via `define_config!`
///
/// `lookup_prefix` is the path at which this reader's schema is mounted in `dto`.
/// It is empty for the schema that owns the DTO, and set to the embedding
/// field's key when a schema is reused through `extern`, so that keys named
/// relative to the reader's own schema resolve against the whole DTO and the
/// prefixed entries of `defaults`.
pub trait BuildFromDto: Sized {
    fn build_from_dto<Dto: for<'a> Facet<'a>>(
        dto: &Dto,
        defaults: &DefaultsRegistry,
        root: RootResolver<'_>,
        lookup_prefix: &str,
        display_prefix: &str,
        profile: Option<&str>,
    ) -> Result<Self, ConfigError>;
}

/// Builds a reader from a DTO with defaults applied
pub fn build_reader<Dto: for<'a> Facet<'a>, Reader: BuildFromDto>(
    dto: &Dto,
    defaults: &DefaultsRegistry,
    root_resolver: RootResolver<'_>,
) -> Result<Reader, ConfigError> {
    build_reader_at(dto, defaults, root_resolver, "", None)
}

/// Builds a reader with display prefix and profile for user-facing messages
pub fn build_reader_at<Dto: for<'a> Facet<'a>, Reader: BuildFromDto>(
    dto: &Dto,
    defaults: &DefaultsRegistry,
    root_resolver: RootResolver<'_>,
    display_prefix: &str,
    profile: Option<&str>,
) -> Result<Reader, ConfigError> {
    Reader::build_from_dto(dto, defaults, root_resolver, "", display_prefix, profile)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::defaults::DefaultSpec;
    use crate::defaults::FieldDefault;
    use crate::reader_helpers;
    use crate::OptionalConfig;

    #[derive(Debug, Default, facet::Facet)]
    struct TestDto {
        url: Option<String>,
        http: Option<String>,
        device: Option<TestDeviceDto>,
    }

    #[derive(Debug, Default, facet::Facet)]
    #[facet(type_tag = "config_group")]
    struct TestDeviceDto {
        id: Option<String>,
    }

    #[derive(Debug)]
    struct TestReader {
        url: OptionalConfig<String>,
        http: OptionalConfig<String>,
        device: TestDeviceReader,
    }

    #[derive(Debug)]
    struct TestDeviceReader {
        id: OptionalConfig<String>,
    }

    impl BuildFromDto for TestReader {
        fn build_from_dto<Dto: for<'a> Facet<'a>>(
            dto: &Dto,
            defaults: &DefaultsRegistry,
            root: RootResolver<'_>,
            lookup_prefix: &str,
            display_prefix: &str,
            profile: Option<&str>,
        ) -> Result<Self, ConfigError> {
            let key = |key| reader_helpers::key_at(lookup_prefix, key);
            Ok(Self {
                url: reader_helpers::read_optional(
                    dto,
                    defaults,
                    root,
                    &key("url"),
                    display_prefix,
                    profile,
                )?,
                http: reader_helpers::read_optional(
                    dto,
                    defaults,
                    root,
                    &key("http"),
                    display_prefix,
                    profile,
                )?,
                // As generated for `device: extern TestDeviceReader`
                device: TestDeviceReader::build_from_dto(
                    dto,
                    defaults,
                    root,
                    &key("device"),
                    display_prefix,
                    profile,
                )?,
            })
        }
    }

    impl BuildFromDto for TestDeviceReader {
        fn build_from_dto<Dto: for<'a> Facet<'a>>(
            dto: &Dto,
            defaults: &DefaultsRegistry,
            root: RootResolver<'_>,
            lookup_prefix: &str,
            display_prefix: &str,
            profile: Option<&str>,
        ) -> Result<Self, ConfigError> {
            let key = reader_helpers::key_at(lookup_prefix, "id");
            Ok(Self {
                id: reader_helpers::read_optional(
                    dto,
                    defaults,
                    root,
                    &key,
                    display_prefix,
                    profile,
                )?,
            })
        }
    }

    #[test]
    fn set_field_is_present_and_carries_its_key() {
        let dto = TestDto {
            url: Some("example.com".into()),
            ..<_>::default()
        };
        let reader: TestReader = build_reader(&dto, &no_defaults(), None).unwrap();
        assert_eq!(reader.url.or_none(), Some(&"example.com".to_string()));
        assert_eq!(reader.url.key(), "url");
    }

    #[test]
    fn unset_field_is_empty_and_carries_its_key() {
        let dto = TestDto::default();
        let reader: TestReader = build_reader(&dto, &no_defaults(), None).unwrap();
        assert_eq!(reader.device.id.or_none(), None);
        assert_eq!(reader.device.id.key(), "device.id");
    }

    #[test]
    fn display_prefix_is_prepended_to_embedded_keys() {
        let dto = TestDto {
            url: Some("example.com".into()),
            ..<_>::default()
        };
        let reader: TestReader = build_reader_at(&dto, &no_defaults(), None, "c8y", None).unwrap();
        assert_eq!(reader.url.key(), "c8y.url");
        assert_eq!(reader.device.id.key(), "c8y.device.id");
    }

    #[test]
    fn profiled_reader_stores_profile_separately() {
        let dto = TestDto {
            url: Some("example.com".into()),
            ..<_>::default()
        };
        let reader: TestReader =
            build_reader_at(&dto, &no_defaults(), None, "c8y", Some("staging")).unwrap();
        assert_eq!(reader.url.key(), "c8y.url");
        assert_eq!(reader.url.profile(), Some("staging"));
        assert_eq!(reader.url.display_key(), "c8y.url (profile 'staging')");
        assert_eq!(reader.device.id.key(), "c8y.device.id");
        assert_eq!(reader.device.id.profile(), Some("staging"));
    }

    #[test]
    fn profiled_unset_field_error_includes_profile() {
        let dto = TestDto::default();
        let reader: TestReader =
            build_reader_at(&dto, &no_defaults(), None, "c8y", Some("staging")).unwrap();
        let err = reader.url.or_config_not_set().unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("c8y.url"), "expected key in error: {msg}");
        assert!(
            msg.contains("(profile 'staging')"),
            "expected profile in error: {msg}"
        );
        assert!(
            msg.contains("--profile staging"),
            "expected --profile hint in error: {msg}"
        );
    }

    #[test]
    fn unset_field_falling_back_to_optional_key_reports_source_key() {
        let defaults = DefaultsRegistry::new(vec![FieldDefault {
            key: "http".into(),
            spec: DefaultSpec::FromOptionalKey("url".into()),
        }])
        .unwrap();

        let unset = TestDto::default();
        let reader: TestReader = build_reader(&unset, &defaults, None).unwrap();
        assert_eq!(reader.http.or_none(), None);
        assert_eq!(reader.http.key(), "url");

        let set = TestDto {
            url: Some("example.com".into()),
            ..<_>::default()
        };
        let reader: TestReader = build_reader(&set, &defaults, None).unwrap();
        assert_eq!(reader.http.or_none(), Some(&"example.com".to_string()));
    }

    fn no_defaults() -> DefaultsRegistry {
        DefaultsRegistry::new(Vec::new()).unwrap()
    }
}
