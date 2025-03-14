use crate::system_services::SystemService;
use anyhow::Context;
use clap_complete::ArgValueCandidates;
use clap_complete::CompletionCandidate;
use std::borrow::Cow;
use std::fmt;
use tedge_config::get_config_dir;
use tedge_config::tedge_toml::ProfileName;
use tedge_config::TEdgeConfigLocation;

#[derive(clap::Subcommand, Debug, Clone, PartialEq, Eq)]
#[clap(rename_all = "snake_case")]
pub enum CloudArg {
    C8y {
        /// The cloud profile you wish to use
        ///
        /// [env: TEDGE_CLOUD_PROFILE]
        #[clap(long)]
        #[arg(add(ArgValueCandidates::new(profile_completions)))]
        profile: Option<ProfileName>,
    },
    Az {
        /// The cloud profile you wish to use
        ///
        /// [env: TEDGE_CLOUD_PROFILE]
        #[clap(long)]
        #[arg(add(ArgValueCandidates::new(profile_completions)))]
        profile: Option<ProfileName>,
    },
    Aws {
        /// The cloud profile you wish to use
        ///
        /// [env: TEDGE_CLOUD_PROFILE]
        #[clap(long)]
        #[arg(add(ArgValueCandidates::new(profile_completions)))]
        profile: Option<ProfileName>,
    },
}

impl TryFrom<CloudArg> for Cloud {
    type Error = anyhow::Error;

    fn try_from(args: CloudArg) -> Result<Self, Self::Error> {
        args.try_with_profile_and_env()
    }
}

impl CloudArg {
    fn try_with_profile_and_env(self) -> anyhow::Result<Cloud> {
        let read_env = || {
            let env = "TEDGE_CLOUD_PROFILE";
            match std::env::var(env).as_deref() {
                Ok("") => Ok(None),
                Ok(var) => var
                    .parse()
                    .with_context(|| {
                        format!("Parsing profile from environment variable {env}={var:?}")
                    })
                    .map(Some),
                _ => Ok(None),
            }
        };
        Ok(match self {
            Self::Aws {
                profile: Some(profile),
            } => Cloud::aws(Some(profile)),
            Self::Az {
                profile: Some(profile),
            } => Cloud::az(Some(profile)),
            Self::C8y {
                profile: Some(profile),
            } => Cloud::c8y(Some(profile)),
            Self::Aws { profile: None } => Cloud::aws(read_env()?),
            Self::Az { profile: None } => Cloud::az(read_env()?),
            Self::C8y { profile: None } => Cloud::c8y(read_env()?),
        })
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

impl<'a> From<&'a MaybeBorrowedCloud<'a>> for tedge_config::tedge_toml::Cloud<'a> {
    fn from(value: &'a MaybeBorrowedCloud<'a>) -> tedge_config::tedge_toml::Cloud<'a> {
        match value {
            MaybeBorrowedCloud::C8y(p) => tedge_config::tedge_toml::Cloud::C8y(p.as_deref()),
            MaybeBorrowedCloud::Azure(p) => tedge_config::tedge_toml::Cloud::Az(p.as_deref()),
            MaybeBorrowedCloud::Aws(p) => tedge_config::tedge_toml::Cloud::Aws(p.as_deref()),
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

    pub fn profile_name(&self) -> Option<&ProfileName> {
        match self {
            Self::Aws(profile) | Self::Azure(profile) | Self::C8y(profile) => profile.as_deref(),
        }
    }
}

/// (Best-effort) tab-completion values for profile names
///
/// This will infer the profile names from the various cloud configurations.
/// It would be significantly more complicated to try and do per-cloud
/// completions, and would likely provide no real value to anyone.
///
/// It will use the configuration directory as set by the
/// `TEDGE_CONFIGURATION_DIR` environment variable, or `/etc/tedge` if
/// that is not set
pub fn profile_completions() -> Vec<CompletionCandidate> {
    let location = TEdgeConfigLocation::from_custom_root(get_config_dir());
    let Ok(tc) = location.load_sync() else {
        return vec![];
    };
    tc.c8y
        .keys_str()
        .flatten()
        .map(CompletionCandidate::new)
        .chain(tc.az.keys_str().flatten().map(CompletionCandidate::new))
        .chain(tc.aws.keys_str().flatten().map(CompletionCandidate::new))
        .collect()
}
