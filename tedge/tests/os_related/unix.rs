use std::os::linux::fs::MetadataExt;

fn command_as_root<I, S>(home_dir: &str, args: I) -> Result<std::process::Command, Box<dyn std::error::Error>>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<std::ffi::OsStr>,
{
    let sudo = which::which("sudo")?;
    let mut command = std::process::Command::new(sudo);
    command
        .env("HOME", home_dir)
        .args(args);

    Ok(command)
}

#[test]
#[cfg(feature="mosquitto-available")]
#[cfg(feature="root-access")]
fn create_certificate_as_root_should_switch_to_mosquitto() -> Result<(), Box<dyn std::error::Error>> {
    let tempdir = tempfile::tempdir()?;
    let device_id = "test";
    let cert_path = temp_path(&tempdir, "test-cert.pem");
    let key_path = temp_path(&tempdir, "test-key.pem");
    let home_dir = tempdir.path().to_str().unwrap();
    let tedge = env!("CARGO_BIN_EXE_tedge");

    let mut chown_home = command_as_root(
        home_dir,
        &["chown", "mosquitto:mosquitto", &home_dir],
    )?;
    let mut set_cert_path_cmd = command_as_root(
        home_dir,
        &[tedge, "config", "set", "device.cert.path", &cert_path],
    )?;
    let mut set_key_path_cmd = command_as_root(
        home_dir,
        &[tedge, "config", "set", "device.key.path", &key_path],
    )?;

    let mut create_cmd =
        command_as_root(home_dir, &[tedge, "cert", "create", "--device-id", device_id])?;

    // Run the commands to configure tedge
    assert!(chown_home.output()?.status.success());
    assert!(set_cert_path_cmd.output()?.status.success());
    assert!(set_key_path_cmd.output()?.status.success());

    // Create the certificate
    assert!(create_cmd.output()?.status.success());

    let cert_metadata = std::fs::metadata(cert_path)?;
    let key_metadata = std::fs::metadata(key_path)?;
    assert_eq!("mosquitto", users::get_user_by_uid(cert_metadata.st_uid()).unwrap().name());
    assert_eq!("mosquitto", users::get_group_by_gid(cert_metadata.st_gid()).unwrap().name());
    assert_eq!(0o444, extract_mode(cert_metadata.st_mode()));
    assert_eq!("mosquitto", users::get_user_by_uid(key_metadata.st_uid()).unwrap().name());
    assert_eq!("mosquitto", users::get_group_by_gid(key_metadata.st_gid()).unwrap().name());
    assert_eq!(0o400, extract_mode(key_metadata.st_mode()));

    Ok(())
}

fn extract_mode(st_type_and_mode: u32) -> u32 {
    st_type_and_mode % 0o1000
}

fn temp_path(dir: &tempfile::TempDir, filename: &str) -> String {
    String::from(dir.path().join(filename).to_str().unwrap())
}
