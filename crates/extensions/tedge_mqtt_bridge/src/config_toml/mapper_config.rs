//! Trait for mapper config lookup during bridge rule expansion.
//!
//! Defines [`MapperConfigLookup`], a trait that abstracts over how `${mapper.*}`
//! template references are resolved. This lets higher-level types (e.g.
//! `EffectiveMapperConfig` in `tedge_mapper`) provide accurate provenance
//! information without introducing a crate dependency in the wrong direction.
//!
//! [`TableMapperLookup`] is a simple implementation backed by a `toml::Table`,
//! used in tests and in situations where no richer config is available.

/// Result of a scalar key lookup in a mapper config.
#[derive(Debug, Clone)]
pub enum MapperKeyResult {
    /// Key was found and has a scalar value.
    Value(String),
    /// Key is recognised (in the mapper schema) but has no configured value.
    /// `help` tells the user where to configure it.
    /// `display_key` overrides the path in the error message (e.g. `"c8y.url"` instead of `"url"`).
    NotSet {
        help: Option<String>,
        display_key: Option<String>,
    },
    /// Key is not recognised — likely a typo or unsupported field.
    UnknownKey,
    /// The value exists but is not a scalar (it's a table, array, etc.).
    NotScalar,
    /// An intermediate path segment exists but is not a table.
    /// E.g. looking up `bridge.topic` where `bridge` is already a string.
    BadIntermediatePath { intermediate: String },
}

/// Result of an array key lookup in a mapper config.
#[derive(Debug, Clone)]
pub enum MapperArrayResult {
    /// Key was found and is an array of strings.
    Values(Vec<String>),
    /// Key was found but is not an array.
    NotAnArray,
    /// Array was found but contains a non-string element.
    BadElement { element: String },
    /// Key is recognised but has no configured value.
    /// `display_key` overrides the path in the error message.
    NotSet {
        help: Option<String>,
        display_key: Option<String>,
    },
    /// Key is not recognised — likely a typo.
    UnknownKey,
    /// An intermediate path segment is not a table.
    BadIntermediatePath { intermediate: String },
}

/// Abstraction over a mapper config source for template expansion.
///
/// Bridge rule templates use `${mapper.some.key}` and `@mapper.some.key`
/// (for-loop sources) references. Implementations provide the value for a
/// given dotted path and classify absent keys as either [`MapperKeyResult::NotSet`]
/// (recognised schema field with no value) or [`MapperKeyResult::UnknownKey`]
/// (not in schema — likely a typo), enabling precise, actionable error messages.
pub trait MapperConfigLookup: Send + Sync {
    /// Look up a scalar value at `path` (e.g. `"url"`, `"device.id"`).
    fn lookup_scalar(&self, path: &str) -> MapperKeyResult;

    /// Look up an array value at `path` (used for `for` loop sources).
    fn lookup_array(&self, path: &str) -> MapperArrayResult;
}

/// Result of walking a dotted path through a TOML table.
///
/// Shared between [`TableMapperLookup`] and `EffectiveMapperConfig` (in `tedge_mapper`)
/// to avoid duplicating the intermediate-table traversal logic.
pub enum WalkResult<'a> {
    /// Successfully navigated to the leaf value.
    Found(&'a toml::Value),
    /// An intermediate segment is present but is not a table.
    BadIntermediatePath { intermediate: String },
    /// A segment was not found in the table.
    NotFound,
}

/// Walks a dotted path through a TOML table, returning the leaf value.
pub fn walk_toml_path<'a>(table: &'a toml::Table, path: &str) -> WalkResult<'a> {
    let parts: Vec<&str> = path.split('.').collect();
    let mut current = table;

    for &part in &parts[..parts.len() - 1] {
        match current.get(part) {
            Some(toml::Value::Table(t)) => current = t,
            Some(_) => {
                return WalkResult::BadIntermediatePath {
                    intermediate: part.to_owned(),
                }
            }
            None => return WalkResult::NotFound,
        }
    }

    let last = parts.last().expect("path must have at least one part");
    match current.get(*last) {
        Some(v) => WalkResult::Found(v),
        None => WalkResult::NotFound,
    }
}

/// Converts a scalar TOML value to its string representation for template expansion.
///
/// Returns `Some(string)` for scalars (string, integer, float, boolean, datetime)
/// and `None` for non-scalar types (tables, arrays).
pub fn toml_scalar_to_string(value: &toml::Value) -> Option<String> {
    match value {
        toml::Value::String(s) => Some(s.clone()),
        toml::Value::Integer(i) => Some(i.to_string()),
        toml::Value::Float(f) => Some(f.to_string()),
        toml::Value::Boolean(b) => Some(b.to_string()),
        toml::Value::Datetime(d) => Some(d.to_string()),
        _ => None,
    }
}

/// Collects a TOML array of strings into a `Vec<String>`.
///
/// Returns the appropriate [`MapperArrayResult`] variant:
/// - `Values` if all elements are strings
/// - `BadElement` if a non-string element is encountered
/// - `NotAnArray` if the value is not an array
pub fn collect_string_array(value: &toml::Value) -> MapperArrayResult {
    match value {
        toml::Value::Array(arr) => {
            let mut result = Vec::with_capacity(arr.len());
            for val in arr {
                match val {
                    toml::Value::String(s) => result.push(s.clone()),
                    other => {
                        return MapperArrayResult::BadElement {
                            element: other.to_string(),
                        }
                    }
                }
            }
            MapperArrayResult::Values(result)
        }
        _ => MapperArrayResult::NotAnArray,
    }
}

/// A [`MapperConfigLookup`] backed by a raw `toml::Table`.
///
/// Used in tests and for cases where no `EffectiveMapperConfig` is available.
/// Missing keys are always reported as [`MapperKeyResult::UnknownKey`] since a
/// plain TOML table carries no schema information.
pub struct TableMapperLookup(pub toml::Table);

impl MapperConfigLookup for TableMapperLookup {
    fn lookup_scalar(&self, path: &str) -> MapperKeyResult {
        match walk_toml_path(&self.0, path) {
            WalkResult::Found(v) => match toml_scalar_to_string(v) {
                Some(s) => MapperKeyResult::Value(s),
                None => MapperKeyResult::NotScalar,
            },
            WalkResult::BadIntermediatePath { intermediate } => {
                MapperKeyResult::BadIntermediatePath { intermediate }
            }
            WalkResult::NotFound => MapperKeyResult::UnknownKey,
        }
    }

    fn lookup_array(&self, path: &str) -> MapperArrayResult {
        match walk_toml_path(&self.0, path) {
            WalkResult::Found(v) => collect_string_array(v),
            WalkResult::BadIntermediatePath { intermediate } => {
                MapperArrayResult::BadIntermediatePath { intermediate }
            }
            WalkResult::NotFound => MapperArrayResult::UnknownKey,
        }
    }
}
