use crate::append_remove::AppendRemoveItem;
use facet::Facet;
use std::collections::BTreeSet;
use std::convert::Infallible;
use std::str::FromStr;

/// Ordered set of template names that accepts TOML arrays and CLI comma lists.
#[derive(Clone, Debug, Default, Facet, serde::Serialize, serde::Deserialize, Eq, PartialEq)]
#[serde(from = "FromTomlOrCli")]
pub struct TemplatesSet(pub Vec<String>);

#[derive(serde::Deserialize)]
#[serde(from = "String")]
struct CommaDelimited(Vec<String>);

#[derive(serde::Deserialize)]
#[serde(untagged)]
enum FromTomlOrCli {
    Toml(Vec<String>),
    Cli(CommaDelimited),
}

impl From<String> for CommaDelimited {
    fn from(value: String) -> Self {
        Self(
            value
                .split(',')
                .map(|s| s.trim().to_owned())
                .filter(|s| !s.is_empty())
                .collect(),
        )
    }
}

impl From<FromTomlOrCli> for TemplatesSet {
    fn from(value: FromTomlOrCli) -> Self {
        match value {
            FromTomlOrCli::Toml(entries) => Self(entries),
            FromTomlOrCli::Cli(CommaDelimited(entries)) => Self(entries),
        }
    }
}

impl From<Vec<String>> for TemplatesSet {
    fn from(value: Vec<String>) -> Self {
        TemplatesSet(value)
    }
}

impl From<TemplatesSet> for String {
    fn from(val: TemplatesSet) -> Self {
        val.to_string()
    }
}

impl From<String> for TemplatesSet {
    fn from(val: String) -> Self {
        Self::from(val.as_str())
    }
}

impl<'a> From<&'a str> for TemplatesSet {
    fn from(val: &'a str) -> Self {
        let strings = val
            .split(',')
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .map(|s| s.into())
            .collect();
        TemplatesSet(strings)
    }
}

impl FromStr for TemplatesSet {
    type Err = Infallible;
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Ok(Self::from(value))
    }
}

impl std::fmt::Display for TemplatesSet {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut first = true;
        for s in &self.0 {
            if !first {
                f.write_str(",")?;
            }
            f.write_str(s)?;
            first = false;
        }
        Ok(())
    }
}

impl AppendRemoveItem for TemplatesSet {
    fn append(current: Option<Self>, new_value: Self) -> Option<Self> {
        if let Some(current) = current {
            Some(TemplatesSet(
                current
                    .0
                    .into_iter()
                    .chain(new_value.0)
                    .collect::<BTreeSet<String>>()
                    .into_iter()
                    .collect(),
            ))
        } else {
            Some(new_value)
        }
    }

    fn remove(mut current: Option<Self>, remove_value: Self) -> Option<Self> {
        if let Some(ref mut current) = current {
            let to_remove = BTreeSet::from_iter(remove_value.0);
            current.0.retain(|value| !to_remove.contains(value));
        }
        current
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_string_parses_to_empty_set() {
        let ts: TemplatesSet = "".parse().unwrap();
        assert_eq!(ts.0, Vec::<String>::new());
    }

    #[test]
    fn append_to_none_sets_value() {
        let new = TemplatesSet::from("t1,t2");
        let result = TemplatesSet::append(None, new);
        assert_eq!(result.unwrap().0, vec!["t1", "t2"]);
    }

    #[test]
    fn append_merges_and_deduplicates() {
        let current = TemplatesSet::from("t1,t2");
        let new = TemplatesSet::from("t2,t3");
        let result = TemplatesSet::append(Some(current), new).unwrap();
        assert_eq!(result.0, vec!["t1", "t2", "t3"]);
    }

    #[test]
    fn remove_deletes_matching_entries() {
        let current = TemplatesSet::from("t1,t2,t3");
        let to_remove = TemplatesSet::from("t2");
        let result = TemplatesSet::remove(Some(current), to_remove).unwrap();
        assert_eq!(result.0, vec!["t1", "t3"]);
    }

    #[test]
    fn remove_last_item_leaves_empty_set() {
        let current = TemplatesSet::from("t1");
        let to_remove = TemplatesSet::from("t1");
        let result = TemplatesSet::remove(Some(current), to_remove).unwrap();
        assert_eq!(result.0, Vec::<String>::new());
    }

    #[test]
    fn remove_from_none_returns_none() {
        let to_remove = TemplatesSet::from("t1");
        let result = TemplatesSet::remove(None, to_remove);
        assert!(result.is_none());
    }
}
