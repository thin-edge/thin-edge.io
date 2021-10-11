mod error;
mod module_check;

use crate::error::InternalError;
use crate::module_check::PackageMetadata;
use std::process::{Command, ExitStatus, Stdio};
use structopt::StructOpt;

#[derive(StructOpt)]
struct AptCli {
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
        file_path: Option<String>,
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

fn run(operation: PluginOp) -> Result<ExitStatus, InternalError> {
    let status = match operation {
        PluginOp::List {} => {
            let apt = Command::new("apt")
                .args(vec!["--manual-installed", "list"])
                .stdout(Stdio::piped()) // To pipe apt.stdout into awk.stdin
                .spawn()
                .map_err(|err| InternalError::exec_error("apt", err))?;

            // apt output    = openssl/focal-security,now 1.1.1f-1ubuntu2.3 amd64 [installed]
            // awk -F '[/ ]' =   $1   ^       $2         ^   $3            ^   $4
            // awk print     =   name ^                  ^   version       ^
            let status = Command::new("awk")
                .args(vec![
                    "-F",
                    "[/ ]",
                    r#"{if ($1 != "Listing...") { print ""$1"\t"$3""}}"#,
                ])
                .stdin(apt.stdout.unwrap()) // Cannot panic: apt.stdout has been set
                .status()
                .map_err(|err| InternalError::exec_error("awk", err))?;

            status
        }

        PluginOp::Install {
            module,
            version,
            file_path,
        } => {
            match (&version, &file_path) {
                (None, None) => {
                    // normal install
                    run_cmd("apt-get", &format!("install --quiet --yes {}", module))?
                }

                (Some(version), None) => run_cmd(
                    "apt-get",
                    &format!("install --quiet --yes {}={}", module, version),
                )?,

                (None, Some(file_path)) => {
                    let mut package = PackageMetadata::try_new(file_path)?;
                    let () = package
                        .validate_package(&[&format!("Package: {}", &module), "Debian package"])?;

                    run_cmd(
                        "apt-get",
                        &format!("install --quiet --yes {}", package.file_path().display()),
                    )?
                }

                (Some(version), Some(file_path)) => {
                    let mut package = PackageMetadata::try_new(file_path)?;
                    let () = package.validate_package(&[
                        &format!("Version: {}", &version),
                        &format!("Package: {}", &module),
                        "Debian package",
                    ])?;

                    run_cmd(
                        "apt-get",
                        &format!("install --quiet --yes {}", package.file_path().display()),
                    )?
                }
            }
        }

        PluginOp::Remove { module, version } => {
            if let Some(version) = version {
                // check the version mentioned present or not
                run_cmd(
                    "apt-get",
                    &format!("remove --quiet --yes {}={}", module, version),
                )?
            } else {
                run_cmd("apt-get", &format!("remove --quiet --yes {}", module))?
            }
        }

        PluginOp::Prepare => run_cmd("apt-get", "update --quiet --yes")?,

        PluginOp::Finalize => run_cmd("apt-get", "auto-remove --quiet --yes")?,
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
    let apt = AptCli::from_args();

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
