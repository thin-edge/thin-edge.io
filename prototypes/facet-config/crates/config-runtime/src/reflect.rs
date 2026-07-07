use crate::append_remove::AppendRemoveRegistry;
use facet::{Def, Facet, MapDef, Shape, Type, UserType};
use facet_reflect::{Partial, Peek};

/// Errors produced while navigating or mutating Facet-backed config DTOs.
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("Unknown config key: '{0}'")]
    UnknownKey(String),
    #[error("Unknown mapper '{name}'. To configure it, create the directory '{mappers_dir}/{name}'. Known mappers: {known}", mappers_dir = mappers_dir.display(), known = format_known(known))]
    UnknownMapper {
        name: String,
        mappers_dir: std::path::PathBuf,
        known: Vec<String>,
    },
    #[error("'{segment}' in key '{key}' is not a struct")]
    NotAStruct { key: String, segment: String },
    #[error("Failed to parse value: {0}")]
    ParseError(String),
    #[error("Config key '{0}' is read-only")]
    ReadOnly(String),
    #[error("Failed to derive a value for '{key}' from {source_key} '{source_value}': {reason}")]
    DerivedValue {
        key: String,
        source_key: String,
        source_value: String,
        reason: String,
    },
    #[error("Reflection error: {0}")]
    ReflectError(String),
    #[error("I/O error: {0}")]
    IoError(String),
}

/// A deprecated key name and the canonical key it now maps to.
pub struct DeprecatedKey {
    pub old: &'static str,
    pub new: &'static str,
}

/// Runtime lookup for deprecated config key names.
pub struct KeyAliases {
    aliases: Vec<DeprecatedKey>,
}

/// Set of keys that can be read but not changed via normal config operations.
pub struct ReadOnlyKeys(std::collections::HashSet<&'static str>);

/// Information used to show a config key in help/list output:
/// the key, generated docs, and example values.
pub struct KeyEntry {
    pub key: String,
    pub doc: &'static [&'static str],
    pub examples: &'static [&'static str],
}

impl KeyAliases {
    /// Creates an alias table from generated schema data.
    pub fn new(aliases: Vec<DeprecatedKey>) -> Self {
        Self { aliases }
    }

    /// Returns the canonical key and the deprecated key if a mapping was used.
    pub fn resolve(&self, key: &str) -> (String, Option<&'static str>) {
        for alias in &self.aliases {
            if key == alias.old {
                return (alias.new.to_owned(), Some(alias.old));
            }
        }
        (key.to_owned(), None)
    }
}

impl ReadOnlyKeys {
    pub fn new(keys: impl IntoIterator<Item = &'static str>) -> Self {
        Self(keys.into_iter().collect())
    }

    pub fn check(&self, key: &str) -> Result<(), ConfigError> {
        if self.0.contains(key) {
            Err(ConfigError::ReadOnly(key.to_owned()))
        } else {
            Ok(())
        }
    }
}

/// Reads an explicitly-set config value by dotted key, without applying defaults.
pub fn config_get<T: for<'a> Facet<'a>>(dto: &T, key: &str) -> Result<Option<String>, ConfigError> {
    validate_key(T::SHAPE, key)?;
    let peek = Peek::new(dto);
    peek_dotted_key(peek, key)
}

/// Rebuilds a DTO with one dotted key set from its CLI string representation.
pub fn config_set<T: for<'a> Facet<'a>>(
    dto: &mut T,
    key: &str,
    value: &str,
) -> Result<(), ConfigError> {
    validate_key(T::SHAPE, key)?;
    let new_dto: T = rebuild_dto(dto, key, FieldAction::Set(value))?;
    *dto = new_dto;
    Ok(())
}

/// Rebuilds a DTO with one dotted key reset to its unset `Option` state.
pub fn config_unset<T: for<'a> Facet<'a>>(dto: &mut T, key: &str) -> Result<(), ConfigError> {
    validate_key(T::SHAPE, key)?;
    let new_dto: T = rebuild_dto(dto, key, FieldAction::Unset)?;
    *dto = new_dto;
    Ok(())
}

/// Applies the registered `add` semantics for the field at `key`.
pub fn config_add<T: for<'a> Facet<'a>>(
    dto: &mut T,
    key: &str,
    value: &str,
    registry: &AppendRemoveRegistry,
) -> Result<(), ConfigError> {
    validate_key(T::SHAPE, key)?;
    let current = config_get(dto, key)?;
    let vtable = registry.get_for_key(T::SHAPE, key).ok_or_else(|| {
        ConfigError::ReflectError(format!("No AppendRemoveItem registered for field '{key}'"))
    })?;
    let result = (vtable.append_str)(current.as_deref(), value)?;
    match result {
        Some(v) => config_set(dto, key, &v),
        None => config_unset(dto, key),
    }
}

/// Applies the registered `remove` semantics for the field at `key`.
pub fn config_remove<T: for<'a> Facet<'a>>(
    dto: &mut T,
    key: &str,
    value: &str,
    registry: &AppendRemoveRegistry,
) -> Result<(), ConfigError> {
    validate_key(T::SHAPE, key)?;
    let current = config_get(dto, key)?;
    let vtable = registry.get_for_key(T::SHAPE, key).ok_or_else(|| {
        ConfigError::ReflectError(format!("No AppendRemoveItem registered for field '{key}'"))
    })?;
    let result = (vtable.remove_str)(current.as_deref(), value)?;
    match result {
        Some(v) => config_set(dto, key, &v),
        None => config_unset(dto, key),
    }
}

/// Finds the reflected type information for the final field in a dotted config key.
pub fn find_leaf_shape(shape: &'static Shape, key: &str) -> Option<&'static Shape> {
    let parts: Vec<&str> = key.split('.').collect();
    find_leaf_shape_parts(shape, &parts)
}

/// Copies set fields from one DTO shape onto another DTO shape using config keys.
pub fn overlay_dto<Base, Overlay>(base: &Base, overlay: &Overlay) -> Result<Base, ConfigError>
where
    Base: for<'a> Facet<'a> + Clone,
    Overlay: for<'a> Facet<'a>,
{
    let mut merged = base.clone();
    let keys = list_keys(Overlay::SHAPE, "");
    for key in &keys {
        if let Ok(Some(value)) = config_get(overlay, key) {
            config_set(&mut merged, key, &value)?;
        }
    }
    Ok(merged)
}

/// Lists assignable dotted config keys such as `mqtt.port`.
pub fn list_keys(shape: &'static Shape, prefix: &str) -> Vec<String> {
    list_key_entries(shape, prefix, &Default::default())
        .into_iter()
        .map(|e| e.key)
        .collect()
}

/// Lists config keys with their help text and example values.
pub fn list_key_entries(
    shape: &'static Shape,
    prefix: &str,
    examples: &std::collections::HashMap<&'static str, &'static [&'static str]>,
) -> Vec<KeyEntry> {
    let mut entries = Vec::new();
    list_keys_recursive(shape, prefix, examples, &mut entries);
    entries
}

fn format_known(known: &[String]) -> String {
    if known.is_empty() {
        "none".to_owned()
    } else {
        known.join(", ")
    }
}

pub(crate) fn is_config_group(shape: &'static Shape) -> bool {
    shape.type_tag == Some("config_group")
}

pub(crate) fn is_optional_config(shape: &'static Shape) -> bool {
    shape.type_tag == Some(crate::optional::OPTIONAL_CONFIG_TYPE_TAG)
}

pub(crate) fn get_struct_fields(shape: &'static Shape) -> Option<&'static [facet::Field]> {
    match shape.ty {
        Type::User(UserType::Struct(s)) => Some(s.fields),
        _ => None,
    }
}

pub(crate) fn field_key_name(field: &facet::Field) -> &'static str {
    field.rename.unwrap_or(field.name)
}

fn peek_field_by_key<'mem, 'facet>(
    peek_struct: &facet_reflect::PeekStruct<'mem, 'facet>,
    key_part: &str,
) -> Option<Peek<'mem, 'facet>> {
    peek_struct
        .ty()
        .fields
        .iter()
        .enumerate()
        .find(|(_, f)| field_key_name(f) == key_part)
        .and_then(|(i, _)| peek_struct.field(i).ok())
}

pub(crate) fn dotted_key(prefix: &str, name: &str) -> String {
    if prefix.is_empty() {
        name.to_owned()
    } else {
        format!("{prefix}.{name}")
    }
}

fn reflect_err(e: facet_reflect::ReflectError) -> ConfigError {
    ConfigError::ReflectError(format!("{e}"))
}

fn peek_dotted_key(peek: Peek<'_, '_>, key: &str) -> Result<Option<String>, ConfigError> {
    let parts: Vec<&str> = key.split('.').collect();
    peek_path(peek, &parts, key)
}

fn peek_path(
    peek: Peek<'_, '_>,
    parts: &[&str],
    full_key: &str,
) -> Result<Option<String>, ConfigError> {
    let Some((&part, rest)) = parts.split_first() else {
        return Ok(Some(format_peek(peek)));
    };

    let peek_struct = peek.into_struct().map_err(|_| ConfigError::NotAStruct {
        key: full_key.to_owned(),
        segment: part.to_owned(),
    })?;

    let field_peek = peek_field_by_key(&peek_struct, part)
        .ok_or_else(|| ConfigError::UnknownKey(full_key.to_owned()))?;

    let field_shape = field_peek.shape();

    if rest.is_empty() {
        if let Ok(opt) = field_peek.into_option() {
            match opt.value() {
                Some(inner) => Ok(Some(format_peek(inner))),
                None => Ok(None),
            }
        } else {
            Ok(Some(format_peek(field_peek)))
        }
    } else if let Ok(opt) = field_peek.into_option() {
        match opt.value() {
            Some(inner) => peek_path(inner, rest, full_key),
            None => Ok(None),
        }
    } else if is_config_group(field_shape) {
        peek_path(field_peek, rest, full_key)
    } else {
        Err(ConfigError::NotAStruct {
            key: full_key.to_owned(),
            segment: part.to_owned(),
        })
    }
}

fn format_peek(peek: Peek<'_, '_>) -> String {
    if let Ok(list) = peek.into_list() {
        let parts: Vec<String> = list.iter().map(|elem| format!("{elem}")).collect();
        parts.join(",")
    } else {
        format!("{peek}")
    }
}

enum FieldAction<'a> {
    Set(&'a str),
    Unset,
}

fn rebuild_dto<T: for<'a> Facet<'a>>(
    dto: &T,
    target_key: &str,
    action: FieldAction<'_>,
) -> Result<T, ConfigError> {
    let peek = Peek::new(dto);
    let partial = Partial::alloc::<T>().map_err(|e| ConfigError::ReflectError(format!("{e}")))?;
    let partial = copy_struct_with_override(partial, peek, T::SHAPE, "", target_key, &action)?;
    let heap_value = partial.build().map_err(reflect_err)?;
    heap_value
        .materialize::<T>()
        .map_err(|e| ConfigError::ReflectError(format!("{e}")))
}

fn copy_struct_with_override<'f>(
    mut partial: Partial<'f>,
    peek: Peek<'_, '_>,
    struct_shape: &'static Shape,
    prefix: &str,
    target_key: &str,
    action: &FieldAction<'_>,
) -> Result<Partial<'f>, ConfigError> {
    let fields = get_struct_fields(struct_shape)
        .ok_or_else(|| ConfigError::ReflectError("Expected struct shape".into()))?;

    let peek_struct = peek
        .into_struct()
        .map_err(|_| ConfigError::ReflectError("Expected struct value".into()))?;

    for field in fields {
        let field_key = dotted_key(prefix, field_key_name(field));
        let field_shape = field.shape();

        partial = partial.begin_field(field.name).map_err(reflect_err)?;

        let field_peek = peek_struct.field_by_name(field.name).map_err(|_| {
            ConfigError::ReflectError(format!("Field '{}' not found in struct", field.name))
        })?;

        if field_key == target_key {
            partial = apply_action_to_field(partial, field_shape, action)?;
        } else if target_key.starts_with(&format!("{field_key}.")) {
            partial = copy_group_with_override(
                partial,
                field_peek,
                field_shape,
                &field_key,
                target_key,
                action,
            )?;
        } else {
            partial = copy_field_via_strings(partial, field_peek, field_shape, &field_key)?;
        }

        partial = partial.end().map_err(reflect_err)?;
    }

    Ok(partial)
}

fn apply_action_to_field<'f>(
    partial: Partial<'f>,
    field_shape: &'static Shape,
    action: &FieldAction<'_>,
) -> Result<Partial<'f>, ConfigError> {
    match action {
        FieldAction::Set(value) => {
            if let Def::Option(_) = field_shape.def {
                let partial = partial.begin_some().map_err(reflect_err)?;
                let partial = partial
                    .parse_from_str(value)
                    .map_err(|e| ConfigError::ParseError(format!("{e}")))?;
                partial.end().map_err(reflect_err)
            } else {
                partial
                    .parse_from_str(value)
                    .map_err(|e| ConfigError::ParseError(format!("{e}")))
            }
        }
        FieldAction::Unset => {
            if let Def::Option(_) = field_shape.def {
                partial.set_default().map_err(reflect_err)
            } else {
                Err(ConfigError::ReflectError(
                    "Cannot unset a non-Option field".into(),
                ))
            }
        }
    }
}

fn copy_group_with_override<'f>(
    partial: Partial<'f>,
    field_peek: Peek<'_, '_>,
    field_shape: &'static Shape,
    field_key: &str,
    target_key: &str,
    action: &FieldAction<'_>,
) -> Result<Partial<'f>, ConfigError> {
    if let Def::Option(opt_def) = field_shape.def {
        let inner_shape = opt_def.t;
        let partial = partial.begin_some().map_err(reflect_err)?;

        let inner_peek = field_peek.into_option().ok().and_then(|opt| opt.value());

        let partial = if let Some(inner_peek) = inner_peek {
            copy_struct_with_override(
                partial,
                inner_peek,
                inner_shape,
                field_key,
                target_key,
                action,
            )?
        } else {
            build_default_with_override(partial, inner_shape, field_key, target_key, action)?
        };

        partial.end().map_err(reflect_err)
    } else if is_config_group(field_shape) {
        copy_struct_with_override(
            partial,
            field_peek,
            field_shape,
            field_key,
            target_key,
            action,
        )
    } else {
        Err(ConfigError::NotAStruct {
            key: target_key.to_owned(),
            segment: field_key.to_owned(),
        })
    }
}

fn build_default_with_override<'f>(
    mut partial: Partial<'f>,
    struct_shape: &'static Shape,
    prefix: &str,
    target_key: &str,
    action: &FieldAction<'_>,
) -> Result<Partial<'f>, ConfigError> {
    let fields = get_struct_fields(struct_shape)
        .ok_or_else(|| ConfigError::ReflectError("Expected struct shape".into()))?;

    for field in fields {
        let sub_key = dotted_key(prefix, field_key_name(field));
        let field_shape = field.shape();

        partial = partial.begin_field(field.name).map_err(reflect_err)?;

        if sub_key == target_key {
            partial = apply_action_to_field(partial, field_shape, action)?;
        } else if target_key.starts_with(&format!("{sub_key}.")) {
            if let Def::Option(opt_def) = field_shape.def {
                partial = partial.begin_some().map_err(reflect_err)?;
                partial =
                    build_default_with_override(partial, opt_def.t, &sub_key, target_key, action)?;
                partial = partial.end().map_err(reflect_err)?;
            } else if is_config_group(field_shape) {
                partial = build_default_with_override(
                    partial,
                    field_shape,
                    &sub_key,
                    target_key,
                    action,
                )?;
            } else {
                return Err(ConfigError::NotAStruct {
                    key: target_key.to_owned(),
                    segment: sub_key,
                });
            }
        } else {
            partial = partial.set_default().map_err(reflect_err)?;
        }

        partial = partial.end().map_err(reflect_err)?;
    }

    Ok(partial)
}

fn copy_field_via_strings<'f>(
    partial: Partial<'f>,
    field_peek: Peek<'_, '_>,
    field_shape: &'static Shape,
    field_key: &str,
) -> Result<Partial<'f>, ConfigError> {
    match field_shape.def {
        Def::Option(opt_def) => {
            let inner_shape = opt_def.t;
            let inner_peek = field_peek.into_option().ok().and_then(|opt| opt.value());

            match inner_peek {
                Some(inner) => {
                    let partial = partial.begin_some().map_err(reflect_err)?;
                    let partial = copy_value_via_strings(partial, inner, inner_shape, field_key)?;
                    partial.end().map_err(reflect_err)
                }
                None => partial.set_default().map_err(reflect_err),
            }
        }
        _ => copy_value_via_strings(partial, field_peek, field_shape, field_key),
    }
}

fn copy_value_via_strings<'f>(
    partial: Partial<'f>,
    peek: Peek<'_, '_>,
    shape: &'static Shape,
    field_key: &str,
) -> Result<Partial<'f>, ConfigError> {
    if is_config_group(shape) {
        return copy_all_fields_via_strings(partial, peek, shape, field_key);
    }

    if let Def::Map(map_def) = shape.def {
        return copy_map_via_strings(partial, peek, map_def, field_key);
    }

    if let Def::List(_) = shape.def {
        return copy_list_via_strings(partial, peek, field_key);
    }

    let s = format_peek(peek);
    partial
        .parse_from_str(&s)
        .map_err(|e| ConfigError::ParseError(format!("copying field '{field_key}': {e}")))
}

fn copy_map_via_strings<'f>(
    mut partial: Partial<'f>,
    peek: Peek<'_, '_>,
    map_def: MapDef,
    field_key: &str,
) -> Result<Partial<'f>, ConfigError> {
    let peek_map = peek
        .into_map()
        .map_err(|e| ConfigError::ReflectError(format!("copying map '{field_key}': {e}")))?;

    if peek_map.is_empty() {
        return partial.set_default().map_err(reflect_err);
    }

    partial = partial.init_map().map_err(reflect_err)?;

    for (key_peek, val_peek) in &peek_map {
        let key_shape = map_def.k();
        let val_shape = map_def.v();

        partial = partial.begin_key().map_err(reflect_err)?;
        partial = copy_value_via_strings(partial, key_peek, key_shape, field_key)?;
        partial = partial.end().map_err(reflect_err)?;

        partial = partial.begin_value().map_err(reflect_err)?;
        partial = copy_value_via_strings(partial, val_peek, val_shape, field_key)?;
        partial = partial.end().map_err(reflect_err)?;
    }

    Ok(partial)
}

fn copy_list_via_strings<'f>(
    mut partial: Partial<'f>,
    peek: Peek<'_, '_>,
    field_key: &str,
) -> Result<Partial<'f>, ConfigError> {
    let peek_list = peek
        .into_list()
        .map_err(|e| ConfigError::ReflectError(format!("copying list '{field_key}': {e}")))?;

    let elem_shape = peek_list.def().t();

    for elem_peek in peek_list.iter() {
        partial = partial.begin_list_item().map_err(reflect_err)?;
        partial = copy_value_via_strings(partial, elem_peek, elem_shape, field_key)?;
        partial = partial.end().map_err(reflect_err)?;
    }

    Ok(partial)
}

fn copy_all_fields_via_strings<'f>(
    mut partial: Partial<'f>,
    peek: Peek<'_, '_>,
    struct_shape: &'static Shape,
    prefix: &str,
) -> Result<Partial<'f>, ConfigError> {
    let fields = get_struct_fields(struct_shape)
        .ok_or_else(|| ConfigError::ReflectError("Expected struct shape".into()))?;

    let peek_struct = peek
        .into_struct()
        .map_err(|_| ConfigError::ReflectError("Expected struct value".into()))?;

    for field in fields {
        let field_key = dotted_key(prefix, field_key_name(field));
        let field_shape = field.shape();

        partial = partial.begin_field(field.name).map_err(reflect_err)?;

        let field_peek = peek_struct
            .field_by_name(field.name)
            .map_err(|_| ConfigError::ReflectError(format!("Field '{}' not found", field.name)))?;

        partial = copy_field_via_strings(partial, field_peek, field_shape, &field_key)?;

        partial = partial.end().map_err(reflect_err)?;
    }

    Ok(partial)
}

fn find_leaf_shape_parts(shape: &'static Shape, parts: &[&str]) -> Option<&'static Shape> {
    let (&part, rest) = parts.split_first()?;

    let fields = get_struct_fields(shape)?;
    let field = fields.iter().find(|f| field_key_name(f) == part)?;
    let field_shape = field.shape();

    let inner = if let Def::Option(opt_def) = field_shape.def {
        opt_def.t
    } else {
        field_shape
    };

    if rest.is_empty() {
        Some(inner)
    } else {
        find_leaf_shape_parts(inner, rest)
    }
}

fn validate_key(shape: &'static Shape, key: &str) -> Result<(), ConfigError> {
    find_leaf_shape(shape, key).ok_or_else(|| ConfigError::UnknownKey(key.to_owned()))?;
    Ok(())
}

fn list_keys_recursive(
    shape: &'static Shape,
    prefix: &str,
    examples: &std::collections::HashMap<&'static str, &'static [&'static str]>,
    entries: &mut Vec<KeyEntry>,
) {
    let fields = match get_struct_fields(shape) {
        Some(f) => f,
        None => return,
    };

    for field in fields {
        let field_key = dotted_key(prefix, field_key_name(field));

        let inner_shape = if let Def::Option(opt_def) = field.shape().def {
            opt_def.t
        } else {
            field.shape()
        };

        if matches!(inner_shape.def, Def::Map(_)) {
            continue;
        } else if is_config_group(inner_shape) {
            list_keys_recursive(inner_shape, &field_key, examples, entries);
        } else {
            let field_examples = examples
                .get(field_key.as_str())
                .copied()
                .unwrap_or_default();
            entries.push(KeyEntry {
                key: field_key,
                doc: field.doc,
                examples: field_examples,
            });
        }
    }
}
