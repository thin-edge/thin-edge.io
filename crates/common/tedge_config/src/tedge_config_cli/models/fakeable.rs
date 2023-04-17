use camino::Utf8Path;
use camino::Utf8PathBuf;
use serde::Deserialize;
use serde::Serialize;
use std::fmt;
use std::num::NonZeroU16;

#[derive(Copy, Clone, Default, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(transparent)]
/// A wrapper type to add implementations of [fake::Dummy] to `T` in cases where `T: !fake::Dummy`
pub struct Fakeable<T>(pub(crate) T);

impl std::ops::Deref for Fakeable<Utf8PathBuf> {
    type Target = Utf8Path;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl std::ops::Deref for Fakeable<NonZeroU16> {
    type Target = NonZeroU16;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<T: fmt::Display> fmt::Display for Fakeable<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl<T> From<T> for Fakeable<T> {
    fn from(value: T) -> Self {
        Self(value)
    }
}

impl From<&str> for Fakeable<Utf8PathBuf> {
    fn from(value: &str) -> Self {
        Self(value.into())
    }
}

impl TryFrom<u16> for Fakeable<NonZeroU16> {
    type Error = <NonZeroU16 as TryFrom<u16>>::Error;
    fn try_from(value: u16) -> Result<Self, Self::Error> {
        value.try_into().map(Self)
    }
}

#[cfg(test)]
impl fake::Dummy<fake::Faker> for Fakeable<Utf8PathBuf> {
    fn dummy_with_rng<R: rand::Rng + ?Sized>(config: &fake::Faker, rng: &mut R) -> Self {
        Self(Utf8PathBuf::from_path_buf(<_>::dummy_with_rng(config, rng)).unwrap())
    }
}

#[cfg(test)]
impl fake::Dummy<fake::Faker> for Fakeable<NonZeroU16> {
    fn dummy_with_rng<R: rand::Rng + ?Sized>(_config: &fake::Faker, rng: &mut R) -> Self {
        std::iter::repeat_with(|| rng.gen())
            .find_map(NonZeroU16::new)
            .map(Self)
            .unwrap()
    }
}

impl doku::Document for Fakeable<Utf8PathBuf> {
    fn ty() -> doku::Type {
        std::path::PathBuf::ty()
    }
}
