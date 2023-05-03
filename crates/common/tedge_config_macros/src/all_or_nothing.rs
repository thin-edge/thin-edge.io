use crate::OptionalConfig;

pub fn all_or_nothing<T, U>(
    t: OptionalConfig<T>,
    u: OptionalConfig<U>,
) -> Result<Option<(T, U)>, String> {
    use OptionalConfig::*;

    match (t, u) {
        (Present { value: t, .. }, Present { value: u, .. }) => Ok(Some((t, u))),
        (Empty(..), Empty(..)) => Ok(None),
        (t, u) => {
            let all_settings = [t.key(), u.key()];
            let present = [t.key_if_present(), u.key_if_present()]
                .into_iter()
                .filter_map(|id| id)
                .collect::<Vec<_>>();
            let missing = [t.key_if_empty(), u.key_if_empty()]
                .into_iter()
                .filter_map(|id| id)
                .collect::<Vec<_>>();
            Err(format!(
     "The thin-edge configuration is invalid. The settings {all_settings:?} must either all be configured, or all unset. Currently {present:?} are set, and {missing:?} are unset."))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_or_nothing_returns_some_when_both_values_are_configured() {
        assert_eq!(
            all_or_nothing(
                OptionalConfig::Present {
                    value: "first",
                    key: "test.key"
                },
                OptionalConfig::Present {
                    value: "second",
                    key: "test.key2"
                }
            ),
            Ok(Some(("first", "second")))
        )
    }

    #[test]
    fn all_or_nothing_returns_none_when_both_values_when_neither_value_is_configured() {
        assert_eq!(
            all_or_nothing::<String, String>(
                OptionalConfig::Empty("first.key"),
                OptionalConfig::Empty("second.key"),
            ),
            Ok(None)
        )
    }

    #[test]
    fn all_or_nothing_returns_an_error_if_only_the_first_value_is_configured() {
        assert!(matches!(
            all_or_nothing::<_, String>(
                OptionalConfig::Present {
                    value: "test",
                    key: "first.key"
                },
                OptionalConfig::Empty("second.key"),
            ),
            Err(_)
        ))
    }

    #[test]
    fn all_or_nothing_returns_an_error_if_only_the_second_value_is_configured() {
        assert!(matches!(
            all_or_nothing::<String, _>(
                OptionalConfig::Empty("first.key"),
                OptionalConfig::Present {
                    value: "test",
                    key: "second.key"
                },
            ),
            Err(_)
        ))
    }
}
