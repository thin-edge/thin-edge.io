pub fn fields_to_csv_string<T>(record: impl IntoIterator<Item = T>) -> String
where
    T: AsRef<str> + AsRef<[u8]>,
{
    let mut writer = csv::Writer::from_writer(vec![]);
    writer
        .write_record(record)
        .expect("write to vec never fails");
    let mut output = writer.into_inner().expect("write to vec never fails");
    output.pop();
    String::from_utf8(output).expect("all input is utf-8")
}

#[cfg(test)]
mod tests {
    use crate::smartrest::csv::fields_to_csv_string;

    #[test]
    fn normal_fields_containing_commas_are_quoted() {
        assert_eq!(fields_to_csv_string(["503", "test,me"]), "503,\"test,me\"");
    }

    #[test]
    fn normal_fields_containing_quotes_are_quoted() {
        let rcd = fields_to_csv_string(["503", r#"A value"with" quotes"#, "field"]);
        assert_eq!(rcd, r#"503,"A value""with"" quotes",field"#);
    }
}
