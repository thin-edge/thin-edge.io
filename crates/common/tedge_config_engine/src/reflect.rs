use crate::append_remove::AppendRemoveRegistry;
use crate::attrs;
use crate::defaults::ValueOrigin;
use facet::Def;
use facet::Facet;
use facet::Shape;
use facet::Type;
use facet::UserType;
use facet_reflect::HeapValue;
use facet_reflect::Partial;
use facet_reflect::Peek;
use facet_reflect::Poke;

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
    #[error("{}", format_invalid_value(key, source_key, origin, value, reason))]
    InvalidValue {
        /// The key that was read
        key: String,
        /// The key that supplied the value, which may differ from `key`
        source_key: String,
        origin: ValueOrigin,
        value: String,
        reason: String,
    },
    #[error("{}", format_cycle_error(cycle))]
    DefaultCycle {
        /// The keys forming the cycle, starting and ending at the same key
        cycle: Vec<String>,
    },
    #[error("Config key '{0}' is read-only")]
    ReadOnly(String),
    #[error("Failed to derive a value for '{key}' from {source_key} '{source_value}': {reason}")]
    DerivedValue {
        key: String,
        source_key: String,
        source_value: String,
        reason: String,
    },
    #[error("'{key}' can fall back to the root config key '{root_key}', but no root config was supplied")]
    NoRootConfig { key: String, root_key: String },
    #[error("'{key}' falls back to '{root_key}', which is not a key in the root config")]
    UnknownRootKey { key: String, root_key: String },
    #[error("'{key}' uses a from_root default ('{root_key}'), which is not allowed in the root config itself")]
    FromRootInRootConfig { key: String, root_key: String },
    #[error("A config source is already mounted at prefix '{0}'")]
    DuplicatePrefix(String),
    #[error("I/O error: {0}")]
    IoError(String),
}

/// A deprecated key name and the canonical key it now maps to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeprecatedKey {
    pub old: String,
    pub new: String,
}

/// Runtime lookup for deprecated config key names.
///
/// Built by walking the DTO shape tree and collecting `tedge::deprecated_key`
/// facet attributes.
pub struct KeyAliases {
    aliases: Vec<DeprecatedKey>,
}

/// Information used to show a config key in help/list output:
/// the key, generated docs, and example values.
pub struct KeyEntry {
    pub key: String,
    pub doc: &'static [&'static str],
    pub examples: Vec<&'static str>,
}

impl KeyAliases {
    /// Builds the alias table by walking the DTO shape tree.
    pub fn from_shape(shape: &'static Shape) -> Self {
        let mut aliases = Vec::new();
        let ctx = AliasWalkContext {
            walk: "",
            schema_root: "",
        };
        collect_aliases(shape, &ctx, &mut aliases);
        Self { aliases }
    }

    /// Returns the canonical key and the deprecated key if a mapping was used.
    pub fn resolve(&self, key: &str) -> (String, Option<&str>) {
        for alias in &self.aliases {
            if key == alias.old {
                return (alias.new.clone(), Some(&alias.old));
            }
        }
        (key.to_owned(), None)
    }
}

/// Returns an error if the field at `key` is marked read-only via `tedge::readonly`.
pub fn check_read_only(shape: &'static Shape, key: &str) -> Result<(), ConfigError> {
    let field = find_leaf_field(shape, key);
    if let Some(field) = field {
        if field.has_attr(Some(attrs::NS), "readonly") {
            return Err(ConfigError::ReadOnly(key.to_owned()));
        }
    }
    Ok(())
}

/// Reads an explicitly-set config value by dotted key, without applying defaults.
pub fn config_get<T: for<'a> Facet<'a>>(dto: &T, key: &str) -> Result<Option<String>, ConfigError> {
    validate_key(T::SHAPE, key)?;
    let peek = Peek::new(dto);
    peek_dotted_key(peek, key)
}

/// Sets one dotted key in a DTO from its CLI string representation.
pub fn config_set<T: for<'a> Facet<'a>>(
    dto: &mut T,
    key: &str,
    value: &str,
) -> Result<(), ConfigError> {
    let leaf_shape = leaf_shape_of(T::SHAPE, key)?;
    // The value is parsed before the DTO is touched, so an invalid value leaves
    // the DTO exactly as it was
    let parsed = parse_to_value(leaf_shape, value)?;
    poke_dotted_key(dto, key, FieldAction::Set(parsed))
}

/// Resets one dotted key in a DTO to its unset `Option` state.
pub fn config_unset<T: for<'a> Facet<'a>>(dto: &mut T, key: &str) -> Result<(), ConfigError> {
    leaf_shape_of(T::SHAPE, key)?;
    poke_dotted_key(dto, key, FieldAction::Unset)
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
    let vtable = registry
        .get_for_key(T::SHAPE, key)
        .unwrap_or_else(|| panic!("No AppendRemoveItem registered for field '{key}'"));
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
    let vtable = registry
        .get_for_key(T::SHAPE, key)
        .unwrap_or_else(|| panic!("No AppendRemoveItem registered for field '{key}'"));
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
    list_key_entries(shape, prefix)
        .into_iter()
        .map(|e| e.key)
        .collect()
}

/// Lists config keys with their help text and example values.
///
/// Examples are read from `tedge::example` facet attributes on each field.
pub fn list_key_entries(shape: &'static Shape, prefix: &str) -> Vec<KeyEntry> {
    let mut entries = Vec::new();
    list_keys_recursive(shape, prefix, &mut entries);
    entries
}

/// Renders [ConfigError::InvalidValue], naming where the offending value came from
///
/// A value the schema supplied is not something the reader can correct in
/// their config file, so those messages say so rather than pointing at a key
/// the reader never set.
fn format_invalid_value(
    key: &str,
    source_key: &str,
    origin: &ValueOrigin,
    value: &str,
    reason: &str,
) -> String {
    const BUG: &str = " This is a bug in thin-edge.io.";

    match origin {
        ValueOrigin::Derived { source_value } => format!(
            "'{key}' is derived from '{source_key}' ('{source_value}'); the derived value '{value}' is not valid: {reason}"
        ),
        ValueOrigin::Root => format!(
            "'{key}' falls back to the root config key '{source_key}', whose value '{value}' is not valid: {reason}"
        ),
        ValueOrigin::Explicit if source_key == key => {
            format!("Invalid value for '{key}': '{value}' — {reason}")
        }
        ValueOrigin::Explicit => format!(
            "'{key}' is not set, so it falls back to '{source_key}', whose value '{value}' is not valid: {reason}"
        ),
        ValueOrigin::SchemaDefault if source_key == key => format!(
            "The built-in default for '{key}' is not a valid value: '{value}' — {reason}.{BUG}"
        ),
        ValueOrigin::SchemaDefault => format!(
            "'{key}' is not set, so it falls back to '{source_key}', whose built-in default '{value}' is not valid: {reason}.{BUG}"
        ),
    }
}

/// The most keys shown in full before a cycle is abbreviated
const MAX_CYCLE_KEYS: usize = 10;

fn format_cycle_error(cycle: &[String]) -> String {
    match cycle.first() {
        Some(key) => format!("The default for '{key}' is cyclic: {}", format_cycle(cycle)),
        None => "A default is cyclic".to_owned(),
    }
}

/// Renders a cycle as the path around it, eliding the middle of a long one
///
/// A cycle spanning a whole schema is unreadable in full and the ends are what
/// identify it, so only the keys either side of the loop are kept.
pub(crate) fn format_cycle(cycle: &[String]) -> String {
    const SEPARATOR: &str = " -> ";

    if cycle.len() <= MAX_CYCLE_KEYS {
        return cycle.join(SEPARATOR);
    }

    let head = cycle[..4].join(SEPARATOR);
    let tail = cycle[cycle.len() - 3..].join(SEPARATOR);
    let elided = cycle.len() - 7;
    format!("{head}{SEPARATOR}... ({elided} more){SEPARATOR}{tail}")
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

fn expect_reflect<T>(result: Result<T, facet_reflect::ReflectError>) -> T {
    result.unwrap_or_else(|e| {
        panic!("facet reflection failed on a shape the config engine already validated: {e}")
    })
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

/// The mutation to apply at the leaf of a dotted config key.
enum FieldAction {
    /// The already-parsed value to store, whose shape is the leaf's inner type
    Set(HeapValue<'static, false>),
    Unset,
}

/// Parses `value` into a standalone value of `shape`.
fn parse_to_value(
    shape: &'static Shape,
    value: &str,
) -> Result<HeapValue<'static, false>, ConfigError> {
    Ok(expect_reflect(
        alloc_shape(shape)
            .parse_from_str(value)
            // The value is parsed on its own, so the reflection path is always the
            // root of that value and adds nothing to the message
            .map_err(|e| ConfigError::ParseError(format!("{}", e.kind)))?
            .build(),
    ))
}

/// Builds an all-unset group so a nested key has somewhere to live.
fn default_group(shape: &'static Shape) -> HeapValue<'static, false> {
    expect_reflect(expect_reflect(alloc_shape(shape).set_default()).build())
}

fn alloc_shape(shape: &'static Shape) -> Partial<'static, false> {
    // SAFETY: `shape` is reached by walking the shape tree of a `Facet` type,
    // so it describes a real type
    unsafe { Partial::alloc_shape_owned(shape) }
        .unwrap_or_else(|e| panic!("allocating a value of type '{shape}': {e}"))
}

/// Applies `action` at the dotted `key`, editing `dto` in place.
fn poke_dotted_key<T: for<'a> Facet<'a>>(
    dto: &mut T,
    key: &str,
    action: FieldAction,
) -> Result<(), ConfigError> {
    let parts: Vec<&str> = key.split('.').collect();
    poke_path(Poke::new(dto), &parts, key, action)
}

fn poke_path(
    poke: Poke<'_, 'static>,
    parts: &[&str],
    full_key: &str,
    action: FieldAction,
) -> Result<(), ConfigError> {
    let Some((&part, rest)) = parts.split_first() else {
        return Err(ConfigError::UnknownKey(full_key.to_owned()));
    };

    let not_a_struct = || ConfigError::NotAStruct {
        key: full_key.to_owned(),
        segment: part.to_owned(),
    };

    let fields = get_struct_fields(poke.shape()).ok_or_else(not_a_struct)?;
    let index = fields
        .iter()
        .position(|f| field_key_name(f) == part)
        .ok_or_else(|| ConfigError::UnknownKey(full_key.to_owned()))?;
    let field_shape = fields[index].shape();

    let mut poke_struct = poke.into_struct().map_err(|_| not_a_struct())?;
    let field_poke = expect_reflect(poke_struct.field(index));

    if rest.is_empty() {
        return apply_action_to_field(field_poke, field_shape, action);
    }

    match field_shape.def {
        Def::Option(_) => {
            let mut option = expect_reflect(field_poke.into_option());
            if option.is_none() {
                let group = default_group(option.def().t());
                expect_reflect(option.set_some_from_heap(group));
            }
            let inner = option
                .value_mut()
                .expect("the group was just initialised, so it is set");
            poke_path(inner, rest, full_key, action)
        }
        _ if is_config_group(field_shape) => poke_path(field_poke, rest, full_key, action),
        _ => Err(not_a_struct()),
    }
}

fn apply_action_to_field(
    field_poke: Poke<'_, 'static>,
    field_shape: &'static Shape,
    action: FieldAction,
) -> Result<(), ConfigError> {
    assert!(
        matches!(field_shape.def, Def::Option(_)),
        "Generated DTOs wrap every leaf in Option, but found non-Option field of type '{field_shape}'"
    );

    let mut option = expect_reflect(field_poke.into_option());
    match action {
        FieldAction::Set(value) => {
            expect_reflect(option.set_some_from_heap(value));
            Ok(())
        }
        FieldAction::Unset => {
            option.set_none();
            Ok(())
        }
    }
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

/// Finds the facet `Field` descriptor for the leaf at the end of a dotted key.
fn find_leaf_field(shape: &'static Shape, key: &str) -> Option<&'static facet::Field> {
    let parts: Vec<&str> = key.split('.').collect();
    find_leaf_field_parts(shape, &parts)
}

fn find_leaf_field_parts(shape: &'static Shape, parts: &[&str]) -> Option<&'static facet::Field> {
    let (&part, rest) = parts.split_first()?;

    let fields = get_struct_fields(shape)?;
    let field = fields.iter().find(|f| field_key_name(f) == part)?;

    if rest.is_empty() {
        Some(field)
    } else {
        let field_shape = field.shape();
        let inner = if let Def::Option(opt_def) = field_shape.def {
            opt_def.t
        } else {
            field_shape
        };
        find_leaf_field_parts(inner, rest)
    }
}

fn validate_key(shape: &'static Shape, key: &str) -> Result<(), ConfigError> {
    leaf_shape_of(shape, key)?;
    Ok(())
}

/// Returns the shape stored at `key`, unwrapping the leaf's `Option`.
fn leaf_shape_of(shape: &'static Shape, key: &str) -> Result<&'static Shape, ConfigError> {
    find_leaf_shape(shape, key).ok_or_else(|| ConfigError::UnknownKey(key.to_owned()))
}

/// Reads `tedge::example` attributes from a field, returning the example values.
fn field_examples(field: &facet::Field) -> Vec<&'static str> {
    field
        .attributes
        .iter()
        .filter(|a| a.ns == Some(attrs::NS) && a.key == "example")
        .filter_map(|a| a.get_as::<&str>().copied())
        .collect()
}

/// Tracks key prefixes during the shape-tree walk in [`collect_aliases`].
///
/// `walk` is the full dotted prefix accumulated field by field.
/// `schema_root` is the prefix at which the current `define_config!`
/// schema was entered — deprecated key values are full paths within
/// their own schema and need this prefix to become absolute.
struct AliasWalkContext<'a> {
    walk: &'a str,
    schema_root: &'a str,
}

/// Walks the DTO shape tree collecting `tedge::deprecated_key` aliases.
fn collect_aliases(
    shape: &'static Shape,
    ctx: &AliasWalkContext<'_>,
    aliases: &mut Vec<DeprecatedKey>,
) {
    let fields = match get_struct_fields(shape) {
        Some(f) => f,
        None => return,
    };

    for field in fields {
        let field_key = dotted_key(ctx.walk, field_key_name(field));

        let inner_shape = if let Def::Option(opt_def) = field.shape().def {
            opt_def.t
        } else {
            field.shape()
        };

        if matches!(inner_shape.def, Def::Map(_)) {
            continue;
        } else if is_config_group(inner_shape) {
            let schema_root = if is_schema_root(inner_shape) {
                &field_key
            } else {
                ctx.schema_root
            };
            let child = AliasWalkContext {
                walk: &field_key,
                schema_root,
            };
            collect_aliases(inner_shape, &child, aliases);
        } else if let Some(attr) = field.get_attr(Some(attrs::NS), "deprecated_key") {
            if let Some(&old_key) = attr.get_as::<&str>() {
                aliases.push(DeprecatedKey {
                    old: dotted_key(ctx.schema_root, old_key),
                    new: field_key,
                });
            }
        }
    }
}

fn is_schema_root(shape: &Shape) -> bool {
    shape
        .attributes
        .iter()
        .any(|a| a.ns == Some(attrs::NS) && a.key == "schema_root")
}

fn list_keys_recursive(shape: &'static Shape, prefix: &str, entries: &mut Vec<KeyEntry>) {
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
            list_keys_recursive(inner_shape, &field_key, entries);
        } else {
            entries.push(KeyEntry {
                key: field_key,
                doc: field.doc,
                examples: field_examples(field),
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_invalid_explicit_value_names_the_key_alone() {
        assert_eq!(
            invalid("mqtt.port", "mqtt.port", ValueOrigin::Explicit).to_string(),
            "Invalid value for 'mqtt.port': '188x' — invalid digit found in string"
        );
    }

    #[test]
    fn an_invalid_built_in_default_is_reported_as_a_bug() {
        assert_eq!(
            invalid("mqtt.port", "mqtt.port", ValueOrigin::SchemaDefault).to_string(),
            "The built-in default for 'mqtt.port' is not a valid value: '188x' — \
             invalid digit found in string. This is a bug in thin-edge.io."
        );
    }

    #[test]
    fn an_inherited_invalid_value_names_the_key_it_fell_back_to() {
        assert_eq!(
            invalid("mqtt.tls_port", "mqtt.port", ValueOrigin::Explicit).to_string(),
            "'mqtt.tls_port' is not set, so it falls back to 'mqtt.port', whose value \
             '188x' is not valid: invalid digit found in string"
        );
    }

    #[test]
    fn an_inherited_invalid_default_names_both_keys_and_is_reported_as_a_bug() {
        assert_eq!(
            invalid("mqtt.tls_port", "mqtt.port", ValueOrigin::SchemaDefault).to_string(),
            "'mqtt.tls_port' is not set, so it falls back to 'mqtt.port', whose built-in \
             default '188x' is not valid: invalid digit found in string. \
             This is a bug in thin-edge.io."
        );
    }

    #[test]
    fn an_invalid_derived_value_names_the_source_key_and_its_value() {
        let origin = ValueOrigin::Derived {
            source_value: "1883".into(),
        };
        assert_eq!(
            invalid("mqtt.tls_port", "mqtt.port", origin).to_string(),
            "'mqtt.tls_port' is derived from 'mqtt.port' ('1883'); the derived value \
             '188x' is not valid: invalid digit found in string"
        );
    }

    #[test]
    fn an_invalid_value_from_the_root_config_names_the_root_key() {
        assert_eq!(
            invalid("mqtt.port", "mqtt.bind_port", ValueOrigin::Root).to_string(),
            "'mqtt.port' falls back to the root config key 'mqtt.bind_port', whose value \
             '188x' is not valid: invalid digit found in string"
        );
    }

    #[test]
    fn a_short_cycle_is_shown_in_full() {
        let cycle = keys(&["mqtt.a", "mqtt.b", "mqtt.c", "mqtt.a"]);

        assert_eq!(
            ConfigError::DefaultCycle { cycle }.to_string(),
            "The default for 'mqtt.a' is cyclic: mqtt.a -> mqtt.b -> mqtt.c -> mqtt.a"
        );
    }

    #[test]
    fn a_cycle_of_the_maximum_length_is_still_shown_in_full() {
        let cycle: Vec<String> = (0..MAX_CYCLE_KEYS).map(|n| format!("k{n}")).collect();

        assert_eq!(
            format_cycle(&cycle),
            "k0 -> k1 -> k2 -> k3 -> k4 -> k5 -> k6 -> k7 -> k8 -> k9"
        );
    }

    #[test]
    fn an_enormous_cycle_keeps_both_ends_and_counts_what_it_elides() {
        let cycle: Vec<String> = (0..20).map(|n| format!("k{n}")).collect();

        assert_eq!(
            format_cycle(&cycle),
            "k0 -> k1 -> k2 -> k3 -> ... (13 more) -> k17 -> k18 -> k19"
        );
    }

    fn keys(keys: &[&str]) -> Vec<String> {
        keys.iter().map(|k| (*k).to_owned()).collect()
    }

    fn invalid(key: &str, source_key: &str, origin: ValueOrigin) -> ConfigError {
        ConfigError::InvalidValue {
            key: key.to_owned(),
            source_key: source_key.to_owned(),
            origin,
            value: "188x".to_owned(),
            reason: "invalid digit found in string".to_owned(),
        }
    }
}
