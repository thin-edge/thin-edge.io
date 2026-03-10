use camino::Utf8Path;
use camino::Utf8PathBuf;
use std::fs;
use tracing::debug;
use tracing::info;
use tracing::warn;

use crate::error::io_error;
use crate::error::FlowsPluginError;
use crate::get_flow_version;
use crate::FlowRecord;
use crate::PARAMS_FILE;

/// Installs a flow archive safely by unpacking to a temporary directory first:
/// 1. Unpacks archive to temporary directory
/// 2. Copy the existing params.toml from destination to temporary directory if it exists
/// 3. Atomically moves temporary directory including params.toml to destination (replaces old files but keeps params.toml)
pub fn install_flow(
    config_dir: impl AsRef<Utf8Path>,
    mappers_dir: impl AsRef<Utf8Path>,
    flow_record: &FlowRecord,
    version: Option<String>,
    file_path: &str,
    validate: bool,
) -> Result<(), FlowsPluginError> {
    info!(
        "Installing flow {flow_record} from archive {file_path} with version {version}",
        version = version.as_deref().unwrap_or("unspecified")
    );

    let mapper_dir = flow_record.mapper_dir(&mappers_dir);
    let target = fs::File::open(file_path).map_err(|e| io_error(file_path, e))?;
    let dest = flow_record.flow_dir(&mappers_dir);
    let params_path = dest.join(PARAMS_FILE);

    // Create a temporary directory in mapper dir (e.g. /etc/tedge/mappers/local) so that inotify doesn't trigger on the temp dir creation but only on the final move.
    let tmp_dir = tempfile::tempdir_in(&mapper_dir).map_err(|e| io_error(&mapper_dir, e))?;
    let tmp_dest =
        Utf8PathBuf::try_from(tmp_dir.path().to_owned()).expect("tempdir path is valid UTF-8");
    let params_backup = tmp_dest.join(PARAMS_FILE);

    unpack_archive(target, file_path, &tmp_dest)?;
    if validate {
        validate_flow(config_dir.as_ref(), &tmp_dest)?;
    }

    if params_path.exists() {
        info!("Copying existing params.toml from {params_path} to temporary directory {tmp_dest}");
        fs::copy(&params_path, &params_backup).map_err(|e| io_error(&params_path, e))?;
    }

    info!("Ensuring destination directory {dest} exists for flow {flow_record}");
    let dest_parent = dest.parent().expect("dest always has a parent");
    fs::create_dir_all(dest_parent).map_err(|e| io_error(dest_parent, e))?;

    match fs::remove_dir_all(&dest) {
        Ok(_) => info!("Removed existing flow directory at {dest}"),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            debug!("No existing flow directory at {dest}")
        }
        Err(e) => return Err(io_error(&dest, e)),
    }

    info!("Moving unpacked flow from temporary directory {tmp_dest} to destination {dest}");
    fs::rename(&tmp_dest, &dest).map_err(|e| io_error(&dest, e))?;

    if let Some(version) = version {
        let version_from_file = get_flow_version(dest.join("flow.toml"));
        if version != version_from_file {
            warn!(
                "Provided version {version} does not match version {version_from_file} in flow.toml for flow {flow_record}."
            );
        }
    }

    info!("Successfully installed flow {flow_record} from archive {file_path}");

    Ok(())
}

fn validate_flow(config_dir: &Utf8Path, tmp_dest: &Utf8Path) -> Result<(), FlowsPluginError> {
    info!("Validating flow at {tmp_dest} by running `tedge flows list --flows-dir {tmp_dest} --config-dir {config_dir}`");
    let output = match std::process::Command::new("tedge")
        .args(["flows", "list", "--flows-dir"])
        .arg(tmp_dest.as_str())
        .arg("--config-dir")
        .arg(config_dir.as_str())
        .output()
    {
        Ok(output) => output,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            warn!("Skipping flow validation: `tedge` binary not found");
            return Ok(());
        }
        Err(e) => return Err(io_error(tmp_dest, e)),
    };

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
        return Err(FlowsPluginError::InvalidFlow { stderr });
    }

    info!("Flow at {tmp_dest} passed validation");
    Ok(())
}

fn unpack_archive(
    target: fs::File,
    file_path: &str,
    dest: impl AsRef<Utf8Path>,
) -> Result<(), FlowsPluginError> {
    let dest = dest.as_ref();
    if file_path.ends_with(".tar.gz") || file_path.ends_with(".tgz") {
        info!("Unpacking .tar.gz archive from {file_path} to {dest}");
        unpack_tar_gz(target, dest)
    } else if file_path.ends_with(".tar") {
        info!("Unpacking .tar archive from {file_path} to {dest}");
        unpack_tar(target, dest)
    } else {
        info!("Guessing archive format for {file_path}, trying .tar.gz then .tar");
        let cloned = target.try_clone().map_err(|e| io_error(file_path, e))?;
        if unpack_tar_gz(cloned, dest).is_ok() {
            return Ok(());
        }
        info!("Could not unpack as .tar.gz. Attempting to unpack as .tar");
        unpack_tar(target, dest)
            .map_err(|_| FlowsPluginError::UnsupportedFormat(file_path.to_owned()))
    }
}

fn unpack_tar_gz(target: fs::File, dest: impl AsRef<Utf8Path>) -> Result<(), FlowsPluginError> {
    unpack_tar(flate2::read::GzDecoder::new(target), dest)
}

fn unpack_tar(
    reader: impl std::io::Read,
    dest: impl AsRef<Utf8Path>,
) -> Result<(), FlowsPluginError> {
    let mut archive = tar::Archive::new(reader);
    archive
        .unpack(dest.as_ref())
        .map_err(|e| FlowsPluginError::UnpackError {
            path: dest.as_ref().to_path_buf(),
            error: e,
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use camino::Utf8PathBuf;
    use tedge_test_utils::fs::TempTedgeDir;

    #[test]
    fn install_new_flow() {
        let ttd = TempTedgeDir::new();
        let mappers_dir = ttd.dir("mappers");
        mappers_dir.dir("local").dir("flows");
        let tarball_path = create_test_tarball(&ttd, "tar.gz");
        let flow_record = FlowRecord::new("local/hello").unwrap();
        let flow_dir = flow_record.flow_dir(mappers_dir.utf8_path());

        install_flow(
            ttd.utf8_path(),
            mappers_dir.utf8_path(),
            &flow_record,
            None,
            tarball_path.as_str(),
            false,
        )
        .unwrap();

        assert!(flow_dir.join("main.js").exists());
        assert!(flow_dir.join("flow.toml").exists());
        assert!(flow_dir.join("params.toml.template").exists());
    }

    #[test]
    fn install_new_nested_flow() {
        let ttd = TempTedgeDir::new();
        let mappers_dir = ttd.dir("mappers");
        mappers_dir.dir("local").dir("flows");
        let tarball_path = create_test_tarball(&ttd, "tar.gz");
        let flow_record = FlowRecord::new("local/hello/world").unwrap();
        let flow_dir = flow_record.flow_dir(mappers_dir.utf8_path());

        install_flow(
            ttd.utf8_path(),
            mappers_dir.utf8_path(),
            &flow_record,
            None,
            tarball_path.as_str(),
            false,
        )
        .unwrap();

        assert!(flow_dir.join("main.js").exists());
        assert!(flow_dir.join("flow.toml").exists());
        assert!(flow_dir.join("params.toml.template").exists());
    }

    #[test]
    fn install_from_tar_archive() {
        let ttd = TempTedgeDir::new();
        let mappers_dir = ttd.dir("mappers");
        mappers_dir.dir("local").dir("flows");
        let flow_record = FlowRecord::new("local/hello").unwrap();
        let flow_dir = flow_record.flow_dir(mappers_dir.utf8_path());

        install_flow(
            ttd.utf8_path(),
            mappers_dir.utf8_path(),
            &flow_record,
            None,
            create_test_tarball(&ttd, "tar").as_str(),
            false,
        )
        .unwrap();

        assert!(flow_dir.join("main.js").exists());
        assert!(flow_dir.join("flow.toml").exists());
        assert!(flow_dir.join("params.toml.template").exists());
    }

    #[test]
    fn override_existing_flow() {
        let ttd = TempTedgeDir::new();
        let mappers_dir = ttd.dir("mappers");
        let flow_record = FlowRecord::new("local/hello").unwrap();
        let flow_dir = flow_record.flow_dir(mappers_dir.utf8_path());
        let tarball_path = create_test_tarball(&ttd, "tar.gz");

        // Pre-existing flow at version 0.1.0
        mappers_dir
            .dir("local")
            .dir("flows")
            .dir("hello")
            .file("flow.toml")
            .with_toml_content(toml::toml! { version = "0.1.0" });
        assert_eq!(get_flow_version(flow_dir.join("flow.toml")), "0.1.0");

        install_flow(
            ttd.utf8_path(),
            mappers_dir.utf8_path(),
            &flow_record,
            None,
            tarball_path.as_str(),
            false,
        )
        .unwrap();

        assert_eq!(get_flow_version(flow_dir.join("flow.toml")), "1.0.0");
        assert!(flow_dir.join("main.js").exists());
        assert!(flow_dir.join("params.toml.template").exists());
    }

    #[test]
    fn retain_params_on_flow_reinstall() {
        let ttd = TempTedgeDir::new();
        let mappers_dir = ttd.dir("mappers");
        let flow_record = FlowRecord::new("local/hello").unwrap();
        let flow_dir = flow_record.flow_dir(mappers_dir.utf8_path());
        let tarball_path = create_test_tarball(&ttd, "tar.gz");

        // Pre-existing flow with a custom params.toml
        let hello = mappers_dir.dir("local").dir("flows").dir("hello");
        hello.file("flow.toml");
        hello
            .file("params.toml")
            .with_toml_content(toml::toml! { param1 = "value1" });

        install_flow(
            ttd.utf8_path(),
            mappers_dir.utf8_path(),
            &flow_record,
            None,
            tarball_path.as_str(),
            false,
        )
        .unwrap();

        // params.toml must be kept
        assert!(flow_dir.join("params.toml").exists());
        let content = fs::read_to_string(flow_dir.join("params.toml")).unwrap();
        let parsed: toml::Value = toml::from_str(&content).unwrap();
        assert_eq!(parsed["param1"].as_str().unwrap(), "value1");

        // other files must be updated
        assert_eq!(get_flow_version(flow_dir.join("flow.toml")), "1.0.0");
        assert!(flow_dir.join("main.js").exists());
        assert!(flow_dir.join("params.toml.template").exists());
    }

    /// Create a test tarball containing `main.js`, `flow.toml` (version 1.0.0),
    /// and `params.toml.template`. `format` is either `"tar.gz"` or `"tar"`.
    fn create_test_tarball(ttd: &TempTedgeDir, format: &str) -> Utf8PathBuf {
        let work_dir = ttd.dir("work");
        work_dir.file("main.js");
        work_dir
            .file("flow.toml")
            .with_toml_content(toml::toml! { version = "1.0.0" });
        work_dir.file("params.toml.template");

        let tarball_path = ttd.utf8_path().join(format!("hello.{format}"));
        let tar_file = fs::File::create(&tarball_path).unwrap();

        match format {
            "tar.gz" => {
                let enc = flate2::write::GzEncoder::new(tar_file, flate2::Compression::default());
                let mut archive = tar::Builder::new(enc);
                append_work_dir(&work_dir, &mut archive);
                archive.finish().unwrap();
            }
            _ => {
                let mut archive = tar::Builder::new(tar_file);
                append_work_dir(&work_dir, &mut archive);
                archive.finish().unwrap();
            }
        }

        tarball_path
    }

    fn append_work_dir<W: std::io::Write>(work_dir: &TempTedgeDir, archive: &mut tar::Builder<W>) {
        for entry in fs::read_dir(work_dir.utf8_path()).unwrap() {
            let entry = entry.unwrap();
            let path = entry.path();
            if path.is_file() {
                let mut file = fs::File::open(&path).unwrap();
                archive.append_file(entry.file_name(), &mut file).unwrap();
            }
        }
    }
}
