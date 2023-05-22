use std::borrow::Cow;
use std::collections::HashMap;

fn dot_separate(prefix: Option<&str>, field: &str, sub_path: &str) -> Cow<'static, str> {
    Cow::Owned(
        prefix
            .into_iter()
            .chain([field, sub_path])
            .collect::<Vec<_>>()
            .join("."),
    )
}

/// Creates a map from aliases to canonical keys
pub fn struct_field_aliases(
    prefix: Option<&str>,
    fields: &[(&'static str, doku::Field)],
) -> HashMap<Cow<'static, str>, Cow<'static, str>> {
    fields
        .iter()
        .flat_map(|(field_name, field)| match named_fields(&field.ty.kind) {
            Some(fields) => {
                // e.g. normal_field.alias
                struct_field_aliases(Some(&key_name(prefix, field_name)), fields)
                    .into_iter()
                    // e.g. alias.normal_field
                    .chain(conventional_sub_paths(field, prefix, field_name, fields))
                    // e.g. alias.other_alias
                    .chain(aliased_sub_paths(field, prefix, field_name, fields))
                    .collect::<HashMap<_, _>>()
            }
            None => field
                .aliases
                .iter()
                .map(|alias| (key_name(prefix, alias), key_name(prefix, field_name)))
                .collect(),
        })
        .collect()
}

fn aliased_sub_paths(
    field: &doku::Field,
    prefix: Option<&str>,
    field_name: &str,
    sub_fields: &[(&'static str, doku::Field)],
) -> Vec<(Cow<'static, str>, Cow<'static, str>)> {
    field
        .aliases
        .iter()
        .flat_map(|alias| {
            // e.g. alias.another_alias
            struct_field_aliases(None, sub_fields).into_iter().map(
                move |(nested_alias, resolved_subpath)| {
                    (
                        dot_separate(prefix, alias, &nested_alias),
                        dot_separate(prefix, field_name, &resolved_subpath),
                    )
                },
            )
        })
        .collect()
}

fn conventional_sub_paths(
    field: &doku::Field,
    prefix: Option<&str>,
    name: &str,
    sub_fields: &[(&'static str, doku::Field)],
) -> Vec<(Cow<'static, str>, Cow<'static, str>)> {
    field
        .aliases
        .iter()
        .flat_map(|alias| {
            // e.g. alias.normal_field
            struct_field_paths(None, sub_fields)
                .into_iter()
                .map(move |(path, _ty)| {
                    (
                        dot_separate(prefix, alias, &path),
                        dot_separate(prefix, name, &path),
                    )
                })
        })
        .collect()
}

/// Creates a "map" from keys to their doku type information
pub fn struct_field_paths(
    prefix: Option<&str>,
    fields: &[(&'static str, doku::Field)],
) -> Vec<(Cow<'static, str>, doku::Type)> {
    fields
        .iter()
        .flat_map(|(name, field)| match named_fields(&field.ty.kind) {
            Some(fields) => struct_field_paths(Some(&key_name(prefix, name)), fields),
            None => vec![(key_name(prefix, name), field.ty.clone())],
        })
        .collect()
}

fn key_name(prefix: Option<&str>, name: &'static str) -> Cow<'static, str> {
    match prefix {
        Some(prefix) => Cow::Owned(format!("{}.{}", prefix, name)),
        None => Cow::Borrowed(name),
    }
}

fn named_fields(kind: &doku::TypeKind) -> Option<&[(&'static str, doku::Field)]> {
    match kind {
        doku::TypeKind::Struct {
            fields: doku::Fields::Named { fields },
            transparent: false,
        } => Some(fields),
        _ => None,
    }
}
