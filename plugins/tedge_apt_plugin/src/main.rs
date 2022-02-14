mod error;
mod module_check;

use crate::error::InternalError;
use crate::module_check::PackageMetadata;
use serde::Deserialize;
use std::io::{self};
use std::process::{Command, ExitStatus, Stdio};
use structopt::StructOpt;
use rumqttc::{Event, Incoming, Outgoing, Packet};

const DEFAULT_QUEUE_CAPACITY: usize = 10;

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

fn run(operation: PluginOp, module_type: &str) -> Result<ExitStatus, InternalError> {
    let status = match operation {
        PluginOp::List {} => {
            let apt = Command::new("apt")
                .args(vec!["--manual-installed", "list"])
                .stdout(Stdio::piped()) // To pipe apt.stdout into awk.stdin
                .spawn()
                .map_err(|err| InternalError::exec_error("apt", err))?;

            let linefilter = if module_type == "tedge" {
                // all packages starting with "tedge"
                r#"{if ($1 != "Listing..." && $1  ~ /^tedge.*/) { print $1"\t"$3 }}"#
            } else {
                // all packages not starting with "tedge"
                r#"{if ($1 != "Listing..." && $1 !~ /^tedge.*/) { print $1"\t"$3 }}"#
            };

            // apt output    = openssl/focal-security,now 1.1.1f-1ubuntu2.3 amd64 [installed]
            // awk -F '[/ ]' =   $1   ^       $2         ^   $3            ^   $4
            // awk print     =   name ^                  ^   version       ^
            let status = Command::new("awk")
                .args(vec![
                    "-F",
                    "[/ ]",
                    linefilter,
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
            let (installer, _metadata) = get_installer(module, version, file_path)?;
            run_cmd("apt-get", &format!("install --quiet --yes {}", installer))?
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
            args.push("install".into());
            args.push("--quiet".into());
            args.push("--yes".into());
            for update_module in updates {
                match update_module.action {
                    UpdateAction::Install => {
                        let (installer, metadata) = get_installer(
                            update_module.name,
                            update_module.version,
                            update_module.path,
                        )?;
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

            println!("apt-get install args: {:?}", args);
            let status = Command::new("apt-get")
                .args(args)
                .stdin(Stdio::null())
                .status()
                .map_err(|err| InternalError::exec_error("apt-get", err))?;

            return Ok(status);
        }

        PluginOp::Prepare => run_cmd("apt-get", "update --quiet --yes")?,

        PluginOp::Finalize => run_cmd("apt-get", "auto-remove --quiet --yes")?,
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
            let () =
                package.validate_package(&[&format!("Package: {}", &module), "Debian package"])?;

            Ok((format!("{}", package.file_path().display()), Some(package)))
        }

        (Some(version), Some(file_path)) => {
            let mut package = PackageMetadata::try_new(file_path)?;
            let () = package.validate_package(&[
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
                return Err(InternalError::VersionMismatch {
                    package: module_name.into(),
                    installed: installed_version.into(),
                    expected: module_version.into(),
                });
            }
        }
    }

    Ok(())
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

    // That plugin can be called via softlinks. The module type the plugin serves is
    // derived from the exec name with which it was called (i.E. a softlink name the original filename).
    // NOTE: In case module type can not be derived, the plugin shall pani. Thats why unwrap() is used intentionally.
    let args : Vec<String> = std::env::args().collect();
    let module_type = std::path::Path::new(& args[0]).file_name().unwrap().to_os_string().into_string().unwrap();
    let exit_code;

    match run(apt.operation, &module_type) {
        Ok(status) if status.success() => {
            exit_code = 0;
        }

        Ok(status) => {
            if status.code().is_some() {
                exit_code= 2;
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

    if module_type == "tedge" {

        let mut publish_succeeded = false;

        let mqtt_port = 1883; // TODO: read from tedge-config

        // loop until message has arrived at the broker
        while !publish_succeeded {

            // Using here runmqtt directly instead of tedge's mqtt_channel class, to have access
            // to runmqtt's eventloop for feedback (PubComp). That is to wait with plugin's process-exit until
            // published message has really arrived at the broker.
            let mut options = rumqttc::MqttOptions::new(String::from("sm plugin-apt"), String::from("localhost"), mqtt_port);
            options.set_clean_session(true);
            let (mut client, mut connection) = rumqttc::Client::new(options, DEFAULT_QUEUE_CAPACITY);
            _ = client.publish(String::from("tedge/plugins/software/tedge"), rumqttc::QoS::ExactlyOnce, true, exit_code.to_string());

            for event in connection.iter() {
                match event {

                    Ok(Event::Incoming(Packet::PubComp(_))) => {
                      publish_succeeded = true;
                      break;
                    }

                    // TODO: Crosscheck that all fail cases are covered!
                    //       Check for a more safe way to really assure not to run in endless loop?
                    Ok(Event::Outgoing(Outgoing::Disconnect))
                    | Ok(Event::Incoming(Incoming::Disconnect))
                    | Err(_) => {
                      // Sending pub to broker failed. Wait some time to not overload the CPU and loopback network with MQTT pub retries.
                      std::thread::sleep(std::time::Duration::from_secs(5));
                      break;
                    }
                    _ => { }
                }
            }
        }
    }

    std::process::exit(exit_code);
}

