use crate::tedge_toml::AppendRemoveItem;
use serde::Deserialize;
use serde::Serialize;
use std::collections::BTreeMap;
use std::str::FromStr;

#[derive(thiserror::Error, Debug)]
pub enum NotCountPerGroup {
    #[error("Not a comma-separated list of name:count pairs")]
    InvalidPairs,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(from = "Vec<GroupCountPair>")]
#[serde(into = "Vec<String>")]
pub struct CountPerGroup {
    groups: BTreeMap<String, u32>,
}

impl CountPerGroup {
    pub fn default_count(&self) -> Option<u32> {
        self.groups.get("*").copied()
    }

    pub fn count(&self, group: &str) -> Option<u32> {
        self.groups
            .get(group)
            .copied()
            .or_else(|| self.default_count())
    }

    pub fn count_per_groups(&self) -> impl Iterator<Item = (&str, u32)> {
        self.groups
            .iter()
            .filter(|(k, _)| *k != "*")
            .map(|(k, v)| (k.as_ref(), *v))
    }
}

impl doku::Document for CountPerGroup {
    fn ty() -> doku::Type {
        Vec::<(String, u32)>::ty()
    }
}

impl std::fmt::Display for CountPerGroup {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        "[".fmt(f)?;
        let mut sep = "";
        if let Some(default) = self.default_count() {
            write!(f, "*:{default}")?;
            sep = ", ";
        }
        for (group, count) in self.count_per_groups() {
            write!(f, "{sep}{group}:{count}")?;
            sep = ", ";
        }
        "]".fmt(f)?;
        Ok(())
    }
}

impl FromStr for CountPerGroup {
    type Err = NotCountPerGroup;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let value = value.trim();
        let value = value.strip_prefix('[').unwrap_or(value);
        let value = value.strip_suffix(']').unwrap_or(value);
        let value = value.trim();
        if value.is_empty() {
            return Ok(Self::default());
        }
        let pairs: Result<Vec<_>, NotCountPerGroup> =
            value.split(',').map(GroupCountPair::try_from).collect();
        Ok(pairs?.into())
    }
}

impl From<Vec<GroupCountPair>> for CountPerGroup {
    fn from(val: Vec<GroupCountPair>) -> Self {
        let groups = val.into_iter().map(|p| (p.group, p.count)).collect();
        CountPerGroup { groups }
    }
}

impl From<CountPerGroup> for Vec<String> {
    fn from(value: CountPerGroup) -> Vec<String> {
        let mut pairs = vec![];
        if let Some(default) = value.default_count() {
            pairs.push(format!("*:{default}"));
        }
        for (group, count) in value.count_per_groups() {
            pairs.push(format!("{group}:{count}"));
        }
        pairs
    }
}

#[derive(Deserialize)]
#[serde(try_from = "String")]
struct GroupCountPair {
    group: String,
    count: u32,
}

impl<'a> TryFrom<&'a str> for GroupCountPair {
    type Error = NotCountPerGroup;

    fn try_from(value: &'a str) -> Result<Self, Self::Error> {
        let value = value.trim();
        let value = value.strip_prefix('"').unwrap_or(value);
        let value = value.strip_suffix('"').unwrap_or(value);
        let value = value.trim();
        if value.is_empty() {
            return Err(NotCountPerGroup::InvalidPairs);
        }
        let (group, count) = match value.split_once(":") {
            None => (value, "1"),
            Some((g, c)) => (g.trim(), c),
        };
        let Ok(count) = count.parse::<u32>() else {
            return Err(NotCountPerGroup::InvalidPairs);
        };
        Ok(GroupCountPair {
            group: group.to_owned(),
            count,
        })
    }
}

impl TryFrom<String> for GroupCountPair {
    type Error = NotCountPerGroup;

    fn try_from(val: String) -> Result<Self, Self::Error> {
        Self::try_from(val.as_str())
    }
}

impl AppendRemoveItem for CountPerGroup {
    type Item = CountPerGroup;

    fn append(current_value: Option<Self::Item>, new_value: Self::Item) -> Option<Self::Item> {
        match current_value {
            None => Some(new_value),
            Some(current_value) => {
                let mut groups = current_value.groups;
                for (group, count) in new_value.groups {
                    groups.insert(group, count);
                }
                Some(CountPerGroup { groups })
            }
        }
    }

    fn remove(current_value: Option<Self::Item>, remove_value: Self::Item) -> Option<Self::Item> {
        current_value.map(|mut current_value| {
            let groups = &mut current_value.groups;
            for group in remove_value.groups.keys() {
                groups.remove(group);
            }
            current_value
        })
    }
}
