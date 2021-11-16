mod error;

use crate::error::InternalError;
use serde::Deserialize;
use std::fs::{self, File};
use std::os::unix::prelude::ExitStatusExt;
use std::path::Path;
use std::process::{Command, ExitStatus, Stdio};
use structopt::StructOpt;

#[derive(StructOpt)]
struct ApamaCli {
    #[structopt(subcommand)]
    operation: PluginOp,
}

#[derive(StructOpt)]
pub enum PluginOp {
    /// List all the installed modules
    List,

    /// Install a module
    Install {
        module: String,
        #[structopt(short = "v", long = "--module-version")]
        version: Option<String>,
        #[structopt(long = "--file")]
        file_path: String,
    },

    /// Uninstall a module
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

const APAMA_ENV_EXE: &str = "/opt/softwareag/Apama/bin/apama_env";
const TEDGE_APAMA_PROJECT_DIR: &str = "/etc/tedge/apama/project";
const TMP_APAMA_PROJECT_DIR: &str = "/tmp/tedge_apama_project";

fn run(operation: PluginOp) -> Result<ExitStatus, InternalError> {
    let success = ExitStatus::from_raw(0);

    let tedge_env_exe_path = Path::new(APAMA_ENV_EXE);

    if !tedge_env_exe_path.exists() {
        return Err(InternalError::ApamaNotInstalled);
    }

    let tedge_apama_project_path = Path::new(TEDGE_APAMA_PROJECT_DIR);
    let tmp_apama_project_path = Path::new(TMP_APAMA_PROJECT_DIR);

    let status = match operation {
        PluginOp::List => {
            if tedge_apama_project_path.exists() {
                println!("project\t")
            }
            success
        }

        PluginOp::Prepare => success,

        PluginOp::Finalize => {
            fs::remove_dir_all(tmp_apama_project_path)?;
            success
        }

        PluginOp::Install {
            module: _,
            version: _,
            file_path,
        } => {
            let archive_path = Path::new(&file_path);
            let archive_file = File::open(&archive_path)?;

            let mut archive = zip::ZipArchive::new(archive_file)?;

            // TODO: Validate the zip to be valid apama project before extraction?
            println!("Extracting the archive at {}", file_path);
            archive.extract(tmp_apama_project_path)?;
            println!("Extraction successful");

            // Deleting existing project as the rename API expects the target dir to be empty
            if tedge_apama_project_path.exists() {
                println!("Removing existing project at {}", TEDGE_APAMA_PROJECT_DIR);
                fs::remove_dir_all(tedge_apama_project_path)?;
                println!("Removal of existing project successful");
            }

            println!(
                "Installing newly extracted project to {}",
                TEDGE_APAMA_PROJECT_DIR
            );
            fs::create_dir_all(tedge_apama_project_path)?;
            fs::rename(tmp_apama_project_path, tedge_apama_project_path)?;
            println!("Installation of new project successful");

            println!("Restarting apama to load the new project");
            run_cmd("service", "apama restart")?;
            println!("Restart of apama service successful");

            success
        }

        PluginOp::Remove {
            module: _,
            version: _,
        } => {
            println!("Removing existing project at {}", TEDGE_APAMA_PROJECT_DIR);
            fs::remove_dir_all(tedge_apama_project_path)?;
            println!("Removal of existing project successful");

            println!("Stopping apama service");
            run_cmd("service", "apama stop")?;
            println!("Stopping apama service successful");

            success
        }
    };

    Ok(status)
}

fn run_cmd(cmd: &str, args: &str) -> Result<ExitStatus, InternalError> {
    let args: Vec<&str> = args.split_whitespace().collect();
    let status = Command::new(cmd)
        .args(args)
        .stdin(Stdio::null())
        .status()
        .map_err(|err| InternalError::exec_error(cmd, err))?;
    Ok(status)
}

fn main() {
    // On usage error, the process exits with a status code of 1
    let apt = ApamaCli::from_args();

    match run(apt.operation) {
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
