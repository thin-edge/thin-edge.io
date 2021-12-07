mod error;

use crate::error::InternalError;
use std::fs::{self, File};
use std::io::ErrorKind;
use std::path::Path;
use std::process::{Command, Stdio};
use structopt::StructOpt;

/// This plugin supports the installation, update and removal of a single unversioned apama project named "project".
/// Installation of multiple parallel projects is not supported.
/// Installing a project will replace the existing project with the new one.
/// Delta update of a project(for eg: updating just the `mon` file definitions in the project) is not supported either.
#[derive(StructOpt)]
struct ApamaCli {
    #[structopt(subcommand)]
    operation: PluginOp,
}

#[derive(StructOpt)]
pub enum PluginOp {
    /// List the one and only apama project if one is installed
    List,

    /// Install an apama project
    Install {
        module: String,
        #[structopt(short = "v", long = "--module-version")]
        version: Option<String>,
        #[structopt(long = "--file")]
        file_path: String,
    },

    /// Remove an apama project
    Remove {
        module: String,
        #[structopt(short = "v", long = "--module-version")]
        version: Option<String>,
    },

    /// Prepare a sequences of install/remove commands
    Prepare,

    /// Finalize a sequences of install/remove commands
    Finalize,
}

const APAMA_ENV_EXE: &str = "/opt/softwareag/Apama/bin/apama_env";
const TEDGE_APAMA_PROJECT_DIR: &str = "/etc/tedge/apama/project";
const TMP_APAMA_PROJECT_DIR: &str = "/tmp/tedge_apama_project";
const DEFAULT_APAMA_PROJECT_NAME: &str = "unnamed";
const APAMA_PROJECT_DESCRIPTOR_FILE: &str = "/etc/tedge/apama/project/.project";

const ENGINE_INJECT_CMD: &str = "engine_inject";
const ENGINE_INSPECT_CMD: &str = "engine_inspect";
const ENGINE_DELETE_CMD: &str = "engine_delete";

const PROJECT_SUFFIX: &str = "project";
const MON_SUFFIX: &str = "mon";

enum ApamaModule {
    Project(String),
    MonFile(String),
}

fn run(operation: PluginOp) -> Result<(), InternalError> {
    let tedge_env_exe_path = Path::new(APAMA_ENV_EXE);
    let tedge_apama_project_path: &Path = Path::new(TEDGE_APAMA_PROJECT_DIR);
    let tmp_apama_project_path: &Path = Path::new(TMP_APAMA_PROJECT_DIR);

    if !tedge_env_exe_path.exists() {
        return Err(InternalError::ApamaNotInstalled);
    }

    match operation {
        PluginOp::List => {
            if tedge_apama_project_path.exists() {
                // Print the project name
                println!("{}::project\t", get_project_name()?);

                // Print the installed monitors
                for monitor in get_installed_monitors()? {
                    println!("{}::mon\t", monitor)
                }
            }
            Ok(())
        }

        PluginOp::Prepare => Ok(()),

        PluginOp::Finalize => {
            // Cleanup any temporary artefacts created by this plugin
            if tmp_apama_project_path.exists() {
                fs::remove_dir_all(tmp_apama_project_path)?;
            }
            Ok(())
        }

        PluginOp::Install {
            module,
            version: _,
            file_path,
        } => match apama_module_from_string(&module)? {
            ApamaModule::Project(_) => install_project(Path::new(&file_path)),
            ApamaModule::MonFile(monitor_name) => {
                install_or_update_monitor(&monitor_name, &file_path)
            }
        },

        PluginOp::Remove { module, version: _ } => match apama_module_from_string(&module)? {
            ApamaModule::Project(project_name) => remove_project(&project_name),
            ApamaModule::MonFile(monitor_name) => remove_monitor(&monitor_name),
        },
    }
}

fn get_project_name() -> Result<String, InternalError> {
    let xml_content = fs::read_to_string(APAMA_PROJECT_DESCRIPTOR_FILE).unwrap();
    let root = roxmltree::Document::parse(xml_content.as_str())?;
    let name_node = root.descendants().find(|node| node.has_tag_name("name"));
    let name = name_node
        .and_then(|node| node.first_child())
        .and_then(|node| node.text())
        .unwrap_or(DEFAULT_APAMA_PROJECT_NAME);
    Ok(name.into())
}

fn apama_module_from_string(module: &str) -> Result<ApamaModule, InternalError> {
    match module.rsplit_once("::") {
        Some((prefix, suffix)) => match suffix {
            PROJECT_SUFFIX => Ok(ApamaModule::Project(prefix.into())),
            MON_SUFFIX => Ok(ApamaModule::MonFile(prefix.into())),
            unsupported_type => Err(InternalError::UnsupportedModuleType {
                module_type: unsupported_type.into(),
            }),
        },
        None => Err(InternalError::ModuleTypeNotProvided {
            module_name: module.into(),
        }),
    }
}

fn install_project(project_archive_path: &Path) -> Result<(), InternalError> {
    let tedge_apama_project_path: &Path = Path::new(TEDGE_APAMA_PROJECT_DIR);
    let tmp_apama_project_path: &Path = Path::new(TMP_APAMA_PROJECT_DIR);

    let archive_file = File::open(&project_archive_path)?;

    let mut archive = zip::ZipArchive::new(archive_file)?;

    if let Err(zip::result::ZipError::FileNotFound) = archive.by_name("project/") {
        return Err(InternalError::InvalidProjectArchive);
    }

    println!("Extracting the archive at {:?}", project_archive_path);
    archive.extract(tmp_apama_project_path)?;
    println!("Extraction successful");

    // Deleting existing project as the rename API expects the target dir to be empty
    delete_project()?;

    println!(
        "Installing newly extracted project to {}",
        TEDGE_APAMA_PROJECT_DIR
    );
    fs::create_dir_all(tedge_apama_project_path)?;
    fs::rename(
        tmp_apama_project_path.join("project"),
        tedge_apama_project_path,
    )?;
    println!("Installation of new project successful");

    restart_apama_service()?;

    Ok(())
}

fn restart_apama_service() -> Result<(), InternalError> {
    println!("Restarting apama to load the new project");
    run_cmd("service", "apama restart")?;
    println!("Restart of apama service successful");
    Ok(())
}

fn remove_project(_project_name: &str) -> Result<(), InternalError> {
    let tedge_apama_project_path: &Path = Path::new(TEDGE_APAMA_PROJECT_DIR);

    if tedge_apama_project_path.exists() {
        stop_apama_service()?;
        delete_project()?;
    } else {
        println!("Doing nothing as there's no project installed");
    }
    Ok(())
}

fn delete_project() -> Result<(), InternalError> {
    println!("Removing existing project at {}", TEDGE_APAMA_PROJECT_DIR);
    let result = fs::remove_dir_all(TEDGE_APAMA_PROJECT_DIR);
    if let Err(err) = result {
        if err.kind() != ErrorKind::NotFound {
            return Err(err)?;
        }
    }
    println!("Removal of existing project successful");
    Ok(())
}

fn stop_apama_service() -> Result<(), InternalError> {
    println!("Stopping apama service");
    run_cmd("service", "apama stop")?;
    println!("Stopping apama service successful");
    Ok(())
}

fn get_installed_monitors() -> Result<Vec<String>, InternalError> {
    // Run `engine_inspect -m -r` command to list all monitors in raw format
    let output = Command::new(APAMA_ENV_EXE)
        .arg(ENGINE_INSPECT_CMD)
        .arg("-m")
        .arg("-r")
        .stdin(Stdio::null())
        .output()
        .map_err(|err| InternalError::exec_error(ENGINE_INSPECT_CMD, err))?;
    let output = String::from_utf8(output.stdout)?;

    // The output contains monitor names and their instance counts separated by a space as follows:
    // ```
    // TedgeDemoMonitor 1
    // TedgeTestMonitor 1
    // ```
    let mon_files = output
        .lines()
        // The first line of the output could "WARNING: JAVA_HOME not set" which is filtered out
        .filter(|line| !line.starts_with("WARNING:"))
        // The counts are filtered out too
        .filter_map(|line| line.split_whitespace().next())
        .map(|line| line.into())
        .collect();

    Ok(mon_files)
}

fn install_or_update_monitor(mon_name: &str, mon_file_path: &str) -> Result<(), InternalError> {
    let installed_monitors = get_installed_monitors()?;
    // If an existing monitor needs to be updated, older version needs to be removed first before installing the new one
    if installed_monitors.contains(&mon_name.to_string()) {
        remove_monitor(mon_name)?;
        install_monitor(mon_file_path)
    } else {
        install_monitor(mon_file_path)
    }
}

fn install_monitor(mon_file_path: &str) -> Result<(), InternalError> {
    run_cmd(
        APAMA_ENV_EXE,
        format!("{} {}", ENGINE_INJECT_CMD, mon_file_path).as_str(),
    )
}

fn remove_monitor(mon_name: &str) -> Result<(), InternalError> {
    run_cmd(
        APAMA_ENV_EXE,
        format!("{} {}", ENGINE_DELETE_CMD, mon_name).as_str(),
    )
}

fn run_cmd(cmd: &str, args: &str) -> Result<(), InternalError> {
    let args: Vec<&str> = args.split_whitespace().collect();
    let exit_status = Command::new(cmd)
        .args(args)
        .stdin(Stdio::null())
        .status()
        .map_err(|err| InternalError::exec_error(cmd, err))?;

    if exit_status.success() {
        Ok(())
    } else {
        Err(InternalError::ExecFailure {
            cmd: cmd.into(),
            exit_status,
        })
    }
}

fn main() {
    // On usage error, the process exits with a status code of 1
    let apama = ApamaCli::from_args();

    match run(apama.operation) {
        Ok(()) => {
            std::process::exit(0);
        }

        Err(err) => {
            eprintln!("ERROR: {}", err);
            std::process::exit(2);
        }
    }
}
