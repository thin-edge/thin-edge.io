use std::borrow::Cow;
use std::str::FromStr;
use tedge_config::system_services::SystemService;
use tedge_config::ProfileName;

pub type Cloud = MaybeBorrowedCloud<'static>;

impl FromStr for Cloud {
    type Err = <ProfileName as FromStr>::Err;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match (s, s.split_once("@")) {
            (_, Some(("c8y", profile))) => Ok(Self::c8y(Some(profile.parse()?))),
            ("c8y", None) => Ok(Self::c8y(None)),
            (_, Some(("az", profile))) => Ok(Self::az(Some(profile.parse()?))),
            ("az", None) => Ok(Self::Azure(None)),
            (_, Some(("aws", profile))) => Ok(Self::aws(Some(profile.parse()?))),
            ("aws", None) => Ok(Self::Aws(None)),
            _ => todo!(),
        }
    }
}

pub type CloudBorrow<'a> = MaybeBorrowedCloud<'a>;

#[derive(Clone, Debug, strum_macros::Display, strum_macros::IntoStaticStr, PartialEq, Eq)]
pub enum MaybeBorrowedCloud<'a> {
    #[strum(serialize = "Cumulocity")]
    C8y(Option<Cow<'a, ProfileName>>),
    Azure(Option<Cow<'a, ProfileName>>),
    Aws(Option<Cow<'a, ProfileName>>),
}

impl Cloud {
    pub fn c8y(profile: Option<ProfileName>) -> Self {
        Self::C8y(profile.map(Cow::Owned))
    }
    pub fn az(profile: Option<ProfileName>) -> Self {
        Self::Azure(profile.map(Cow::Owned))
    }
    pub fn aws(profile: Option<ProfileName>) -> Self {
        Self::Aws(profile.map(Cow::Owned))
    }
}

impl<'a> CloudBorrow<'a> {
    pub fn c8y_borrowed(profile: Option<&'a ProfileName>) -> Self {
        Self::C8y(profile.map(Cow::Borrowed))
    }
    pub fn az_borrowed(profile: Option<&'a ProfileName>) -> Self {
        Self::Azure(profile.map(Cow::Borrowed))
    }
    pub fn aws_borrowed(profile: Option<&'a ProfileName>) -> Self {
        Self::Aws(profile.map(Cow::Borrowed))
    }
}

impl MaybeBorrowedCloud<'_> {
    pub fn mapper_service(&self) -> SystemService<'_> {
        match self {
            Self::Aws(profile) => SystemService::TEdgeMapperAws(profile.as_deref()),
            Self::Azure(profile) => SystemService::TEdgeMapperAz(profile.as_deref()),
            Self::C8y(profile) => SystemService::TEdgeMapperC8y(profile.as_deref()),
        }
    }

    pub fn bridge_config_filename(&self) -> Cow<'static, str> {
        match self {
            Self::C8y(None) => "c8y-bridge.conf".into(),
            Self::C8y(Some(profile)) => format!("c8y@{profile}-bridge.conf").into(),
            Self::Aws(None) => "aws-bridge.conf".into(),
            Self::Aws(Some(profile)) => format!("aws@{profile}-bridge.conf").into(),
            Self::Azure(None) => "az-bridge.conf".into(),
            Self::Azure(Some(profile)) => format!("az@{profile}-bridge.conf").into(),
        }
    }
}
