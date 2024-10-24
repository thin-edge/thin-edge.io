use serde_json::Value;

pub trait Record {
    /// Extract from this record the JSON value pointed by the given path
    fn extract_value(&self, path: &str) -> Option<Value>;

    /// Inject values extracted from the record into a template string
    ///
    /// - Search the template string for path patterns `${...}`
    /// - Replace all these paths by the value extracted from self using the paths
    ///
    /// `"prefix-${.payload.x}-separator-${.payload.y}-suffix"` is replaced by
    /// `"prefix-X-separator-Y-suffix"` in a context where the payload is `{"x":"X", "y":"Y"}`
    fn inject_values_into_template(&self, target: &str) -> String {
        target
            .split_inclusive('}')
            .flat_map(|s| match s.find("${") {
                None => vec![s],
                Some(i) => {
                    let (prefix, template) = s.split_at(i);
                    vec![prefix, template]
                }
            })
            .map(|s| self.replace_path_with_value(s))
            .collect()
    }

    /// Inject values extracted from the record into a vector of target strings.
    fn inject_values_into_templates(&self, targets: &[String]) -> Vec<String> {
        targets
            .iter()
            .map(|arg| self.inject_values_into_template(arg))
            .collect()
    }

    /// Replace a path pattern with the value extracted from the message payload using that path
    ///
    /// `${.payload}` -> the whole message payload
    /// `${.payload.x}` -> the value of x if there is any in the payload
    /// `${.payload.unknown}` -> `${.payload.unknown}` unchanged
    /// `Not a path expression` -> `Not a path expression` unchanged
    fn replace_path_with_value(&self, template: &str) -> String {
        Self::extract_path(template)
            .and_then(|path| self.extract_value(path))
            .map(|v| json_as_string(&v))
            .unwrap_or_else(|| template.to_string())
    }

    /// Extract a path  from a `${ ... }` expression
    ///
    /// Return None if the input is not a path expression
    fn extract_path(input: &str) -> Option<&str> {
        input.strip_prefix("${").and_then(|s| s.strip_suffix('}'))
    }
}

fn json_as_string(value: &Value) -> String {
    match value {
        Value::Null => "null".to_string(),
        Value::Bool(b) => b.to_string(),
        Value::Number(n) => n.to_string(),
        Value::String(s) => s.clone(),
        _ => value.to_string(),
    }
}
