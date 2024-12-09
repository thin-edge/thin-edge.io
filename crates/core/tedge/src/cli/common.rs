use anyhow::Context;
use std::borrow::Cow;
use std::fmt;
use tedge_config::system_services::SystemService;
use tedge_config::ProfileName;

#[derive(clap::Args, PartialEq, Eq, Debug, Clone)]
pub struct CloudArgs {
    /// The cloud you wish to interact with
    cloud: CloudType,

    /// The cloud profile you wish to use, if not specified as part of the cloud
    #[clap(long)]
    profile: Option<ProfileName>,
}

#[derive(clap::Args, PartialEq, Eq, Debug, Clone)]
pub struct OptionalCloudArgs {
    /// The cloud you wish to interact with
    cloud: Option<CloudType>,

    /// The cloud profile you wish to use, if not specified as part of the cloud
    #[clap(long)]
    profile: Option<ProfileName>,
}

#[derive(clap::ValueEnum, Debug, Copy, Clone, PartialEq, Eq)]
#[clap(rename_all = "snake_case")]
enum CloudType {
    C8y,
    Az,
    Aws,
}

impl TryFrom<CloudArgs> for Cloud {
    type Error = anyhow::Error;

    fn try_from(args: CloudArgs) -> Result<Self, Self::Error> {
        args.cloud.try_with_profile_and_env(args.profile)
    }
}

impl TryFrom<OptionalCloudArgs> for Option<Cloud> {
    type Error = anyhow::Error;

    fn try_from(args: OptionalCloudArgs) -> Result<Self, Self::Error> {
        args.cloud
            .map(|cloud| cloud.try_with_profile_and_env(args.profile))
            .transpose()
    }
}

impl CloudType {
    pub fn try_with_profile_and_env(self, profile: Option<ProfileName>) -> anyhow::Result<Cloud> {
        let env = "TEDGE_CLOUD_PROFILE";

        match profile {
            Some(profile) => Ok(self.with_profile(Some(profile))),
            None => match std::env::var(env).as_deref() {
                Ok("") => Ok(self.with_profile(None)),
                Ok(e) => Ok(self.with_profile(Some(e.parse().with_context(|| {
                    format!("Parsing profile from environment variable {env}={e:?}")
                })?))),
                _ => Ok(self.with_profile(None)),
            },
        }
    }

    fn with_profile(self, profile: Option<ProfileName>) -> Cloud {
        let profile = profile.map(Cow::Owned);
        match self {
            Self::Aws => Cloud::Aws(profile),
            Self::Az => Cloud::Azure(profile),
            Self::C8y => Cloud::C8y(profile),
        }
    }
}

pub type Cloud = MaybeBorrowedCloud<'static>;

pub type CloudBorrow<'a> = MaybeBorrowedCloud<'a>;

#[derive(Clone, Debug, strum_macros::IntoStaticStr, PartialEq, Eq)]
pub enum MaybeBorrowedCloud<'a> {
    #[strum(serialize = "Cumulocity")]
    C8y(Option<Cow<'a, ProfileName>>),
    Azure(Option<Cow<'a, ProfileName>>),
    Aws(Option<Cow<'a, ProfileName>>),
}

impl fmt::Display for MaybeBorrowedCloud<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}",
            match self {
                Self::C8y(_) => "Cumulocity",
                Self::Azure(_) => "Azure",
                Self::Aws(_) => "Aws",
            }
        )
    }
}

impl<'a> From<&'a MaybeBorrowedCloud<'a>> for tedge_config::Cloud<'a> {
    fn from(value: &'a MaybeBorrowedCloud<'a>) -> tedge_config::Cloud<'a> {
        match value {
            MaybeBorrowedCloud::C8y(p) => tedge_config::Cloud::C8y(p.as_deref()),
            MaybeBorrowedCloud::Azure(p) => tedge_config::Cloud::Az(p.as_deref()),
            MaybeBorrowedCloud::Aws(p) => tedge_config::Cloud::Aws(p.as_deref()),
        }
    }
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
