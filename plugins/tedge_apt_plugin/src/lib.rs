mod error;
mod module_check;

use crate::error::InternalError;
use crate::module_check::PackageMetadata;
use regex::Regex;
use serde::Deserialize;
use std::io;
use std::path::Path;
use std::process::Command;
use std::process::ExitStatus;
use std::process::Stdio;
use tedge_config::cli::CommonArgs;
use tedge_config::system_services::log_init;
use tedge_config::AptConfig;
use tedge_config::TEdgeConfig;
use tedge_config::TEdgeConfigLocation;
use tracing::error;
use tracing::warn;

#[derive(clap::Parser, Debug)]
#[clap(
    name = clap::crate_name!(),
    version = clap::crate_version!(),
    about = clap::crate_description!(),
    arg_required_else_help(true)
)]
pub struct AptCli {
    #[command(flatten)]
    common: CommonArgs,

    #[clap(subcommand)]
    operation: PluginOp,
}

#[derive(clap::Subcommand, Debug)]
pub enum PluginOp {
    /// List all the installed modules
    List {
        /// Filter packages list output by name
        #[clap(long, short)]
        name: Option<String>,

        /// Filter packages list output by maintainer
        #[clap(long, short)]
        maintainer: Option<String>,
    },

    /// Install a module
    Install {
        module: String,
        #[clap(short = 'v', long = "module-version")]
        version: Option<String>,
        #[clap(long = "file")]
        file_path: Option<String>,
    },

    /// Uninstall a module
    Remove {
        module: String,
        #[clap(short = 'v', long = "module-version")]
        version: Option<String>,
    },

    /// Install or remove multiple modules at once
    UpdateList,

    /// Prepare a sequences of install/remove commands
    Prepare,

    /// Finalize a sequences of install/remove commands
    Finalize,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "lowercase")]
enum UpdateAction {
    Install,
    Remove,
}
#[derive(Debug, Deserialize)]
struct SoftwareModuleUpdate {
    pub action: UpdateAction,
    pub name: String,
    #[serde(default)]
    pub version: Option<String>,
    #[serde(default)]
    pub path: Option<String>,
}

fn run_op(apt: AptCli) -> Result<ExitStatus, InternalError> {
    if let Err(err) = log_init(
        "tedge-apt-plugin",
        &apt.common.log_args,
        &apt.common.config_dir,
    ) {
        error!("Can't enable logging due to error: {err}");
    }
    let status = match apt.operation {
        PluginOp::List { name, maintainer } => {
            let dpkg_query = Command::new("dpkg-query")
                .args(vec![
                    "-f",
                    "${Package}\t${Version}\t${Maintainer}\t${Status}\n",
                    "-W",
                ])
                .stdout(Stdio::piped())
                .spawn()
                .map_err(|err| InternalError::exec_error("dpkg-query", err))?
                .wait_with_output()
                .map_err(|err| InternalError::exec_error("dpkg-query", err))?;

            let stdout = String::from_utf8(dpkg_query.stdout).unwrap_or_default();

            let filter = match (&name, &maintainer) {
                (None, None) => Regex::new(r"install ok installed").unwrap(),

                _ => match Regex::new(
                    format!(
                        r"(^{}\t.*|^\S+\t\S+\t{}\s+.*)install ok installed",
                        name.unwrap_or_default(),
                        maintainer.unwrap_or_default()
                    )
                    .as_str(),
                ) {
                    Ok(filter) => filter,
                    Err(err) => {
                        eprintln!("tedge-apt-plugin fails to list packages with matching name and maintainer: {err}");
                        std::process::exit(1)
                    }
                },
            };

            for line in stdout.trim_end().lines() {
                if filter.is_match(line) {
                    let (name, version) = get_name_and_version(line);
                    println!("{name}\t{version}");
                }
            }

            dpkg_query.status
        }

        PluginOp::Install {
            module,
            version,
            file_path,
        } => {
            let (installer, _metadata) = get_installer(module, version, file_path)?;
            let dpk_option = get_dpk_option(apt.common.config_dir.as_std_path());
            AptGetCmd::Install(dpk_option, vec![installer]).run()?
        }

        PluginOp::Remove { module, version } => {
            let package = match version {
                None => module,
                Some(version) => format!("{}={}", module, version),
            };
            AptGetCmd::Remove(package).run()?
        }

        PluginOp::UpdateList => {
            let mut updates: Vec<SoftwareModuleUpdate> = Vec::new();
            let mut rdr = csv::ReaderBuilder::new()
                .has_headers(false)
                .delimiter(b'\t')
                .from_reader(io::stdin());
            for result in rdr.deserialize() {
                updates.push(result?);
            }

            // Maintaining this metadata list to keep the debian package symlinks until the installation is complete,
            // which will get cleaned up once it goes out of scope after this block
            let mut metadata_vec = Vec::new();
            let mut args: Vec<String> = Vec::new();
            let dpk_option = get_dpk_option(apt.common.config_dir.as_std_path());

            for update_module in updates {
                match update_module.action {
                    UpdateAction::Install => {
                        // if version is `latest` we want to set `version` to an empty value, so
                        // the apt plugin fetches the most up to date version.
                        let version = update_module.version.filter(|version| version != "latest");

                        let (installer, metadata) =
                            get_installer(update_module.name, version, update_module.path)?;
                        args.push(installer);
                        metadata_vec.push(metadata);
                    }
                    UpdateAction::Remove => {
                        if let Some(version) = update_module.version {
                            validate_version(update_module.name.as_str(), version.as_str())?
                        }

                        // Adding a '-' at the end of the package name like 'rolldice-' instructs apt to treat it as removal
                        args.push(format!("{}-", update_module.name))
                    }
                };
            }
            AptGetCmd::Install(dpk_option, args).run()?
        }

        PluginOp::Prepare => AptGetCmd::Update.run()?,

        PluginOp::Finalize => AptGetCmd::AutoRemove.run()?,
    };

    Ok(status)
}

fn get_installer(
    module: String,
    version: Option<String>,
    file_path: Option<String>,
) -> Result<(String, Option<PackageMetadata>), InternalError> {
    match (&version, &file_path) {
        (None, None) => Ok((module, None)),

        (Some(version), None) => Ok((format!("{}={}", module, version), None)),

        (None, Some(file_path)) => {
            let mut package = PackageMetadata::try_new(file_path)?;
            package.validate_package(&[&format!("Package: {}", &module), "Debian package"])?;
            Ok((format!("{}", package.file_path().display()), Some(package)))
        }

        (Some(version), Some(file_path)) => {
            let mut package = PackageMetadata::try_new(file_path)?;
            package.validate_package(&[
                &format!("Version: {}", &version),
                &format!("Package: {}", &module),
                "Debian package",
            ])?;

            Ok((format!("{}", package.file_path().display()), Some(package)))
        }
    }
}

/// Validate if the provided module version matches the currently installed version
fn validate_version(module_name: &str, module_version: &str) -> Result<(), InternalError> {
    // Get the current installed version of the provided package
    let output = Command::new("apt")
        .arg("list")
        .arg("--installed")
        .arg(module_name)
        .output()
        .map_err(|err| InternalError::exec_error("apt-get", err))?;

    let stdout = String::from_utf8(output.stdout)?;

    // Check if the installed version and the provided version match
    let second_line = stdout.lines().nth(1); //Ignore line 0 which is always 'Listing...'
    if let Some(package_info) = second_line {
        if let Some(installed_version) = package_info.split_whitespace().nth(1)
        // Value at index 0 is the package name
        {
            if installed_version != module_version {
                return Err(InternalError::MetaDataMismatch {
                    package: module_name.into(),
                    expected_key: "Version".into(),
                    expected_value: installed_version.into(),
                    provided_value: module_version.into(),
                });
            }
        }
    }

    Ok(())
}

enum AptGetCmd {
    Install(AptConfig, Vec<String>),
    Remove(String),
    Update,
    AutoRemove,
}

impl AptGetCmd {
    fn run(&self) -> Result<ExitStatus, InternalError> {
        let mut cmd = Command::new("apt-get");
        // Keep all common options here
        cmd.args(["--quiet", "--yes"]);

        match self {
            AptGetCmd::Install(dpk_option, packages) => {
                let config_option = match dpk_option {
                    AptConfig::KeepOld => "DPkg::Options::=--force-confold",
                    AptConfig::KeepNew => "DPkg::Options::=--force-confnew",
                };
                cmd.args([
                    "-o",
                    config_option,
                    "install",
                    "--allow-downgrades",
                    "--no-install-recommends",
                ])
                .args(packages);
            }
            AptGetCmd::Remove(package) => {
                cmd.args(["remove", package]);
            }
            AptGetCmd::Update => {
                cmd.arg("update");
            }
            AptGetCmd::AutoRemove => {
                cmd.arg("autoremove");
            }
        }

        println!("Executing command: {cmd:?}");
        let status = cmd
            .env("DEBIAN_FRONTEND", "noninteractive")
            .stdin(Stdio::null())
            .status()
            .map_err(|err| InternalError::exec_error(format!("{cmd:?}"), err))?;

        Ok(status)
    }
}

fn get_dpk_option(config_dir: &Path) -> AptConfig {
    match get_config(config_dir) {
        None => AptConfig::KeepNew,
        Some(config) => config.apt.dpk.options.config.clone(),
    }
}

fn get_name_and_version(line: &str) -> (&str, &str) {
    let vec: Vec<&str> = line.split('\t').collect();

    let name = vec.first().unwrap_or(&"unknown name");
    let version = vec.get(1).unwrap_or(&"unknown version");
    (name, version)
}

fn get_config(config_dir: &Path) -> Option<TEdgeConfig> {
    let tedge_config_location = TEdgeConfigLocation::from_custom_root(config_dir);

    match TEdgeConfig::try_new(tedge_config_location) {
        Ok(config) => Some(config),
        Err(err) => {
            warn!("Failed to load TEdgeConfig: {}", err);
            None
        }
    }
}

pub fn run_and_exit(cli: Result<AptCli, clap::Error>) -> ! {
    let mut apt = match cli {
        Ok(aptcli) => aptcli,
        Err(err) => {
            err.print().expect("Failed to print help message");
            // re-write the clap exit_status from 2 to 1, if parse fails
            std::process::exit(1)
        }
    };

    if let PluginOp::List { name, maintainer } = &mut apt.operation {
        if let Some(config) = get_config(apt.common.config_dir.as_std_path()) {
            if name.is_none() {
                *name = config.apt.name.or_none().cloned();
            }

            if maintainer.is_none() {
                *maintainer = config.apt.maintainer.or_none().cloned();
            }
        }
    }

    match run_op(apt) {
        Ok(status) if status.success() => {
            std::process::exit(0);
        }

        Ok(status) => {
            if status.code().is_some() {
                std::process::exit(2);
            } else {
                eprintln!("Interrupted by a signal!");
                std::process::exit(4);
            }
        }

        Err(err) => {
            eprintln!("ERROR: {}", err);
            std::process::exit(5);
        }
    }
}

#[cfg(target_os = "linux")]
#[cfg(test)]
mod tests {
    use super::*;
    use tedge_config::cli::LogConfigArgs;
    use test_case::test_case;

    #[test_case(
    "zsh\t5.8-6+deb11u1\tDebian Zsh Maintainers <pkg-zsh-devel@lists.alioth.debian.org>\tinstall ok installed",
    "zsh", "5.8-6+deb11u1"
    ; "installed"
    )]
    fn get_package_name_and_version(line: &str, expected_name: &str, expected_version: &str) {
        let (name, version) = get_name_and_version(line);
        assert_eq!(name, expected_name);
        assert_eq!(version, expected_version);
    }

    #[test]
    fn both_filters_are_empty_strings() {
        let filters = PluginOp::List {
            name: Some("".into()),
            maintainer: Some("".into()),
        };
        let apt = AptCli {
            operation: filters,
            common: CommonArgs {
                log_args: LogConfigArgs {
                    debug: false,
                    log_level: None,
                },
                config_dir: "".into(),
            },
        };
        assert!(run_op(apt).is_ok())
    }
}
