use anyhow::Context;
use clap_complete::ArgValueCandidates;
use clap_complete::CompletionCandidate;
use std::borrow::Cow;
use std::fmt;
use std::path::Path;
use tedge_config::get_config_dir;
use tedge_config::tedge_toml::ProfileName;
use tedge_config::TEdgeConfig;
use tedge_system_services::SystemService;

#[derive(clap::Subcommand, Debug, Clone, PartialEq, Eq)]
#[clap(rename_all = "snake_case")]
pub enum CloudArg {
    #[cfg(feature = "c8y")]
    C8y {
        /// The cloud profile you wish to use
        ///
        /// [env: TEDGE_CLOUD_PROFILE]
        #[clap(long)]
        #[arg(add(ArgValueCandidates::new(profile_completions)))]
        profile: Option<ProfileName>,
    },
    #[cfg(feature = "azure")]
    Az {
        /// The cloud profile you wish to use
        ///
        /// [env: TEDGE_CLOUD_PROFILE]
        #[clap(long)]
        #[arg(add(ArgValueCandidates::new(profile_completions)))]
        profile: Option<ProfileName>,
    },
    #[cfg(feature = "aws")]
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
            #[cfg(feature = "aws")]
            Self::Aws {
                profile: Some(profile),
            } => Cloud::aws(Some(profile)),
            #[cfg(feature = "azure")]
            Self::Az {
                profile: Some(profile),
            } => Cloud::az(Some(profile)),
            #[cfg(feature = "c8y")]
            Self::C8y {
                profile: Some(profile),
            } => Cloud::c8y(Some(profile)),
            #[cfg(feature = "aws")]
            Self::Aws { profile: None } => Cloud::aws(read_env()?),
            #[cfg(feature = "azure")]
            Self::Az { profile: None } => Cloud::az(read_env()?),
            #[cfg(feature = "c8y")]
            Self::C8y { profile: None } => Cloud::c8y(read_env()?),
        })
    }
}

pub type Cloud = MaybeBorrowedCloud<'static>;

pub type CloudBorrow<'a> = MaybeBorrowedCloud<'a>;

#[derive(Clone, Debug, strum_macros::IntoStaticStr, PartialEq, Eq)]
pub enum MaybeBorrowedCloud<'a> {
    #[strum(serialize = "Cumulocity")]
    #[cfg(feature = "c8y")]
    C8y(Option<Cow<'a, ProfileName>>),
    #[cfg(feature = "azure")]
    Azure(Option<Cow<'a, ProfileName>>),
    #[cfg(feature = "aws")]
    Aws(Option<Cow<'a, ProfileName>>),
}

impl fmt::Display for MaybeBorrowedCloud<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}",
            match self {
                #[cfg(feature = "c8y")]
                Self::C8y(_) => "Cumulocity",
                #[cfg(feature = "azure")]
                Self::Azure(_) => "Azure",
                #[cfg(feature = "aws")]
                Self::Aws(_) => "Aws",
            }
        )
    }
}

impl<'a> From<&'a MaybeBorrowedCloud<'a>> for tedge_config::tedge_toml::Cloud<'a> {
    fn from(value: &'a MaybeBorrowedCloud<'a>) -> tedge_config::tedge_toml::Cloud<'a> {
        match value {
            #[cfg(feature = "c8y")]
            MaybeBorrowedCloud::C8y(p) => tedge_config::tedge_toml::Cloud::C8y(p.as_deref()),
            #[cfg(feature = "azure")]
            MaybeBorrowedCloud::Azure(p) => tedge_config::tedge_toml::Cloud::Az(p.as_deref()),
            #[cfg(feature = "aws")]
            MaybeBorrowedCloud::Aws(p) => tedge_config::tedge_toml::Cloud::Aws(p.as_deref()),
        }
    }
}

impl Cloud {
    #[cfg(feature = "c8y")]
    pub fn c8y(profile: Option<ProfileName>) -> Self {
        Self::C8y(profile.map(Cow::Owned))
    }

    #[cfg(feature = "azure")]
    pub fn az(profile: Option<ProfileName>) -> Self {
        Self::Azure(profile.map(Cow::Owned))
    }

    #[cfg(feature = "aws")]
    pub fn aws(profile: Option<ProfileName>) -> Self {
        Self::Aws(profile.map(Cow::Owned))
    }
}

impl<'a> CloudBorrow<'a> {
    #[cfg(feature = "c8y")]
    pub fn c8y_borrowed(profile: Option<&'a ProfileName>) -> Self {
        Self::C8y(profile.map(Cow::Borrowed))
    }
    #[cfg(feature = "azure")]
    pub fn az_borrowed(profile: Option<&'a ProfileName>) -> Self {
        Self::Azure(profile.map(Cow::Borrowed))
    }
    #[cfg(feature = "aws")]
    pub fn aws_borrowed(profile: Option<&'a ProfileName>) -> Self {
        Self::Aws(profile.map(Cow::Borrowed))
    }
}

impl MaybeBorrowedCloud<'_> {
    pub fn mapper_service(&self) -> SystemService<'_> {
        match self {
            #[cfg(feature = "aws")]
            Self::Aws(profile) => SystemService {
                name: "tedge-mapper-aws",
                profile: profile.as_deref(),
            },
            #[cfg(feature = "azure")]
            Self::Azure(profile) => SystemService {
                name: "tedge-mapper-az",
                profile: profile.as_deref(),
            },
            #[cfg(feature = "c8y")]
            Self::C8y(profile) => SystemService {
                name: "tedge-mapper-c8y",
                profile: profile.as_deref(),
            },
        }
    }

    pub fn mosquitto_config_filename(&self) -> Cow<'static, str> {
        match self {
            #[cfg(feature = "c8y")]
            Self::C8y(None) => "c8y-bridge.conf".into(),
            #[cfg(feature = "c8y")]
            Self::C8y(Some(profile)) => format!("c8y@{profile}-bridge.conf").into(),
            #[cfg(feature = "aws")]
            Self::Aws(None) => "aws-bridge.conf".into(),
            #[cfg(feature = "aws")]
            Self::Aws(Some(profile)) => format!("aws@{profile}-bridge.conf").into(),
            #[cfg(feature = "azure")]
            Self::Azure(None) => "az-bridge.conf".into(),
            #[cfg(feature = "azure")]
            Self::Azure(Some(profile)) => format!("az@{profile}-bridge.conf").into(),
        }
    }

    pub fn profile_name(&self) -> Option<&ProfileName> {
        match self {
            #[cfg(feature = "c8y")]
            Self::C8y(profile) => profile.as_deref(),
            #[cfg(feature = "aws")]
            Self::Aws(profile) => profile.as_deref(),
            #[cfg(feature = "azure")]
            Self::Azure(profile) => profile.as_deref(),
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
    tokio::task::block_in_place(|| {
        tokio::runtime::Handle::current()
            .block_on(profile_completions_for_config_dir(get_config_dir()))
    })
}

async fn profile_completions_for_config_dir(dir: impl AsRef<Path>) -> Vec<CompletionCandidate> {
    let Ok(tc) = TEdgeConfig::load(dir).await else {
        return vec![];
    };
    tc.c8y_keys_str()
        .flatten()
        .map(CompletionCandidate::new)
        .chain(tc.az_keys_str().flatten().map(CompletionCandidate::new))
        .chain(tc.aws_keys_str().flatten().map(CompletionCandidate::new))
        .collect()
}

#[cfg(test)]
mod tests {
    use tedge_test_utils::fs::TempTedgeDir;

    use crate::cli::common::profile_completions_for_config_dir;

    #[tokio::test]
    async fn profile_completions_include_tedge_toml_profile_names() {
        let ttd = TempTedgeDir::new();
        ttd.file("tedge.toml")
            .with_raw_content("[c8y.profiles.something]\n[c8y.profiles.other]");
        let completions = completion_names(&ttd).await;
        assert_eq!(completions, ["other", "something"]);
    }

    #[tokio::test]
    async fn profile_completions_include_separate_mapper_config_profile_names() {
        let ttd = TempTedgeDir::new();
        let mappers = ttd.dir("mappers");
        mappers
            .dir("c8y.profile1")
            .file("tedge.toml")
            .with_raw_content("");
        mappers
            .dir("c8y.profile2")
            .file("tedge.toml")
            .with_raw_content("");
        let completions = completion_names(&ttd).await;
        assert_eq!(completions, ["profile1", "profile2"]);
    }

    /// Generates profile completions for the provided config dir, extracts just
    /// the completion text portion, and sorts them to ensure the order is
    /// stable
    async fn completion_names(ttd: &TempTedgeDir) -> Vec<String> {
        let completions = profile_completions_for_config_dir(ttd.path()).await;
        let mut completions = completions
            .iter()
            .map(|candidate| candidate.get_value().to_str().unwrap().to_owned())
            .collect::<Vec<_>>();
        completions.sort();
        completions
    }
}
