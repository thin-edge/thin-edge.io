use std::io::Cursor;

use miette::miette;
use miette::IntoDiagnostic;
use serde::de::DeserializeOwned;

pub fn deserialize_csv_record<D: DeserializeOwned>(csv: &str) -> miette::Result<D> {
    let mut reader = csv::ReaderBuilder::new()
        .has_headers(false)
        .from_reader(Cursor::new(csv));

    reader
        .deserialize()
        .next()
        .ok_or_else(|| miette!("No CSV record found"))?
        .into_diagnostic()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deserialize_a_single_csv_record() {
        let (_, actual): (String, String) = deserialize_csv_record("71,abcdef").unwrap();

        assert_eq!(actual, "abcdef");
    }
}
