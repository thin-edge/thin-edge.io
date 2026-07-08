#[cfg(feature = "aws")]
use crate::aws::mapper::AwsMapper;
#[cfg(feature = "azure")]
use crate::az::mapper::AzureMapper;
#[cfg(feature = "c8y")]
use crate::c8y::mapper::CumulocityMapper;
use crate::collectd::mapper::CollectdMapper;
use crate::core::component::TEdgeComponent;
use crate::custom::mapper::CustomMapper;
use anyhow::bail;
use anyhow::Context;
use camino::Utf8Path;
use clap::Parser;
use flockfile::check_another_instance_is_not_running;
use flockfile::Flockfile;
use flockfile::FlockfileError;
use futures::FutureExt;
use std::collections::HashMap;
use std::fmt;
use std::str::FromStr;
use tedge_actors::Runtime;
use tedge_api::mqtt_topics::EntityTopicId;
use tedge_config::cli::CommonArgs;
use tedge_config::log_init_reloadable_for_services;
use tedge_config::tedge_toml::ProfileName;
use tedge_config::TEdgeConfig;
use tedge_flows::BaseFlowRegistry;
use tedge_flows::ConnectedFlowRegistry;
use tedge_flows::FlowRegistryExt;
use tedge_flows::FlowsMapperConfig;
use tedge_flows::UpdateFlowRegistryError;
use tedge_supervisor::Supervisor;
use tedge_supervisor::UnitKind;
use tedge_utils::paths::ManagedDir;
use tedge_utils::paths::TedgePaths;
use tracing::error;
use tracing::log::warn;

/// Validates that a mapper name matches `[a-z][a-z0-9-]*` and does not start with `bridge-`.
///
/// Underscores are forbidden because they would create ambiguity in the
/// `MAPPER_{NAME}_{KEY}` environment variable scheme.
///
/// Names starting with `bridge-` are forbidden because they would produce a service name of
/// `tedge-mapper-bridge-{rest}`, which collides with the bridge sub-service naming pattern.
fn validate_mapper_name(name: &str) -> anyhow::Result<()> {
    anyhow::ensure!(!name.is_empty(), "Mapper name cannot be empty");
    let mut chars = name.chars();
    let first = chars.next().unwrap();
    anyhow::ensure!(
        first.is_ascii_lowercase(),
        "Invalid mapper name '{name}': must start with a lowercase ASCII letter"
    );
    for ch in chars {
        anyhow::ensure!(
            matches!(ch, 'a'..='z' | '0'..='9' | '-'),
            "Invalid mapper name '{name}': names must match [a-z][a-z0-9-]* \
             (underscores are not allowed)"
        );
    }
    anyhow::ensure!(
        !name.starts_with("bridge-"),
        "Invalid mapper name '{name}': names starting with 'bridge-' are reserved \
         (would collide with the bridge sub-service name 'tedge-mapper-bridge-{name}')"
    );
    Ok(())
}

#[cfg(feature = "aws")]
pub mod aws;
#[cfg(feature = "azure")]
pub mod az;
#[cfg(feature = "c8y")]
pub mod c8y;
mod collectd;
mod core;
mod custom;
use crate::custom_mapper_resolve::EffectiveMapperConfig;
/// Re-export mapper directory warnings for use by CLI commands.
pub use core::mappers_dir::warn_misconfigured_mapper_dirs;
/// Re-export custom mapper config for use by bridge inspection commands.
pub use custom::config as custom_mapper_config;
/// Re-export custom mapper config resolution for use by CLI commands.
pub use custom::resolve as custom_mapper_resolve;

/// Read the cloud profile from the CLI argument or env variable.
macro_rules! read_profile {
    ($profile:ident, $var:literal) => {
        $profile.or_else(|| {
            let profile = std::env::var($var).ok()?;
            if profile.is_empty() {
                return None;
            }
            Some(
                profile
                    .parse()
                    .context(concat!("Reading environment variable ", $var))
                    .unwrap(),
            )
        })
    };
}

fn lookup_component(component_name: MapperName) -> anyhow::Result<Box<dyn TEdgeComponent>> {
    Ok(match component_name {
        #[cfg(feature = "azure")]
        MapperName::Az { profile } => Box::new(AzureMapper {
            profile: read_profile!(profile, "TEDGE_CLOUD_PROFILE"),
        }),
        #[cfg(feature = "aws")]
        MapperName::Aws { profile } => Box::new(AwsMapper {
            profile: read_profile!(profile, "TEDGE_CLOUD_PROFILE"),
        }),
        MapperName::Collectd => Box::new(CollectdMapper),
        #[cfg(feature = "c8y")]
        MapperName::C8y { profile } => Box::new(CumulocityMapper {
            profile: read_profile!(profile, "TEDGE_CLOUD_PROFILE"),
        }),
        MapperName::UserDefined(mut args) => {
            let name = args.remove(0);
            validate_mapper_name(&name)?;
            anyhow::ensure!(
                args.is_empty(),
                "User-defined mapper '{name}' does not accept additional arguments, \
                 got: {args:?}. Global flags (e.g. --config-dir) must appear before the mapper name."
            );
            Box::new(CustomMapper { name })
        }
    })
}

#[derive(Debug, Parser)]
#[clap(
    name = clap::crate_name!(),
    version = clap::crate_version!(),
    about = clap::crate_description!()
)]
pub struct MapperOpt {
    #[clap(subcommand)]
    name: MapperName,

    /// Start the mapper with clean session off, subscribe to the topics, so that no messages are lost
    #[clap(short, long)]
    init: bool,

    /// Start the agent with clean session on, drop the previous session and subscriptions
    ///
    /// WARNING: All pending messages will be lost.
    #[clap(short, long)]
    clear: bool,

    #[command(flatten)]
    pub common: CommonArgs,
}

#[derive(Debug, clap::Subcommand, Clone)]
#[clap(rename_all = "kebab-case")]
pub enum MapperName {
    #[cfg(feature = "azure")]
    Az {
        /// The cloud profile to use
        #[clap(long)]
        profile: Option<ProfileName>,
    },
    #[cfg(feature = "aws")]
    Aws {
        /// The cloud profile to use
        #[clap(long)]
        profile: Option<ProfileName>,
    },
    #[cfg(feature = "c8y")]
    C8y {
        /// The cloud profile to use
        #[clap(long)]
        profile: Option<ProfileName>,
    },
    Collectd,
    /// Run a user-defined mapper from `/etc/tedge/mappers/{name}/`.
    ///
    /// The mapper name must match `[a-z][a-z0-9-]*`.
    #[clap(external_subcommand)]
    UserDefined(Vec<String>),
}

impl fmt::Display for MapperName {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            #[cfg(feature = "azure")]
            MapperName::Az { profile: None } => write!(f, "tedge-mapper-az"),
            #[cfg(feature = "azure")]
            MapperName::Az {
                profile: Some(profile),
            } => write!(f, "tedge-mapper-az@{profile}"),
            #[cfg(feature = "aws")]
            MapperName::Aws { profile: None } => write!(f, "tedge-mapper-aws"),
            #[cfg(feature = "aws")]
            MapperName::Aws {
                profile: Some(profile),
            } => write!(f, "tedge-mapper-aws@{profile}"),
            #[cfg(feature = "c8y")]
            MapperName::C8y { profile: None } => write!(f, "tedge-mapper-c8y"),
            #[cfg(feature = "c8y")]
            MapperName::C8y {
                profile: Some(profile),
            } => write!(f, "tedge-mapper-c8y@{profile}"),
            MapperName::Collectd => write!(f, "tedge-mapper-collectd"),
            MapperName::UserDefined(args) => write!(
                f,
                "tedge-mapper-{}",
                args.first().map(String::as_str).unwrap_or("<unknown>")
            ),
        }
    }
}

impl MapperName {
    pub fn log_service_name(&self) -> &str {
        match self {
            #[cfg(feature = "azure")]
            MapperName::Az { .. } => "tedge-mapper-az",
            #[cfg(feature = "aws")]
            MapperName::Aws { .. } => "tedge-mapper-aws",
            #[cfg(feature = "c8y")]
            MapperName::C8y { .. } => "tedge-mapper-c8y",
            MapperName::Collectd => "tedge-mapper-collectd",
            MapperName::UserDefined(_) => "tedge-mapper",
        }
    }
}

pub async fn run(mapper_opt: MapperOpt, config: TEdgeConfig) -> anyhow::Result<()> {
    let mapper_name = mapper_opt.name.to_string();

    // Only the concrete component name: the generic `tedge-mapper` level and the
    // `tedge` fallback are resolved inside the filter, and the single-name shape is
    // what lets the configured level apply process-wide — the standalone supervisor
    // runs its unit without a `component` span to attribute events to.
    let log_reload = log_init_reloadable_for_services(
        &[mapper_opt.name.log_service_name()],
        &mapper_opt.common.log_args,
        &mapper_opt.common.config_dir,
    )?;

    let lock = acquire_lock(&mapper_name, &config)?;

    if mapper_opt.init {
        warn!("This --init option has been deprecated and will be removed in a future release");
        Ok(())
    } else if mapper_opt.clear {
        warn!("This --clear option has been deprecated and will be removed in a future release");
        Ok(())
    } else {
        let config_dir = mapper_opt.common.config_dir.clone();
        let mapper = mapper_opt.name;
        let factory: tedge_supervisor::RuntimeFactory = Box::new(move || {
            let config_dir = config_dir.clone();
            let mapper = mapper.clone();
            async move {
                let config = TEdgeConfig::load(&config_dir).await?;
                build(mapper, config).await
            }
            .boxed()
        });

        Supervisor::run_standalone(mapper_name, UnitKind::Mapper, factory, lock, log_reload).await
    }
}

/// Rebuildable factory the single-process supervisor calls (on each restart) for a
/// mapper unit. Resolves the named component and assembles it via
/// `TEdgeComponent::build` — no lock, no signal handling, no run-to-completion.
pub async fn build(name: MapperName, config: TEdgeConfig) -> anyhow::Result<Runtime> {
    let component = lookup_component(name)?;
    let config_root = config.config_root();
    let mappers_dir = config_root.dir("mappers")?;
    core::mappers_dir::warn_misconfigured_mapper_dirs(mappers_dir.path()).await;
    component.build(config, &config_root).await
}

/// Acquires a mapper's single-instance lock, if locking is enabled.
///
/// `mapper_name` is the full service name (e.g. `tedge-mapper-c8y`). The supervisor
/// takes this once per process and holds it for the mapper unit's whole lifetime
/// (across restarts), so it guards only against an external duplicate.
pub fn acquire_lock(
    mapper_name: &str,
    config: &TEdgeConfig,
) -> Result<Option<Flockfile>, FlockfileError> {
    if config.run.lock_files {
        check_another_instance_is_not_running(mapper_name, config.run.path.as_std_path())
    } else {
        Ok(None)
    }
}

pub fn mapper_dir(
    config_dir: &TedgePaths,
    mapper: &str,
    profile: Option<&(impl fmt::Display + ?Sized)>,
) -> ManagedDir {
    let profiled_name = match profile {
        None => mapper.to_string(),
        Some(profile) => format!("{mapper}.{profile}"),
    };
    config_dir.dir(format!("mappers/{profiled_name}")).unwrap()
}

pub(crate) fn flows_config(
    tedge_config: &TEdgeConfig,
    mapper_name: &str,
) -> Result<FlowsMapperConfig, anyhow::Error> {
    let te = tedge_config.mqtt.topic_root.as_str();
    let service_topic_id = EntityTopicId::default_main_service(mapper_name)?;

    let stats_config = &tedge_config.flows.stats;
    let mem_config = &tedge_config.flows.memory;
    let flows_config = FlowsMapperConfig::new(
        &format!("{te}/{service_topic_id}"),
        stats_config.interval.duration(),
        stats_config.on_message,
        stats_config.on_interval,
        stats_config.on_startup,
    )
    .with_js_config(
        mem_config.heap_size as usize,
        mem_config.stack_size as usize,
    );
    Ok(flows_config)
}

fn load_builtin_transformers(flows: &mut impl FlowRegistryExt) {
    #[cfg(feature = "c8y")]
    c8y_mapper_ext::load_builtin_transformers(flows);
    #[cfg(feature = "azure")]
    az_mapper_ext::load_builtin_transformers(flows);
    #[cfg(feature = "aws")]
    aws_mapper_ext::load_builtin_transformers(flows);
}

pub(crate) async fn mapper_flow_registry(
    tedge_config: &TEdgeConfig,
    mapper_dir: &ManagedDir,
) -> anyhow::Result<ConnectedFlowRegistry> {
    let flows_dir = tedge_flows::managed_flows_dir(mapper_dir);
    let mapper_config = effective_mapper_config(tedge_config, mapper_dir).await?;
    let flows = flow_registry(mapper_config, flows_dir).await?;
    Ok(flows)
}

pub async fn test_cli_flow_registry(
    tedge_config: &TEdgeConfig,
    mapper_dir: impl AsRef<Utf8Path>,
    flows_dir: impl AsRef<Utf8Path>,
) -> anyhow::Result<BaseFlowRegistry> {
    let mapper_config = effective_mapper_config(tedge_config, mapper_dir).await?;
    let mut flows = match mapper_config {
        None => BaseFlowRegistry::new(HashMap::new(), flows_dir),
        Some(effective_mapper_config) => BaseFlowRegistry::new(effective_mapper_config, flows_dir),
    }?;
    load_builtin_transformers(&mut flows);
    Ok(flows)
}

async fn flow_registry(
    mapper_config: Option<EffectiveMapperConfig>,
    flows_dir: ManagedDir,
) -> Result<ConnectedFlowRegistry, UpdateFlowRegistryError> {
    if let Err(err) = flows_dir.ensure().await {
        error!(
            "failed to create flow directory '{}': {err}",
            flows_dir.as_ref()
        );
        Err(err)?;
    };

    let mut flows = match mapper_config {
        None => ConnectedFlowRegistry::new(HashMap::new(), flows_dir),
        Some(effective_mapper_config) => {
            ConnectedFlowRegistry::new(effective_mapper_config, flows_dir)
        }
    }?;

    load_builtin_transformers(&mut flows);
    Ok(flows)
}

async fn effective_mapper_config(
    tedge_config: &TEdgeConfig,
    mapper_dir: impl AsRef<Utf8Path>,
) -> anyhow::Result<Option<EffectiveMapperConfig>> {
    let Some(raw) = custom::config::load_mapper_config(mapper_dir.as_ref()).await? else {
        return Ok(None);
    };
    let Some(mapper_fullname) = mapper_dir.as_ref().file_name() else {
        bail!(
            "Cannot derive the mapper name from its directory: {}",
            mapper_dir.as_ref()
        );
    };
    let (mapper_name, profile) = match mapper_fullname.split_once('.') {
        None => (mapper_fullname, None),
        Some((mapper_name, profile)) => (mapper_name, Some(ProfileName::from_str(profile)?)),
    };
    match mapper_name {
        #[cfg(feature = "c8y")]
        "c8y" => Ok(Some(
            c8y::mapper::resolve_effective_mapper_config(tedge_config, profile.as_ref()).await?,
        )),

        #[cfg(feature = "aws")]
        "aws" => Ok(Some(
            aws::mapper::resolve_effective_mapper_config(tedge_config, profile.as_ref()).await?,
        )),

        #[cfg(feature = "azure")]
        "az" => Ok(Some(
            az::mapper::resolve_effective_mapper_config(tedge_config, profile.as_ref()).await?,
        )),

        _ => Ok(Some(
            custom::resolve::resolve_effective_config(&raw, tedge_config, None, None).await?,
        )),
    }
}

#[cfg(test)]
pub(crate) mod test_helpers {
    pub(crate) fn current_user_group() -> (String, String) {
        let user = std::process::Command::new("id")
            .arg("-un")
            .output()
            .ok()
            .and_then(|o| String::from_utf8(o.stdout).ok())
            .map(|s| s.trim().to_owned())
            .unwrap();
        let group = std::process::Command::new("id")
            .arg("-gn")
            .output()
            .ok()
            .and_then(|o| String::from_utf8(o.stdout).ok())
            .map(|s| s.trim().to_owned())
            .unwrap();
        (user, group)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    mod mapper_name_display {
        use super::*;

        #[test]
        fn user_defined_display() {
            let name = MapperName::UserDefined(vec!["thingsboard".to_string()]);
            assert_eq!(name.to_string(), "tedge-mapper-thingsboard");
        }
    }

    mod validate_mapper_name_tests {
        use super::*;

        #[test]
        fn valid_name() {
            assert!(validate_mapper_name("thingsboard").is_ok());
            assert!(validate_mapper_name("my-cloud").is_ok());
            assert!(validate_mapper_name("abc123").is_ok());
        }

        #[test]
        fn empty_name_errors() {
            assert!(validate_mapper_name("").is_err());
        }

        #[test]
        fn underscore_errors() {
            let err = validate_mapper_name("my_cloud").unwrap_err();
            assert!(format!("{err}").contains("underscores are not allowed"));
        }

        #[test]
        fn uppercase_errors() {
            let err = validate_mapper_name("MyCloud").unwrap_err();
            assert!(format!("{err}").contains("lowercase"));
        }

        #[test]
        fn starts_with_digit_errors() {
            assert!(validate_mapper_name("1cloud").is_err());
        }

        #[test]
        fn bridge_prefix_errors() {
            let err = validate_mapper_name("bridge-cloud").unwrap_err();
            assert!(format!("{err}").contains("bridge-"));
        }
    }
}
