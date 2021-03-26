use crate::*;
use std::convert::TryFrom;

pub trait QuerySetting<T: ConfigSetting> {
    fn query(&self, setting: T) -> ConfigSettingResult<T::Value>;
}

pub trait QueryStringSetting<T: ConfigSetting>: QuerySetting<T> {
    fn query_string(&self, setting: T) -> ConfigSettingResult<String>;

    fn query_string_or_default(
        &self,
        setting: T,
        default: impl Into<String>,
    ) -> ConfigSettingResult<String> {
        match self.query_string(setting) {
            Ok(s) => Ok(s),
            Err(ConfigSettingError::ConfigNotSet { .. }) => Ok(default.into()),
            Err(err) => Err(err),
        }
    }
}

pub trait UpdateSetting<T: ConfigSetting> {
    fn update(&mut self, setting: T, value: T::Value) -> ConfigSettingResult<()>;
}

pub trait UpdateStringSetting<T: ConfigSetting>: UpdateSetting<T> {
    fn update_string(&mut self, setting: T, value: String) -> ConfigSettingResult<()>;
}

pub trait UnsetSetting<T: ConfigSetting> {
    fn unset(&mut self, setting: T) -> ConfigSettingResult<()>;
}

// Provide default implementation when T::Value implements Into<String>
impl<T, C> QueryStringSetting<T> for C
where
    T: ConfigSetting,
    C: QuerySetting<T>,
    T::Value: Into<String>,
{
    fn query_string(&self, setting: T) -> ConfigSettingResult<String> {
        self.query(setting).map(Into::into)
    }
}

// Provide default implementation when T::Value implements TryFrom<String>
impl<T, C, E> UpdateStringSetting<T> for C
where
    T: ConfigSetting,
    C: UpdateSetting<T>,
    T::Value: TryFrom<String, Error = E>,
    E: Into<ConfigSettingError>,
{
    fn update_string(&mut self, setting: T, value: String) -> ConfigSettingResult<()> {
        self.update(
            setting,
            T::Value::try_from(value).map_err(|err| err.into())?,
        )
    }
}
