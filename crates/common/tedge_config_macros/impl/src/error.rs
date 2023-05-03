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
