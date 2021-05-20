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
    Uninstall {
        module: String,
        version: Option<String>,
    },

    /// Display the installed version of a module
    Version { module: String },
}

fn run(operation: PluginOp) -> anyhow::Result<std::process::ExitStatus> {
    let status = match operation {
        PluginOp::List {} => run_cmd(
            "dpkg-query",
            " --show --showformat='${{Package}}\\t${{Version}}\\n'",
        )?,

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

        PluginOp::Uninstall {
            module,
            version: _unused,
        } => run_cmd("apt-get", &format!("remove --quiet --yes {}", module))?,

        PluginOp::Version { module } => run_cmd(
            "dpkg-query",
            &format!(
                " --show --showformat='${{Package}}\\t${{Version}}\\n' {}",
                module
            ),
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
