use facet::{Def, Facet, Shape};
use facet_reflect::Partial;

use crate::defaults::{config_get_with_defaults, DefaultSpec, DefaultsRegistry, RootResolver};
use crate::reflect::{
    dotted_key, get_struct_fields, is_config_group, is_optional_config, ConfigError,
};

/// Builds the application-facing config type from the file-facing DTO.
///
/// Defaults are applied, required fields are parsed, and optional fields keep
/// the config key used in missing-value errors.
pub fn build_reader<Dto: for<'a> Facet<'a>, Reader: for<'a> Facet<'a>>(
    dto: &Dto,
    defaults: &DefaultsRegistry,
    root_resolver: RootResolver<'_>,
) -> Result<Reader, ConfigError> {
    build_reader_at(dto, defaults, root_resolver, "")
}

/// Builds a reader with [OptionalConfig](crate::OptionalConfig) keys shown under
/// `display_prefix`, such as `mappers.c8y.`.
pub fn build_reader_at<Dto: for<'a> Facet<'a>, Reader: for<'a> Facet<'a>>(
    dto: &Dto,
    defaults: &DefaultsRegistry,
    root_resolver: RootResolver<'_>,
    display_prefix: &str,
) -> Result<Reader, ConfigError> {
    let partial =
        Partial::alloc::<Reader>().map_err(|e| ConfigError::ReflectError(format!("{e}")))?;
    let partial = populate_fields(
        partial,
        Reader::SHAPE,
        dto,
        defaults,
        root_resolver,
        "",
        display_prefix,
    )?;
    let heap_value = partial.build().map_err(reflect_err)?;
    heap_value
        .materialize::<Reader>()
        .map_err(|e| ConfigError::ReflectError(format!("{e}")))
}

fn populate_fields<'f, Dto: for<'a> Facet<'a>>(
    mut partial: Partial<'f>,
    struct_shape: &'static Shape,
    dto: &Dto,
    defaults: &DefaultsRegistry,
    root_resolver: RootResolver<'_>,
    prefix: &str,
    display_prefix: &str,
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
                partial = populate_optional_field(partial, dto, defaults, root_resolver, &key)?;
            }
            _ if is_optional_config(field_shape) => {
                partial = populate_optional_config_field(
                    partial,
                    dto,
                    defaults,
                    root_resolver,
                    &key,
                    display_prefix,
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
                    display_prefix,
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

fn populate_optional_field<'f, Dto: for<'a> Facet<'a>>(
    partial: Partial<'f>,
    dto: &Dto,
    defaults: &DefaultsRegistry,
    root_resolver: RootResolver<'_>,
    key: &str,
) -> Result<Partial<'f>, ConfigError> {
    let value = match config_get_with_defaults(dto, key, defaults, root_resolver) {
        Ok(Some(v)) => Some(v),
        Ok(None) => None,
        Err(ConfigError::ReflectError(_)) => None,
        Err(e) => return Err(e),
    };

    match value {
        Some(v) => {
            let partial = partial.begin_some().map_err(reflect_err)?;
            match partial.parse_from_str(&v) {
                Ok(partial) => Ok(partial.end().map_err(reflect_err)?),
                Err(_) => Err(ConfigError::ParseError(format!(
                    "Failed to parse value for optional field '{key}': {v}"
                ))),
            }
        }
        None => partial.set_default().map_err(reflect_err),
    }
}

fn populate_optional_config_field<'f, Dto: for<'a> Facet<'a>>(
    partial: Partial<'f>,
    dto: &Dto,
    defaults: &DefaultsRegistry,
    root_resolver: RootResolver<'_>,
    key: &str,
    display_prefix: &str,
) -> Result<Partial<'f>, ConfigError> {
    let value = match config_get_with_defaults(dto, key, defaults, root_resolver) {
        Ok(Some(v)) => Some(v),
        Ok(None) => None,
        Err(ConfigError::ReflectError(_)) => None,
        Err(e) => return Err(e),
    };

    match value {
        Some(v) => {
            let display_key = dotted_key(display_prefix, key);
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
            let partial = partial.begin_field("key").map_err(reflect_err)?;
            let partial = partial.set(display_key).map_err(reflect_err)?;
            partial.end().map_err(reflect_err)
        }
        None => {
            let display_key = dotted_key(display_prefix, presentation_key(defaults, key));
            let partial = partial.select_variant_named("Empty").map_err(reflect_err)?;
            let partial = partial.begin_field("key").map_err(reflect_err)?;
            let partial = partial.set(display_key).map_err(reflect_err)?;
            partial.end().map_err(reflect_err)
        }
    }
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
        let reader: TestReader = build_reader_at(&dto, &no_defaults(), None, "c8y").unwrap();
        assert_eq!(reader.url.key(), "c8y.url");
        assert_eq!(reader.device.id.key(), "c8y.device.id");
    }

    #[test]
    fn unset_field_falling_back_to_optional_key_reports_source_key() {
        let defaults = DefaultsRegistry::new(vec![FieldDefault {
            key: "http",
            spec: DefaultSpec::FromOptionalKey("url"),
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
