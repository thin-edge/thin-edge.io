#![forbid(unsafe_code)]
#![deny(clippy::mem_forget)]

use anyhow::Context;
use structopt::StructOpt;

mod certificate;
mod cli;
mod command;
mod config;
mod mqtt;
mod privsep;
mod services;
mod utils;

use command::BuildCommand;
use command::ExecutionContext;

fn main() -> anyhow::Result<()> {
    let mut privileged_executor: Box<dyn privsep::PrivilegedCommandExecutor> =
        if users::get_current_uid() == 0 {
            Box::new(privsep::PrivilegeSeparatedCommandExecutor::new(
                "/home/mneumann/tedge_priv",
            ))
        } else {
            Box::new(privsep::UnprivilegedDummyCommandExecutor::new())
        };

    // from now on running as unprivileged user
    assert!(users::get_current_uid() != 0);

    // Use this code deep inside the Commands to execute a privileged command.
    privileged_executor
        .execute(privsep::PrivilegedCommand::Command1)
        .expect("Command succeeded");

    // ...

    let context = ExecutionContext::new(privileged_executor);

    // let _user_guard = context.user_manager.become_user(utils::users::TEDGE_USER)?;

    let opt = cli::Opt::from_args();

    let config = config::TEdgeConfig::from_default_config()
        .with_context(|| "failed to read the tedge configuration")?;

    let cmd = opt
        .tedge
        .build_command(config)
        .with_context(|| "missing configuration parameter")?;

    cmd.execute(&context)
        .with_context(|| format!("failed to {}", cmd.description()))
}
