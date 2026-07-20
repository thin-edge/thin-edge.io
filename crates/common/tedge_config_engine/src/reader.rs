use facet::Def;
use facet::Facet;
use facet::Shape;
use facet_reflect::Partial;

use crate::defaults::config_get_with_defaults;
use crate::defaults::DefaultSpec;
use crate::defaults::DefaultsRegistry;
use crate::defaults::RootResolver;
use crate::reflect::dotted_key;
use crate::reflect::get_struct_fields;
use crate::reflect::is_config_group;
use crate::reflect::is_optional_config;
use crate::reflect::ConfigError;

/// Builds the application-facing config type from the file-facing DTO.
///
/// Defaults are applied, required fields are parsed, and optional fields keep
/// the config key used in missing-value errors.
pub fn build_reader<Dto: for<'a> Facet<'a>, Reader: for<'a> Facet<'a>>(
    dto: &Dto,
    defaults: &DefaultsRegistry,
    root_resolver: RootResolver<'_>,
) -> Result<Reader, ConfigError> {
    build_reader_at(dto, defaults, root_resolver, "", None)
}

/// Builds a reader with [OptionalConfig](crate::OptionalConfig) keys shown under
/// `display_prefix` (e.g. `"c8y"`), with an optional `profile` attached to
/// each [OptionalConfig](crate::OptionalConfig) for user-facing messages.
pub fn build_reader_at<Dto: for<'a> Facet<'a>, Reader: for<'a> Facet<'a>>(
    dto: &Dto,
    defaults: &DefaultsRegistry,
    root_resolver: RootResolver<'_>,
    display_prefix: &str,
    profile: Option<&str>,
) -> Result<Reader, ConfigError> {
    let optional_config_metadata = OptionalConfigMetadata {
        prefix: display_prefix,
        profile,
    };
    let partial =
        Partial::alloc::<Reader>().map_err(|e| ConfigError::ReflectError(format!("{e}")))?;
    let partial = populate_fields(
        partial,
        Reader::SHAPE,
        dto,
        defaults,
        root_resolver,
        "",
        optional_config_metadata,
    )?;
    let heap_value = partial.build().map_err(reflect_err)?;
    heap_value
        .materialize::<Reader>()
        .map_err(|e| ConfigError::ReflectError(format!("{e}")))
}

#[derive(Clone, Copy)]
/// Metadata stored in each [`OptionalConfig`](crate::OptionalConfig) for user-facing diagnostics.
struct OptionalConfigMetadata<'a> {
    /// Prefix added to the schema key, such as `c8y` in `c8y.url`.
    prefix: &'a str,
    /// Configuration profile to mention in errors and command suggestions.
    profile: Option<&'a str>,
}

fn populate_fields<'f, Dto: for<'a> Facet<'a>>(
    mut partial: Partial<'f>,
    struct_shape: &'static Shape,
    dto: &Dto,
    defaults: &DefaultsRegistry,
    root_resolver: RootResolver<'_>,
    prefix: &str,
    optional_config_metadata: OptionalConfigMetadata<'_>,
) -> Result<Partial<'f>, ConfigError> {
    let fields = get_struct_fields(struct_shape)
        .ok_or_else(|| ConfigError::ReflectError("Reader type is not a struct".into()))?;

    for field in fields {
        let name = crate::reflect::field_key_name(field);
        let key = dotted_key(prefix, name);

        let field_shape = field.shape();

        partial = partial.begin_field(field.name).map_err(reflect_err)?;

        match field_shape.def {
            Def::Option(_) => {
                unreachable!(
                    "Reader type should not have Option<T> fields; the macro should be generating OptionalConfig<T> instead"
                );
            }
            _ if is_optional_config(field_shape) => {
                partial = populate_optional_config_field(
                    partial,
                    dto,
                    defaults,
                    root_resolver,
                    &key,
                    optional_config_metadata,
                )?;
            }
            _ if is_config_group(field_shape) => {
                partial = populate_fields(
                    partial,
                    field_shape,
                    dto,
                    defaults,
                    root_resolver,
                    &key,
                    optional_config_metadata,
                )?;
            }
            _ => {
                partial = populate_required_field(partial, dto, defaults, root_resolver, &key)?;
            }
        }

        partial = partial.end().map_err(reflect_err)?;
    }

    Ok(partial)
}

fn populate_optional_config_field<'f, Dto: for<'a> Facet<'a>>(
    partial: Partial<'f>,
    dto: &Dto,
    defaults: &DefaultsRegistry,
    root_resolver: RootResolver<'_>,
    key: &str,
    metadata: OptionalConfigMetadata<'_>,
) -> Result<Partial<'f>, ConfigError> {
    let value = match config_get_with_defaults(dto, key, defaults, root_resolver) {
        Ok(Some(v)) => Some(v),
        Ok(None) => None,
        Err(ConfigError::ReflectError(_)) => None,
        Err(e) => return Err(e),
    };

    let profile_value: Option<String> = metadata.profile.map(str::to_owned);

    match value {
        Some(v) => {
            let display_key = dotted_key(metadata.prefix, key);
            let partial = partial
                .select_variant_named("Present")
                .map_err(reflect_err)?;
            let partial = partial.begin_field("value").map_err(reflect_err)?;
            let partial = partial.parse_from_str(&v).map_err(|_| {
                ConfigError::ParseError(format!(
                    "Failed to parse value for optional field '{key}': {v}"
                ))
            })?;
            let partial = partial.end().map_err(reflect_err)?;
            let partial = set_string_field(partial, "key", display_key)?;
            set_string_field(partial, "profile", profile_value)
        }
        None => {
            let display_key = dotted_key(metadata.prefix, presentation_key(defaults, key));
            let partial = partial.select_variant_named("Empty").map_err(reflect_err)?;
            let partial = set_string_field(partial, "key", display_key)?;
            set_string_field(partial, "profile", profile_value)
        }
    }
}

fn set_string_field<'f, T: facet::Facet<'f>>(
    partial: Partial<'f>,
    name: &str,
    value: T,
) -> Result<Partial<'f>, ConfigError> {
    let partial = partial.begin_field(name).map_err(reflect_err)?;
    let partial = partial.set(value).map_err(reflect_err)?;
    partial.end().map_err(reflect_err)
}

/// Resolves the key a user should set to give a field a value, following any
/// `from_optional_key` chain to its ultimate source
fn presentation_key<'a>(defaults: &'a DefaultsRegistry, mut key: &'a str) -> &'a str {
    for _ in 0..10 {
        match defaults.get(key) {
            Some(DefaultSpec::FromOptionalKey(source)) => key = source,
            _ => break,
        }
    }
    key
}

fn populate_required_field<'f, Dto: for<'a> Facet<'a>>(
    partial: Partial<'f>,
    dto: &Dto,
    defaults: &DefaultsRegistry,
    root_resolver: RootResolver<'_>,
    key: &str,
) -> Result<Partial<'f>, ConfigError> {
    let value = config_get_with_defaults(dto, key, defaults, root_resolver)?.ok_or_else(|| {
        ConfigError::ReflectError(format!(
            "Required config key '{key}' is not set and has no default"
        ))
    })?;
    partial
        .parse_from_str(&value)
        .map_err(|_| ConfigError::ParseError(format!("Failed to parse value for '{key}': {value}")))
}

fn reflect_err(e: facet_reflect::ReflectError) -> ConfigError {
    ConfigError::ReflectError(format!("{e}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::defaults::FieldDefault;
    use crate::OptionalConfig;

    #[derive(Debug, Default, facet::Facet)]
    struct TestDto {
        url: Option<String>,
        http: Option<String>,
        device: TestDeviceDto,
    }

    #[derive(Debug, Default, facet::Facet)]
    #[facet(type_tag = "config_group")]
    struct TestDeviceDto {
        id: Option<String>,
    }

    #[derive(Debug, facet::Facet)]
    #[facet(type_tag = "config_group")]
    struct TestReader {
        url: OptionalConfig<String>,
        http: OptionalConfig<String>,
        device: TestDeviceReader,
    }

    #[derive(Debug, facet::Facet)]
    #[facet(type_tag = "config_group")]
    struct TestDeviceReader {
        id: OptionalConfig<String>,
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
