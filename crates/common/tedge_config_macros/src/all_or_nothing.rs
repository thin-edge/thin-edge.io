use crate::OptionalConfig;

pub trait MultiOption {
    type Output;
    fn extract_all(self) -> Result<Option<Self::Output>, PartialConfiguration>;
}

/// The keys which were and weren't provided as part of an all or nothing group
pub struct PartialConfiguration {
    present: Vec<&'static str>,
    missing: Vec<&'static str>,
}

impl PartialConfiguration {
    fn error_message(&self) -> String {
        let mut all_settings = self.present.clone();
        all_settings.append(&mut self.missing.clone());
        let present = &self.present;
        let missing = &self.missing;

        format!(
            "The thin-edge configuration is invalid. The settings {all_settings:?} \
            must either all be configured, or all unset. Currently {present:?} are \
            set, and {missing:?} are unset."
        )
    }
}

impl<T, U> MultiOption for (OptionalConfig<T>, OptionalConfig<U>) {
    type Output = (T, U);
    fn extract_all(self) -> Result<Option<Self::Output>, PartialConfiguration> {
        use OptionalConfig::*;
        match self {
            (Present { value: t, .. }, Present { value: u, .. }) => Ok(Some((t, u))),
            (Empty(..), Empty(..)) => Ok(None),
            (t, u) => {
                let present = [t.key_if_present(), u.key_if_present()]
                    .into_iter()
                    .flatten()
                    .collect::<Vec<_>>();
                let missing = [t.key_if_empty(), u.key_if_empty()]
                    .into_iter()
                    .flatten()
                    .collect::<Vec<_>>();
                Err(PartialConfiguration { present, missing })
            }
        }
    }
}

pub fn all_or_nothing<Configs: MultiOption>(
    input: Configs,
) -> Result<Option<Configs::Output>, String> {
    input
        .extract_all()
        .map_err(|partial_config| partial_config.error_message())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_or_nothing_returns_some_when_both_values_are_configured() {
        assert_eq!(
            all_or_nothing((
                OptionalConfig::Present {
                    value: "first",
                    key: "test.key"
                },
                OptionalConfig::Present {
                    value: "second",
                    key: "test.key2"
                }
            )),
            Ok(Some(("first", "second")))
        )
    }

    #[test]
    fn all_or_nothing_returns_none_when_both_values_when_neither_value_is_configured() {
        assert_eq!(
            all_or_nothing((
                OptionalConfig::<String>::Empty("first.key"),
                OptionalConfig::<String>::Empty("second.key")
            )),
            Ok(None)
        )
    }

    #[test]
    fn all_or_nothing_returns_an_error_if_only_the_first_value_is_configured() {
        assert!(matches!(
            all_or_nothing((
                OptionalConfig::Present {
                    value: "test",
                    key: "first.key"
                },
                OptionalConfig::<String>::Empty("second.key")
            )),
            Err(_)
        ))
    }

    #[test]
    fn all_or_nothing_returns_an_error_if_only_the_second_value_is_configured() {
        assert!(matches!(
            all_or_nothing((
                OptionalConfig::<String>::Empty("first.key"),
                OptionalConfig::Present {
                    value: "test",
                    key: "second.key"
                },
            )),
            Err(_)
        ))
    }
}
