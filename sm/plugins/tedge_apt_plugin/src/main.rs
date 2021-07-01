use std::process::{Command, Stdio};
use structopt::StructOpt;

#[derive(StructOpt)]
struct AptCli {
    #[structopt(subcommand)]
    operation: PluginOp,
}

#[derive(StructOpt)]
pub enum PluginOp {
    /// List all the installed modules
    List {},

    /// Install a module
    Install {
        module: String,
        version: Option<String>,
    },

    /// Uninstall a module
    Remove {
        module: String,
        version: Option<String>,
    },

    /// Prepare a sequences of install/remove commands
    Prepare,

    /// Finalize a sequences of install/remove commands
    Finalize,
}

fn run(operation: PluginOp) -> anyhow::Result<std::process::ExitStatus> {
    let status = match operation {
        PluginOp::List {} => {
            let cmd = Command::new("apt")
                .args(vec!["--installed", "list"])
                .stdout(Stdio::piped())
                .spawn()?;

            let cmd2 = Command::new("grep")
                .args(vec!["-v", "automatic"])
                .stdin(cmd.stdout.unwrap())
                .stdout(Stdio::piped())
                .spawn()?;

            let status = Command::new("awk")
                .arg(r#"{print "{\"name\":\""$1"\",\"version\":\""$2"\"}"}"#)
                .stdin(cmd2.stdout.unwrap())
                .status()?;
            status
        }

        PluginOp::Install { module, version } => {
            if let Some(version) = version {
                run_cmd(
                    "apt-get",
                    &format!("install --quiet --yes {}={}", module, version),
                )?
            } else {
                run_cmd("apt-get", &format!("install --quiet --yes {}", module))?
            }
        }

        PluginOp::Remove {
            module,
            version: _unused,
        } => run_cmd("apt-get", &format!("remove --quiet --yes {}", module))?,

        PluginOp::Prepare => run_cmd(
            "apt-get",
            &format!("update --quiet --yes"),
        )?,

        PluginOp::Finalize => run_cmd(
            "apt-get",
            &format!("auto-remove --quiet --yes"),
        )?,

    };

    Ok(status)
}

fn run_cmd(cmd: &str, args: &str) -> anyhow::Result<std::process::ExitStatus> {
    let args: Vec<&str> = args.split_whitespace().collect();
    let status = Command::new(cmd).args(args).stdin(Stdio::null()).status()?;
    Ok(status)
}

fn main() -> anyhow::Result<()> {
    let apt = AptCli::from_args();
    let status = run(apt.operation)?;
    if let Some(code) = status.code() {
        std::process::exit(code);
    } else {
        anyhow::bail!("Interrupted by a signal!");
    }
}
