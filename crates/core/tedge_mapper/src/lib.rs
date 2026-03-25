use std::fmt;

#[cfg(feature = "aws")]
use crate::aws::mapper::AwsMapper;
#[cfg(feature = "azure")]
use crate::az::mapper::AzureMapper;
#[cfg(feature = "c8y")]
use crate::c8y::mapper::CumulocityMapper;
use crate::collectd::mapper::CollectdMapper;
use crate::core::component::TEdgeComponent;
use crate::custom::mapper::CustomMapper;
use crate::flows::GenMapper;
use anyhow::Context;
use camino::Utf8Path;
use clap::Parser;
use flockfile::check_another_instance_is_not_running;
use tedge_api::mqtt_topics::EntityTopicId;
use tedge_config::cli::CommonArgs;
use tedge_config::log_init;
use tedge_config::tedge_toml::ProfileName;
use tedge_config::TEdgeConfig;
use tedge_flows::ConnectedFlowRegistry;
use tedge_flows::FlowRegistryExt;
use tedge_flows::FlowsMapperConfig;
use tedge_flows::UpdateFlowRegistryError;
use tedge_utils::file::create_directory_with_defaults;
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
mod aws;
#[cfg(feature = "azure")]
mod az;
#[cfg(feature = "c8y")]
pub mod c8y;
mod collectd;
mod core;
mod custom;
/// Re-export custom mapper config for use by bridge inspection commands.
pub use custom::config as custom_mapper_config;
/// Re-export custom mapper config resolution for use by CLI commands.
pub use custom::resolve as custom_mapper_resolve;
mod flows;

/// Set the cloud profile either from the CLI argument or env variable,
/// then set the environment variable so child processes automatically
/// have the correct profile set.
macro_rules! read_and_set_var {
    ($profile:ident, $var:literal) => {
        $profile
            .or_else(|| {
                Some(
                    std::env::var($var)
                        .ok()?
                        .parse()
                        .context(concat!("Reading environment variable ", $var))
                        .unwrap(),
                )
            })
            .inspect(|profile| std::env::set_var($var, profile))
    };
}

fn lookup_component(component_name: MapperName) -> anyhow::Result<Box<dyn TEdgeComponent>> {
    Ok(match component_name {
        #[cfg(feature = "azure")]
        MapperName::Az { profile } => Box::new(AzureMapper {
            profile: read_and_set_var!(profile, "TEDGE_CLOUD_PROFILE"),
        }),
        #[cfg(feature = "aws")]
        MapperName::Aws { profile } => Box::new(AwsMapper {
            profile: read_and_set_var!(profile, "TEDGE_CLOUD_PROFILE"),
        }),
        MapperName::Collectd => Box::new(CollectdMapper),
        #[cfg(feature = "c8y")]
        MapperName::C8y { profile } => Box::new(CumulocityMapper {
            profile: read_and_set_var!(profile, "TEDGE_CLOUD_PROFILE"),
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
        MapperName::Local => Box::new(GenMapper),
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
    Local,
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
            MapperName::Local => write!(f, "tedge-mapper-local"),
        }
    }
}

pub async fn run(mapper_opt: MapperOpt, config: TEdgeConfig) -> anyhow::Result<()> {
    let mapper_name = mapper_opt.name.to_string();
    let component = lookup_component(mapper_opt.name)?;

    log_init(
        "tedge-mapper",
        &mapper_opt.common.log_args,
        &mapper_opt.common.config_dir,
    )?;

    let mappers_dir = mapper_opt.common.config_dir.join("mappers");
    core::mappers_dir::warn_unrecognised_mapper_dirs(&mappers_dir).await;

    // Run only one instance of a mapper (if enabled)
    let mut _flock = None;
    if config.run.lock_files {
        let run_dir = config.run.path.as_std_path();
        _flock = check_another_instance_is_not_running(&mapper_name, run_dir)?;
    }

    if mapper_opt.init {
        warn!("This --init option has been deprecated and will be removed in a future release");
        Ok(())
    } else if mapper_opt.clear {
        warn!("This --clear option has been deprecated and will be removed in a future release");
        Ok(())
    } else {
        component
            .start(config, mapper_opt.common.config_dir.as_ref())
            .await
    }
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

pub fn load_builtin_transformers(flows: &mut impl FlowRegistryExt) {
    c8y_mapper_ext::load_builtin_transformers(flows);
    az_mapper_ext::load_builtin_transformers(flows);
    aws_mapper_ext::load_builtin_transformers(flows);
}

pub(crate) async fn flow_registry(
    flows_dir: impl AsRef<Utf8Path>,
) -> Result<ConnectedFlowRegistry, UpdateFlowRegistryError> {
    if let Err(err) = create_directory_with_defaults(flows_dir.as_ref()).await {
        error!(
            "failed to create flow directory '{}': {err}",
            flows_dir.as_ref()
        );
        return Err(err)?;
    };
    let mut flows = ConnectedFlowRegistry::new(flows_dir);
    load_builtin_transformers(&mut flows);
    Ok(flows)
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
