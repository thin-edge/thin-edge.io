use crate::optional_error::OptionalError;

pub fn combine_errors<T>(
    items: impl Iterator<Item = Result<T, syn::Error>>,
) -> Result<Vec<T>, syn::Error> {
    let mut error = OptionalError::default();
    let mut successful_values = Vec::new();
    for item in items {
        match item {
            Ok(value) => successful_values.push(value),
            Err(e) => error.combine(e),
        }
    }
    error.try_throw().and(Ok(successful_values))
}

// Based on https://stackoverflow.com/a/56264023
pub fn extract_type_from_result(ty: &syn::Type) -> Option<(&syn::Type, &syn::Type)> {
    use syn::GenericArgument;
    use syn::Path;
    use syn::PathArguments;
    use syn::PathSegment;

    fn extract_type_path(ty: &syn::Type) -> Option<&Path> {
        match *ty {
            syn::Type::Path(ref typepath) if typepath.qself.is_none() => Some(&typepath.path),
            _ => None,
        }
    }

    fn extract_result_segment(path: &Path) -> Option<&PathSegment> {
        let idents_of_path = path.segments.iter().fold(String::new(), |mut acc, v| {
            acc.push_str(&v.ident.to_string());
            acc.push('|');
            acc
        });
        vec!["Result|", "std|result|Result|", "core|result|Result|"]
            .into_iter()
            .find(|s| idents_of_path == *s)
            .and_then(|_| path.segments.last())
    }

    extract_type_path(ty)
        .and_then(extract_result_segment)
        .and_then(|path_seg| {
            let type_params = &path_seg.arguments;
            // It should have only on angle-bracketed param ("<String>"):
            match *type_params {
                PathArguments::AngleBracketed(ref params) => {
                    Some((params.args.first()?, params.args.last()?))
                }
                _ => None,
            }
        })
        .and_then(|generic_arg| match generic_arg {
            (GenericArgument::Type(ok), GenericArgument::Type(err)) => Some((ok, err)),
            _ => None,
        })
}

#[test]
fn extract_type_from_different_results() {
    use syn::parse_quote;
    assert_eq!(
        extract_type_from_result(&parse_quote!(Result<String, Error>)),
        Some((&parse_quote!(String), &parse_quote!(Error)))
    );
    assert_eq!(
        extract_type_from_result(&parse_quote!(::std::result::Result<String, Error>)),
        Some((&parse_quote!(String), &parse_quote!(Error)))
    );
    assert_eq!(
        extract_type_from_result(&parse_quote!(std::result::Result<String, Error>)),
        Some((&parse_quote!(String), &parse_quote!(Error)))
    );
    assert_eq!(
        extract_type_from_result(&parse_quote!(core::result::Result<String, Error>)),
        Some((&parse_quote!(String), &parse_quote!(Error)))
    );
}
