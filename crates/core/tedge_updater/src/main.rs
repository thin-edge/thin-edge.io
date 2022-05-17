use clap::Parser;
use logged_command::LoggedCommand;
use std::{
    fs::File,
    io::BufWriter,
    path::Path,
    process::{Command, Output, Stdio},
};

use tedge_config::DEFAULT_TEDGE_CONFIG_PATH;

mod errors;

#[derive(Parser)]
struct AptCli {
    #[clap(subcommand)]
    operation: PluginOp,
}

#[derive(clap::Subcommand)]
pub enum PluginOp {
    /// Install or remove multiple modules at once
    UpdateList {
        #[clap(long = "--plugin-name")]
        plugin_name: String,
    },
}

fn exit_with_error<T>(code: i32, message: &str) -> T {
    eprintln!("{}", message);
    std::process::exit(code); // no return from that call
}

fn run(operation: PluginOp) -> Result<i32, anyhow::Error> {
    match operation {
        PluginOp::UpdateList { plugin_name } => {
            
            // TODO: add error handling
            // TODO: consider --config-dir option
            run_hook("/etc/tedge/hooks/hook-stop-and-snapshot", "");

            let mut failed = false;

            // NOTE: Here is point-of-no-return. SM Agent was stopped

            let exitcode = if failed {
                // disconnect has completely or partially failed, so avoid to start update,
                // instead report error to agent
                eprintln!("Error on disconnect. At least one cloud could not be disconnected, so skipping update.");
                2
            } else {
                // exec plugin and forward STDIN
                println!("Executing: {} update-list", plugin_name);

                let log_file_path = "/var/log/tedge/updater.log";
                let log_file = File::create(log_file_path)?;
                let mut logger = BufWriter::new(log_file);
                let status = LoggedCommand::new(plugin_name)
                    .arg("update-list")
                    .execute(&mut logger);

                status_to_exitcode(&status)
            };

            match store_exitcode(exitcode) {
                Err(e) => eprintln!(
                    "Error writing exit-code '{}' to '/var/run/tedge_update/update_result': {}",
                    exitcode, e
                ),
                Ok(_) => println!(
                    "Stored exit-code '{}' to '/var/run/tedge_update/update_result'",
                    exitcode
                ),
            };

            // TODO: add error handling
            // TODO: consider --config-dir option
            run_hook("/etc/tedge/hooks/hoo-start-or-rollback", exitcode.to_string().as_str());

            Ok(exitcode)
        }
    }
}

fn run_hook(hook_path: &str, args: &str) -> std::result::Result<i32, String> {
    let args: Vec<&str> = args.split_whitespace().collect();

    println!("Executing hook '{}'", hook_path);

    // TODO: add error handling
    let status = Command::new("sudo")
        .arg(hook_path)
        .args(args)
        .stdin(Stdio::null())
        .status();
    Ok(0)
}

fn run_tedge_cli(args: &str) -> std::result::Result<i32, String> {
    let args: Vec<&str> = args.split_whitespace().collect();

    let status = Command::new("sudo")
        .arg("tedge")
        .args(args)
        .stdin(Stdio::null())
        .status();

    match status {
        Ok(s) => match s.code() {
            Some(code) => {
                if code == 0 {
                    Ok(0)
                } else {
                    Err(format!("tedge exited with code: {}", code))
                }
            }
            None => Err("tedge terminated by a signal".to_string()),
        },

        Err(_) => Err("Could not exec tedge".to_string()),
    }
}

fn store_exitcode(exit_code: i32) -> std::io::Result<()> {
    let filepath = "/var/run/tedge_update";
    let filename = "update_result";

    std::fs::create_dir_all(filepath)?;
    std::fs::write(format!("{}/{}", filepath, filename), exit_code.to_string())?;
    Ok(())
}

fn status_to_exitcode(status: &Result<Output, std::io::Error>) -> i32 {
    let exit_code;

    match status {
        Ok(status) if status.status.success() => {
            exit_code = 0;
        }

        Ok(status) => {
            if status.status.code().is_some() {
                exit_code = status.status.code().unwrap();
            } else {
                eprintln!("Interrupted by a signal!");
                exit_code = 4;
            }
        }

        Err(err) => {
            eprintln!("ERROR: {}", err);
            exit_code = 5;
        }
    }
    exit_code
}

fn main() {
    let apt = AptCli::parse();

    let exitcode = run(apt.operation).unwrap();
    std::process::exit(exitcode);
}
