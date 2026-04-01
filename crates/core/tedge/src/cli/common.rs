use anyhow::Context;
use clap_complete::ArgValueCandidates;
use clap_complete::CompletionCandidate;
use std::borrow::Cow;
use std::fmt;
use std::path::Path;
use tedge_config::get_config_dir;
use tedge_config::tedge_toml::ProfileName;
use tedge_config::tedge_toml::ReadableKey;
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

/// A free-form cloud/mapper name for `tedge connect`, which accepts both
/// built-in cloud names (c8y, az, aws) and custom mapper names.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConnectCloudArg {
    pub name: String,
    pub profile: Option<ProfileName>,
}

impl ConnectCloudArg {
    pub fn into_cloud(self) -> Cloud {
        match resolve_cloud(&self.name, self.profile) {
            Some(cloud) => cloud,
            None => Cloud::Custom(self.name),
        }
    }
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
        match self {
            #[cfg(feature = "aws")]
            Self::Aws {
                profile: Some(profile),
            } => Ok(Cloud::aws(Some(profile))),
            #[cfg(feature = "azure")]
            Self::Az {
                profile: Some(profile),
            } => Ok(Cloud::az(Some(profile))),
            #[cfg(feature = "c8y")]
            Self::C8y {
                profile: Some(profile),
            } => Ok(Cloud::c8y(Some(profile))),
            #[cfg(feature = "aws")]
            Self::Aws { profile: None } => Ok(Cloud::aws(read_env()?)),
            #[cfg(feature = "azure")]
            Self::Az { profile: None } => Ok(Cloud::az(read_env()?)),
            #[cfg(feature = "c8y")]
            Self::C8y { profile: None } => Ok(Cloud::c8y(read_env()?)),
        }
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
    #[strum(serialize = "custom")]
    Custom(String),
}

impl fmt::Display for MaybeBorrowedCloud<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            #[cfg(feature = "c8y")]
            Self::C8y(_) => write!(f, "Cumulocity"),
            #[cfg(feature = "azure")]
            Self::Azure(_) => write!(f, "Azure"),
            #[cfg(feature = "aws")]
            Self::Aws(_) => write!(f, "Aws"),
            Self::Custom(name) => write!(f, "{name}"),
        }
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
            MaybeBorrowedCloud::Custom(_) => {
                unreachable!("Custom mappers do not have a tedge_toml::Cloud representation")
            }
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

/// Resolve a cloud name string to a known `Cloud`, or `None` for custom mappers.
pub fn resolve_cloud(name: &str, profile: Option<ProfileName>) -> Option<Cloud> {
    // If no explicit profile is given and the name looks like `<cloud>.<profile>`,
    // split and treat the second segment as the profile name.
    if profile.is_none() {
        if let Some((cloud, profile_str)) = name.split_once('.') {
            if let Ok(parsed_profile) = profile_str.parse::<ProfileName>() {
                return resolve_cloud(cloud, Some(parsed_profile));
            }
        }
    }

    match name {
        #[cfg(feature = "c8y")]
        "c8y" => Some(Cloud::c8y(profile)),
        #[cfg(feature = "aws")]
        "aws" => Some(Cloud::aws(profile)),
        #[cfg(feature = "azure")]
        "az" => Some(Cloud::az(profile)),
        _ => None,
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
            Self::Custom(_) => {
                unreachable!("mapper_service is not used for custom mappers")
            }
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
            Self::Custom(name) => format!("{name}-bridge.conf").into(),
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
            Self::Custom(_) => None,
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
/// Completion candidates for mapper name arguments.
///
/// Scans the `mappers/` directory under `TEDGE_CONFIG_DIR` (or the compiled-in
/// default) and returns subdirectory names.
pub fn mapper_name_completions() -> Vec<CompletionCandidate> {
    tokio::task::block_in_place(|| {
        tokio::runtime::Handle::current()
            .block_on(mapper_name_completions_for_config_dir(get_config_dir()))
    })
}

async fn mapper_name_completions_for_config_dir(dir: impl AsRef<Path>) -> Vec<CompletionCandidate> {
    let mappers_root = dir.as_ref().join("mappers");
    let Ok(mut entries) = tokio::fs::read_dir(&mappers_root).await else {
        return vec![];
    };
    let mut names = Vec::new();
    while let Ok(Some(entry)) = entries.next_entry().await {
        let Ok(ft) = entry.file_type().await else {
            continue;
        };
        if ft.is_dir() {
            names.push(CompletionCandidate::new(
                entry.file_name().to_string_lossy().into_owned(),
            ));
        }
    }
    names.sort_by(|a, b| a.get_value().cmp(b.get_value()));
    names
}

/// Completion candidates for `tedge mapper config get`.
///
/// The argument format is `<mapper-name>.<key>`, so completions are:
/// - For built-in clouds (`c8y`, `az`, `aws` + profiles): the `tedge.toml`
///   config keys from `ReadableKey` matching that cloud prefix.
/// - For custom mappers (on-disk only): the cross-product of the mapper name
///   and the custom mapper schema keys.
pub fn mapper_config_key_completions() -> Vec<CompletionCandidate> {
    tokio::task::block_in_place(|| {
        tokio::runtime::Handle::current().block_on(mapper_config_key_completions_for_config_dir(
            get_config_dir(),
        ))
    })
}

/// Returns `true` if `name` is a known built-in cloud name.
pub fn is_builtin_cloud(name: &str) -> bool {
    #[cfg(feature = "c8y")]
    if name == "c8y" {
        return true;
    }
    #[cfg(feature = "azure")]
    if name == "az" {
        return true;
    }
    #[cfg(feature = "aws")]
    if name == "aws" {
        return true;
    }
    let _ = name;
    false
}

async fn mapper_config_key_completions_for_config_dir(
    dir: impl AsRef<Path>,
) -> Vec<CompletionCandidate> {
    let mut candidates = Vec::new();

    let readable_completions: Vec<_> = ReadableKey::completions()
        .into_iter()
        .map(|c| {
            let key = c.get_value().to_str().unwrap_or_default().to_string();
            (key, c)
        })
        .collect();

    // Non-profiled built-in clouds: offer the real tedge.toml config keys
    // directly (e.g. `c8y.url`, `c8y.smartrest.templates`), excluding
    // profile-qualified variants (`c8y.profiles.*.url`).
    for (key, _) in &readable_completions {
        let first = key.split('.').next().unwrap_or_default();
        if !is_builtin_cloud(first) {
            continue;
        }
        if !key.contains(".profiles.") {
            candidates.push(CompletionCandidate::new(key));
        }
    }

    let disk_names = mapper_name_completions_for_config_dir(&dir).await;
    let schema_keys = tedge_mapper::custom_mapper_resolve::custom_mapper_schema_keys();

    for name in &disk_names {
        let name_str = name.get_value().to_str().unwrap_or_default();
        let first_segment = name_str.split('.').next().unwrap_or(name_str);

        if is_builtin_cloud(first_segment) {
            if name_str == first_segment {
                // Non-profiled built-in (e.g. "c8y") — already covered above.
                continue;
            }
            // Profiled built-in cloud (e.g. "c8y.new"): rewrite non-profiled
            // ReadableKey keys for this cloud by inserting the profile name.
            // `c8y.url` → `c8y.new.url`, `c8y.device.cert_path` → `c8y.new.device.cert_path`
            let profile = &name_str[first_segment.len() + 1..]; // e.g. "new"
            for (key, _c) in &readable_completions {
                if key.split('.').next() == Some(first_segment) && !key.contains(".profiles.") {
                    // key = "c8y.url" → suffix = "url"
                    let suffix = &key[first_segment.len() + 1..];
                    candidates.push(CompletionCandidate::new(format!(
                        "{first_segment}.{profile}.{suffix}"
                    )));
                }
            }
        } else {
            // Custom mapper: cross-product of name × schema keys.
            for key in &schema_keys {
                candidates.push(CompletionCandidate::new(format!("{name_str}.{key}")));
            }
        }
    }

    candidates
}

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

    use crate::cli::common::mapper_config_key_completions_for_config_dir;
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
            .file("mapper.toml")
            .with_raw_content("");
        mappers
            .dir("c8y.profile2")
            .file("mapper.toml")
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

    #[tokio::test]
    async fn mapper_config_key_completions_include_profiled_builtin_clouds() {
        let ttd = TempTedgeDir::new();
        let mappers = ttd.dir("mappers");
        mappers
            .dir("c8y.staging")
            .file("mapper.toml")
            .with_raw_content("");
        let completions = mapper_config_key_completions_for_config_dir(ttd.path()).await;
        let keys: Vec<String> = completions
            .iter()
            .map(|c| c.get_value().to_str().unwrap().to_owned())
            .collect();
        // Profiled built-in cloud should get the c8y tedge.toml keys rewritten
        // with the profile name (not the limited custom mapper schema keys).
        assert!(
            keys.contains(&"c8y.staging.url".to_string()),
            "expected c8y.staging.url in completions, got: {keys:?}"
        );
        // c8y-specific keys should appear (not just generic schema keys)
        assert!(
            keys.contains(&"c8y.staging.smartrest.templates".to_string()),
            "expected c8y.staging.smartrest.templates in completions, got: {keys:?}"
        );
        // Non-profiled c8y keys should still be present
        assert!(
            keys.contains(&"c8y.url".to_string()),
            "expected c8y.url in completions, got: {keys:?}"
        );
        // ReadableKey profile format should NOT appear
        assert!(
            !keys.iter().any(|k| k.contains(".profiles.")),
            "should not contain .profiles. keys, got: {keys:?}"
        );
    }

    #[tokio::test]
    #[cfg(feature = "azure")]
    async fn mapper_config_key_completions_include_profiled_az_cloud() {
        let ttd = TempTedgeDir::new();
        let mappers = ttd.dir("mappers");
        mappers
            .dir("az.eu")
            .file("mapper.toml")
            .with_raw_content("");
        let completions = mapper_config_key_completions_for_config_dir(ttd.path()).await;
        let keys: Vec<String> = completions
            .iter()
            .map(|c| c.get_value().to_str().unwrap().to_owned())
            .collect();
        assert!(
            keys.contains(&"az.eu.url".to_string()),
            "expected az.eu.url in completions, got: {keys:?}"
        );
        assert!(
            !keys.iter().any(|k| k.contains(".profiles.")),
            "should not contain .profiles. keys, got: {keys:?}"
        );
    }

    #[tokio::test]
    #[cfg(feature = "aws")]
    async fn mapper_config_key_completions_include_profiled_aws_cloud() {
        let ttd = TempTedgeDir::new();
        let mappers = ttd.dir("mappers");
        mappers
            .dir("aws.us-east")
            .file("mapper.toml")
            .with_raw_content("");
        let completions = mapper_config_key_completions_for_config_dir(ttd.path()).await;
        let keys: Vec<String> = completions
            .iter()
            .map(|c| c.get_value().to_str().unwrap().to_owned())
            .collect();
        assert!(
            keys.contains(&"aws.us-east.url".to_string()),
            "expected aws.us-east.url in completions, got: {keys:?}"
        );
        assert!(
            !keys.iter().any(|k| k.contains(".profiles.")),
            "should not contain .profiles. keys, got: {keys:?}"
        );
    }

    mod connect_cloud_arg {
        use super::super::Cloud;
        use super::super::ConnectCloudArg;

        #[test]
        fn c8y_resolves_to_builtin_cloud() {
            assert!(matches!(arg("c8y").into_cloud(), Cloud::C8y(None)));
        }

        #[test]
        fn az_resolves_to_builtin_cloud() {
            assert!(matches!(arg("az").into_cloud(), Cloud::Azure(None)));
        }

        #[test]
        fn aws_resolves_to_builtin_cloud() {
            assert!(matches!(arg("aws").into_cloud(), Cloud::Aws(None)));
        }

        #[test]
        fn unknown_name_resolves_to_custom() {
            let cloud = arg("thingsboard").into_cloud();
            assert!(matches!(cloud, Cloud::Custom(ref n) if n == "thingsboard"));
        }

        #[test]
        fn c8y_with_profile_resolves_to_builtin_with_profile() {
            assert!(matches!(
                arg_with_profile("c8y", "prod").into_cloud(),
                Cloud::C8y(Some(_))
            ));
        }

        #[test]
        fn dotted_c8y_name_resolves_as_profile() {
            // "c8y.prod" without an explicit profile flag should be treated as c8y + profile "prod"
            assert!(matches!(arg("c8y.prod").into_cloud(), Cloud::C8y(Some(_))));
        }

        #[test]
        fn dotted_custom_name_stays_custom() {
            // A dotted name whose first segment is not a known cloud is still custom
            let cloud = arg("my-mapper.prod").into_cloud();
            assert!(
                matches!(cloud, Cloud::Custom(ref n) if n == "my-mapper.prod"),
                "expected Custom(my-mapper.prod), got {cloud:?}"
            );
        }

        fn arg(name: &str) -> ConnectCloudArg {
            ConnectCloudArg {
                name: name.into(),
                profile: None,
            }
        }

        fn arg_with_profile(name: &str, profile: &str) -> ConnectCloudArg {
            ConnectCloudArg {
                name: name.into(),
                profile: Some(profile.parse().unwrap()),
            }
        }
    }
}
