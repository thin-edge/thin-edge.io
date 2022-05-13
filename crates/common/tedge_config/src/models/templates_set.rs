use std::convert::TryInto;

/// Represents a set of smartrest templates.
///
/// New type to add conversion methods and deduplicate provided templates.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, Eq, PartialEq)]
#[serde(transparent)]
pub struct TemplatesSet(pub Vec<String>);

#[derive(thiserror::Error, Debug)]
#[error("FilePath to String conversion failed: {0:?}")]
pub struct TemplatesSetToStringConversionFailure(String);

impl TryFrom<Vec<String>> for TemplatesSet {
    type Error = TemplatesSetToStringConversionFailure;

    fn try_from(value: Vec<String>) -> Result<Self, Self::Error> {
        let set = value
            .iter()
            .flat_map(|s| {
                // Smartrest templates should be deserialized as:
                // c8y/s/uc/template-1 (in from localhost), s/uc/template-1
                // c8y/s/dc/template-1 (out to localhost), s/dc/template-1
                [
                    format!(r#"c8y/s/uc/{s} out 2 c8y/ """#),
                    format!(r#"c8y/s/dc/{s} in 2 c8y/ """#),
                ]
                .into_iter()
            })
            .collect::<Vec<String>>();
        Ok(TemplatesSet(set))
    }
}

impl TryFrom<Vec<&str>> for TemplatesSet {
    type Error = TemplatesSetToStringConversionFailure;

    fn try_from(value: Vec<&str>) -> Result<Self, Self::Error> {
        let set = value
            .iter()
            .flat_map(|s| {
                // Smartrest templates should be deserialized as:
                // c8y/s/uc/template-1 (in from localhost), s/uc/template-1
                // c8y/s/dc/template-1 (out to localhost), s/dc/template-1
                [
                    format!(r#"c8y/s/uc/{s} out 2 c8y/ """#),
                    format!(r#"c8y/s/dc/{s} in 2 c8y/ """#),
                ]
                .into_iter()
            })
            .collect::<Vec<String>>();
        Ok(TemplatesSet(set))
    }
}

impl TryInto<Vec<String>> for TemplatesSet {
    type Error = TemplatesSetToStringConversionFailure;

    fn try_into(self) -> Result<Vec<String>, TemplatesSetToStringConversionFailure> {
        Ok(self.0)
    }
}

impl From<TemplatesSet> for String {
    fn from(val: TemplatesSet) -> Self {
        val.to_string()
    }
}

impl From<String> for TemplatesSet {
    fn from(val: String) -> Self {
        let strings = val.split(',').map(|ss| ss.into()).collect();
        TemplatesSet(strings)
    }
}

impl std::fmt::Display for TemplatesSet {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self.0)
    }
}

#[cfg(test)]
mod test {
    use super::TemplatesSet;

    #[test]
    fn conversion_from_strings() {
        let strings = vec!["template-1", "template-2"];
        let expected = vec![
            r#"c8y/s/uc/template-1 out 2 c8y/ """#,
            r#"c8y/s/dc/template-1 in 2 c8y/ """#,
            r#"c8y/s/uc/template-2 out 2 c8y/ """#,
            r#"c8y/s/dc/template-2 in 2 c8y/ """#,
        ];

        let res = TemplatesSet::try_from(strings).unwrap();

        assert_eq!(res.0, expected);
    }
}
