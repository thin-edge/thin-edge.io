use std::collections::BTreeSet;
use std::fmt;
use std::str::FromStr;

use tedge_config_engine::AppendRemoveItem;

/// A set of names, such as SmartREST template IDs or MQTT topics
///
/// `tedge config` takes it comma separated and shows it as a list, `["a",
/// "b"]`; a TOML file holds it as an array or as a comma-separated string
#[derive(Debug, Clone, Default, PartialEq, Eq, facet::Facet)]
#[facet(proxy = TomlTemplatesSet)]
pub struct TemplatesSet(pub Vec<String>);

impl FromStr for TemplatesSet {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        // `add` and `remove` reparse the value as it is shown, so the list
        // form is read back as the list it shows
        if value.starts_with('[') {
            return parse_shown_list(value);
        }
        Ok(Self(value.split(',').map(<_>::to_owned).collect()))
    }
}

impl fmt::Display for TemplatesSet {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}", self.0)
    }
}

impl AppendRemoveItem for TemplatesSet {
    fn append(current: Option<Self>, new_value: Self) -> Option<Self> {
        match current {
            Some(current) => Some(Self(
                current
                    .0
                    .into_iter()
                    .chain(new_value.0)
                    .collect::<BTreeSet<_>>()
                    .into_iter()
                    .collect(),
            )),
            None => Some(new_value),
        }
    }

    fn remove(current: Option<Self>, remove_value: Self) -> Option<Self> {
        let to_remove = BTreeSet::from_iter(remove_value.0);
        current.map(|mut current| {
            current.0.retain(|value| !to_remove.contains(value));
            current
        })
    }
}

fn parse_shown_list(value: &str) -> Result<TemplatesSet, String> {
    let invalid = |e: &dyn fmt::Display| format!("'{value}' is not a list of strings: {e}");
    let table: toml::Table = format!("list = {value}").parse().map_err(|e| invalid(&e))?;
    let toml::Value::Array(items) = &table["list"] else {
        return Err(invalid(&"not an array"));
    };
    items
        .iter()
        .map(|item| {
            item.as_str()
                .map(<_>::to_owned)
                .ok_or_else(|| invalid(&"an item is not a string"))
        })
        .collect::<Result<_, _>>()
        .map(TemplatesSet)
}

/// How a TOML file holds a [TemplatesSet]
#[derive(Debug, Clone, facet::Facet)]
#[repr(u8)]
#[facet(untagged)]
pub enum TomlTemplatesSet {
    List(Vec<String>),
    CommaSeparated(String),
}

impl From<TomlTemplatesSet> for TemplatesSet {
    fn from(value: TomlTemplatesSet) -> Self {
        match value {
            TomlTemplatesSet::List(items) => Self(items),
            TomlTemplatesSet::CommaSeparated(items) => Self(
                items
                    .split(',')
                    .map(|s| s.trim().to_owned())
                    .filter(|s| !s.is_empty())
                    .collect(),
            ),
        }
    }
}

impl From<&TemplatesSet> for TomlTemplatesSet {
    fn from(value: &TemplatesSet) -> Self {
        Self::List(value.0.clone())
    }
}
