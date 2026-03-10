mod error;
mod install;
mod list;
mod remove;

use camino::Utf8Path;
use camino::Utf8PathBuf;
use clap::Parser;
use serde::Deserialize;
use std::fs;
use std::path::Path;
use tedge_config::cli::CommonArgs;
use tedge_config::log_init;
use tedge_config::TEdgeConfig;
use tracing::debug;
use tracing::error;
use tracing::warn;

use crate::error::io_error;
use crate::error::FlowsPluginError;

pub(crate) const PARAMS_FILE: &str = "params.toml";
pub(crate) const DEFAULT_VERSION: &str = "0.0.0";

#[derive(Parser, Debug)]
#[clap(
    name = clap::crate_name!(),
    version = clap::crate_version!(),
    about = clap::crate_description!(),
    arg_required_else_help(true)
)]
pub struct FlowsCli {
    #[command(flatten)]
    pub common: CommonArgs,

    #[clap(subcommand)]
    pub operation: PluginOp,
}

#[derive(clap::Subcommand, Debug)]
pub enum PluginOp {
    /// List all installed flows (sm-plugin API)
    List,

    /// Install a flow from a .tar or .tar.gz archive (sm-plugin API)
    Install {
        /// Name of flow in the form <mapper>/<flow-name> (sm-plugin API)
        module: String,

        /// Version of the flow to install (sm-plugin API)
        #[clap(short = 'v', long = "module-version")]
        version: Option<String>,

        /// Path to the .tar or .tar.gz file (sm-plugin API)
        #[clap(long = "file")]
        file_path: String,
    },

    /// Remove an installed flow (sm-plugin API)
    Remove {
        /// Name of flow in the form <mapper>/<flow-name> (sm-plugin API)
        module: String,

        /// Version of the flow to remove (sm-plugin API): not used
        #[clap(short = 'v', long = "module-version")]
        version: Option<String>,

        /// If set and params.toml exists, only the params.toml file is kept; otherwise, the entire flow directory is deleted
        #[clap(long = "keep-params")]
        keep_params: bool,
    },

    /// Install or remove multiple modules at once (sm-plugin API): not supported
    UpdateList,

    /// Prepare a sequences of install/remove commands (sm-plugin API): do nothing
    Prepare,

    /// Finalize a sequences of install/remove commands (sm-plugin API): do nothing
    Finalize,
}

fn run_op(flows: FlowsCli, tedge_config: Option<TEdgeConfig>) -> Result<(), FlowsPluginError> {
    if let Err(err) = log_init(
        "tedge-flows-plugin",
        &flows.common.log_args,
        &flows.common.config_dir,
    ) {
        error!("Can't enable logging due to error: {err}");
    }
    let mappers_dir = flows.common.config_dir.join("mappers");

    match flows.operation {
        PluginOp::List => {
            crate::list::list_flows(&mappers_dir);
        }
        PluginOp::Install {
            module,
            version,
            file_path,
        } => {
            let flow_record = FlowRecord::new(&module)?;
            crate::install::install_flow(
                flows.common.config_dir,
                &mappers_dir,
                &flow_record,
                version,
                &file_path,
                true,
            )?;
        }
        PluginOp::Remove {
            module,
            version,
            keep_params,
        } => {
            let flow_record = FlowRecord::new(&module)?;
            let keep_params = keep_params
                || tedge_config
                    .as_ref()
                    .is_some_and(|config| config.flows.params.keep_on_delete);
            crate::remove::remove_flow(&mappers_dir, &flow_record, version, keep_params)?;
        }
        PluginOp::UpdateList => return Err(FlowsPluginError::InvalidUsage),
        PluginOp::Prepare => {}
        PluginOp::Finalize => {}
    };

    Ok(())
}

#[derive(Debug)]
pub struct FlowRecord {
    pub mapper_name: String,
    pub flow_name: String,
}

impl FlowRecord {
    fn new(module: &str) -> Result<Self, FlowsPluginError> {
        if !Self::is_safe_path(module) {
            return Err(FlowsPluginError::InvalidModuleName(module.to_owned()));
        }

        let (mapper_name, flow_name) = module
            .split_once('/')
            .ok_or_else(|| FlowsPluginError::InvalidModuleName(module.to_owned()))?;

        Ok(Self {
            mapper_name: mapper_name.to_string(),
            flow_name: flow_name.to_string(),
        })
    }

    /// Validate the input module name to prevent directory traversal attacks
    fn is_safe_path(module: &str) -> bool {
        // Reject no mapper name
        if module.starts_with('/') {
            return false;
        }
        // Reject any empty, ".", or ".." path components
        for component in module.split('/') {
            if let "" | "." | ".." = component {
                return false;
            }
        }
        true
    }

    /// The directory path for a mapper (e.g., /etc/tedge/mappers/local)
    fn mapper_dir(&self, mappers_dir: impl AsRef<Utf8Path>) -> Utf8PathBuf {
        mappers_dir.as_ref().join(&self.mapper_name)
    }

    /// The directory path for the root of flows for a mapper (e.g., /etc/tedge/mappers/local/flows)
    fn flows_dir(&self, mappers_dir: impl AsRef<Utf8Path>) -> Utf8PathBuf {
        self.mapper_dir(mappers_dir).join("flows")
    }

    /// The directory path for a flow (e.g., /etc/tedge/mappers/local/flows/hello)
    fn flow_dir(&self, mappers_dir: impl AsRef<Utf8Path>) -> Utf8PathBuf {
        self.flows_dir(mappers_dir).join(&self.flow_name)
    }

    /// The toml file path for a flow (e.g., /etc/tedge/mappers/local/flows/hello.toml)
    fn flow_toml(&self, mappers_dir: impl AsRef<Utf8Path>) -> Utf8PathBuf {
        self.flows_dir(mappers_dir)
            .join(format!("{}.toml", self.flow_name))
    }
}

impl std::fmt::Display for FlowRecord {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}/{}", self.mapper_name, self.flow_name)
    }
}

/// Flow metadata extracted from the flow.toml file
#[derive(Deserialize, Debug)]
struct FlowMeta {
    version: Option<String>,
}

/// Get flow's version from its flow.toml file
pub fn get_flow_version(flow_toml: impl AsRef<Utf8Path>) -> String {
    let flow_toml = flow_toml.as_ref();
    extract_flow_version(flow_toml).unwrap_or_else(|e| {
        warn!("Failed to read a file {flow_toml}: {e}. Defaulting to {DEFAULT_VERSION}");
        DEFAULT_VERSION.to_string()
    })
}

fn extract_flow_version(flow_toml: impl AsRef<Utf8Path>) -> Result<String, FlowsPluginError> {
    let flow_toml = flow_toml.as_ref();
    let content = fs::read_to_string(flow_toml).map_err(|e| io_error(flow_toml, e))?;

    let toml: FlowMeta =
        toml::from_str(&content).map_err(|e| FlowsPluginError::ParseFlowTomlError {
            path: flow_toml.to_path_buf(),
            source: e,
        })?;

    match toml.version {
        Some(v) => Ok(v),
        None => {
            debug!("version field missing at {flow_toml}, defaulting to {DEFAULT_VERSION}");
            Ok(DEFAULT_VERSION.to_string())
        }
    }
}

pub async fn get_config(config_dir: &Path) -> Option<TEdgeConfig> {
    match TEdgeConfig::load(&config_dir).await {
        Ok(config) => Some(config),
        Err(err) => {
            warn!("Failed to load TEdgeConfig: {err}");
            None
        }
    }
}

pub fn run_and_exit(flows: FlowsCli, tedge_config: Option<TEdgeConfig>) -> ! {
    match run_op(flows, tedge_config) {
        Ok(()) => std::process::exit(0),
        Err(FlowsPluginError::InvalidUsage) => {
            std::process::exit(1);
        }
        Err(err) => {
            eprintln!("ERROR: {err}");
            std::process::exit(2);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use test_case::test_case;

    #[test_case("local/hello","local", "hello"; "simple case")]
    #[test_case("local/hello/world", "local", "hello/world"; "flow name with two segments")]
    #[test_case("c8y/a/b/c", "c8y", "a/b/c"; "flow name with many segments")]
    #[test_case("c8y/a.a/.bb/c..c/dd..", "c8y", "a.a/.bb/c..c/dd.."; "flow name with dots but no traversal")]
    fn parse_module_name_ok(module: &str, expected_mapper: &str, expected_flow: &str) {
        let flow_record = FlowRecord::new(module).unwrap();
        assert_eq!(flow_record.mapper_name, expected_mapper);
        assert_eq!(flow_record.flow_name, expected_flow);
    }

    #[test_case("noslash"; "no slash at all")]
    #[test_case("../hello"; "mapper is dot-dot")]
    #[test_case("../hello/world"; "mapper is dot-dot with multi-segment flow")]
    #[test_case("./hello"; "mapper is dot")]
    #[test_case("/etc/hello"; "absolute path as mapper")]
    #[test_case("/local/hello"; "leading slash makes mapper empty")]
    #[test_case("local/../secret"; "flow name dot-dot traversal")]
    #[test_case("local/hello/./world"; "flow name dot segment in middle")]
    #[test_case("local/../hello/"; "flow name trailing slash")]
    #[test_case("local/hello/."; "flow name trailing dot")]
    #[test_case("local/hello/.."; "flow name trailing dot-dot")]
    #[test_case("local//etc/passwd"; "flow name with empty segment from double slash")]
    fn parse_module_name_err(module: &str) {
        assert!(
            FlowRecord::new(module).is_err(),
            "expected error for {module:?}"
        );
    }
}
