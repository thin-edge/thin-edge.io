use camino::Utf8Path;
use camino::Utf8PathBuf;
use doku::Document;
use doku::Type;
use serde::Deserialize;
use serde::Serialize;
use std::ffi::OsStr;
use std::fmt::Display;
use std::fmt::Formatter;
use std::ops::Deref;
use std::path::Path;
use std::path::PathBuf;
use std::str::FromStr;

#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub struct AbsolutePath(Utf8PathBuf);

impl AbsolutePath {
    pub fn try_new(value: &str) -> Result<Self, InvalidAbsolutePath> {
        Utf8PathBuf::from(value).try_into()
    }

    pub fn from_path(path: Utf8PathBuf) -> Result<Self, InvalidAbsolutePath> {
        let absolute_path = if path.is_absolute() {
            path
        } else {
            let Ok(Ok(pwd)) = std::env::current_dir().map(Utf8PathBuf::try_from) else {
                return Err(InvalidAbsolutePath(path));
            };
            pwd.join(path)
        };

        // unwrap is safe because clean returns an utf8 path when given an utf8 path
        let clean_path = Utf8PathBuf::try_from(path_clean::clean(absolute_path)).unwrap();
        Ok(AbsolutePath(clean_path))
    }
}

#[derive(thiserror::Error, Debug)]
#[error("Cannot be converted to an absolute path: {0}")]
pub struct InvalidAbsolutePath(Utf8PathBuf);

impl Document for AbsolutePath {
    fn ty() -> Type {
        PathBuf::ty()
    }
}

impl Deref for AbsolutePath {
    type Target = Utf8PathBuf;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl FromStr for AbsolutePath {
    type Err = InvalidAbsolutePath;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        AbsolutePath::try_new(s)
    }
}

impl TryFrom<Utf8PathBuf> for AbsolutePath {
    type Error = InvalidAbsolutePath;
    fn try_from(value: Utf8PathBuf) -> Result<Self, Self::Error> {
        AbsolutePath::from_path(value)
    }
}

impl From<AbsolutePath> for Utf8PathBuf {
    fn from(value: AbsolutePath) -> Self {
        value.0
    }
}

impl AsRef<Utf8Path> for AbsolutePath {
    fn as_ref(&self) -> &Utf8Path {
        self.0.as_path()
    }
}

impl AsRef<Path> for AbsolutePath {
    fn as_ref(&self) -> &Path {
        self.0.as_std_path()
    }
}

impl Display for AbsolutePath {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

impl AsRef<OsStr> for AbsolutePath {
    fn as_ref(&self) -> &OsStr {
        self.0.as_os_str()
    }
}
