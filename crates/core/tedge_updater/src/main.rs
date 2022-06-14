use clap::Parser;
use std::{
    fs::File,
    io::{BufWriter, Write},
    path::{Path, PathBuf},
    process::{Command, ExitStatus, Stdio},
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
            let mut clouds: Vec<String> = Vec::new();
            let mut failed = false;

            let mut buffer = String::new();
            std::io::stdin().read_line(&mut buffer)?;

            let plugin_name_file = PathBuf::from(&plugin_name);
            let filename = plugin_name_file.file_name().unwrap().to_str().unwrap();

            let _ = run_hook(
                format!("/etc/tedge/hooks/hook-stop-and-snapshot-{filename}").as_str(),
                "",
            );

            // check for existing bridges
            // TODO: maybe better to have a tedge command as "tedge connected list", as here trying
            //       to interpret files in mosquitto conf folder.
            const TEDGE_BRIDGE_CONF_DIR_PATH: &str = "mosquitto-conf"; // NOTE: stolen from tedge/src/cli/connect/command.rs
            const TEDGE_BRIDGE_CONF_POSTFIX: &str = "-bridge.conf"; // TODO: definition is just made here, might be better part of tedge CLI.

            let config_location = DEFAULT_TEDGE_CONFIG_PATH; // NOTE: hardcoded here, maybe allow to set with option "--config-dir"

            let bridge_cfg_dir = Path::new(config_location).join(TEDGE_BRIDGE_CONF_DIR_PATH);
            let walker = globwalk::GlobWalkerBuilder::from_patterns(
                bridge_cfg_dir,
                &[format!("*{}", TEDGE_BRIDGE_CONF_POSTFIX)],
            )
            .max_depth(1)
            .build()
            .unwrap_or_else(|e: globwalk::GlobError| {
                exit_with_error(2, format!("Cannot build globwalk, error: '{}'", e).as_str())
            })
            .into_iter()
            .filter_map(Result::ok);

            for img in walker {
                let cloud_name = img
                    .path()
                    .file_name()
                    .unwrap_or_else(|| {
                        exit_with_error(2, "Cannot read bridge-config filename from folder.")
                    })
                    .to_str()
                    .unwrap_or_else(|| {
                        exit_with_error(2, "Cannot handle bridge-config filename as string.")
                    })
                    .strip_suffix(TEDGE_BRIDGE_CONF_POSTFIX)
                    .unwrap_or_else(|| {
                        exit_with_error(2, "Cannot strip suffix from bridge-config filename.")
                    });
                clouds.push(cloud_name.to_string());
                println!("Found cloud connection: {}", cloud_name);
            }

            // disconnect all clouds
            for cloud in clouds.clone().into_iter() {
                println!("Disconnecting {}", cloud);
                match run_tedge_cli(&format!("disconnect {}", cloud)) {
                    Ok(_) => (),
                    Err(e) => {
                        eprintln!("Error occurred when calling tedge CLI: {}", e);
                        failed = true; // one cloud failed to disconnect, so fail entire update
                    }
                }
            }
            // NOTE: Here is point-of-no-return. SM Agent was stopped

            let exitcode = if failed {
                // disconnect has completely or partially failed, so avoid to start update,
                // instead report error to agent
                eprintln!("Error on disconnect. At least one cloud could not be disconnected, so skipping update.");
                2
            } else {
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

                status_to_exitcode(status)
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

            let _ = run_hook(
                format!("/etc/tedge/hooks/hook-start-or-rollback-{plugin_name}").as_str(),
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
