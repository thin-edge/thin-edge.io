use camino::Utf8PathBuf;
use std::fmt;
use std::str::FromStr;

/// A path made absolute against the current directory and cleaned of `.` and
/// `..` components
#[derive(Clone, Debug, PartialEq, Eq, facet::Facet)]
#[facet(proxy = String)]
pub struct AbsolutePath(Utf8PathBuf);

impl AbsolutePath {
    pub fn as_path(&self) -> &camino::Utf8Path {
        &self.0
    }
}

impl FromStr for AbsolutePath {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let path = Utf8PathBuf::from(s);
        let absolute = if path.is_absolute() {
            path
        } else {
            let Ok(Ok(pwd)) = std::env::current_dir().map(Utf8PathBuf::try_from) else {
                return Err(format!("Cannot be converted to an absolute path: {path}"));
            };
            pwd.join(path)
        };
        let clean = Utf8PathBuf::try_from(path_clean::clean(absolute))
            .expect("cleaning a UTF-8 path gives a UTF-8 path");
        Ok(Self(clean))
    }
}

impl fmt::Display for AbsolutePath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl TryFrom<String> for AbsolutePath {
    type Error = String;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        value.parse()
    }
}

impl From<&AbsolutePath> for String {
    fn from(value: &AbsolutePath) -> Self {
        value.0.to_string()
    }
}

tedge_config_engine::impl_single_value_append_remove!(AbsolutePath);
