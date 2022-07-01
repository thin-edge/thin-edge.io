use clap::Parser;
use std::{
    fs::File,
    io::{BufWriter, Write},
    path::{Path, PathBuf},
    process::{Command, ExitStatus, Stdio},
};
use std::fs::OpenOptions;
use std::io::prelude::*;

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
            let mut failed = false;

            let mut buffer = String::new();
            std::io::stdin().read_line(&mut buffer)?;

            let plugin_name_file = PathBuf::from(&plugin_name);
            let filename = plugin_name_file.file_name().unwrap().to_str().unwrap();

            let _ = run_hook(
                format!("/etc/tedge/hooks/hook-stop-and-snapshot-{filename}").as_str(),
                "",
            );

            // exec plugin and forward STDIN
            println!("Executing: {} update-list", plugin_name);

            let mut status = Command::new(format!("/etc/tedge/sm-plugins/{filename}"))
                    .args(vec!["update-list"])
                    .stdin(std::process::Stdio::piped())
                    .spawn()?;

            dbg!(&status);
            let cmd_stdin = status.stdin.as_mut().unwrap();
            let _ = cmd_stdin.write_all(buffer.as_bytes());
            let status = status.wait();

            let exitcode = status_to_exitcode(status);

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

            let _ = run_hook(
                format!("/etc/tedge/hooks/hook-start-or-rollback-{filename}").as_str(),
                exitcode.to_string().as_str(),
            );

            Ok(exitcode)
        }
    }
}

fn run_hook(hook_path: &str, args: &str) -> std::result::Result<ExitStatus, std::io::Error> {
    let args: Vec<&str> = args.split_whitespace().collect();

    println!("Executing hook '{}'", hook_path);

    Command::new("sudo")
        .arg(hook_path)
        .args(args)
        .stdin(Stdio::null())
        .status()
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

fn status_to_exitcode(status: Result<ExitStatus, std::io::Error>) -> i32 {
    let exit_code;

    match status {
        Ok(status) if status.success() => {
            exit_code = 0;
        }

        Ok(status) => {
            if status.code().is_some() {
                exit_code = status.code().unwrap();
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
