use crate::error::io_error;
use crate::error::FlowsPluginError;
use crate::FlowRecord;
use crate::PARAMS_FILE;

use camino::Utf8Path;
use std::fs;
use tracing::info;

/// Remove a flow given its module name `<mapper>/<flow-name>`.
///
/// Resolution order:
/// 1. If `{mappers_dir}/{mapper}/flows/{flow_name}` directory exists -> remove it.
///    When `keep_params` is true and a `params.toml` is present, only the other
///    files are deleted (the directory and `params.toml` are kept).
/// 2. Otherwise fall back to removing `{mappers_dir}/{mapper}/flows/{flow_name}.toml`.
pub fn remove_flow(
    mappers_dir: impl AsRef<Utf8Path>,
    flow_record: &FlowRecord,
    _version: Option<String>,
    keep_params: bool,
) -> Result<(), FlowsPluginError> {
    let flow_dir = flow_record.flow_dir(&mappers_dir);
    let flow_toml = flow_record.flow_toml(&mappers_dir);

    if flow_dir.is_dir() {
        remove_flow_dir(&flow_dir, flow_record, keep_params)?;
    } else if flow_toml.is_file() {
        info!("Removing flow file {flow_toml}");
        fs::remove_file(&flow_toml).map_err(|e| io_error(&flow_toml, e))?;
        info!("Successfully removed flow {flow_record}");
    } else {
        info!("Flow {flow_record} does not exist; nothing to remove");
    }

    Ok(())
}

fn remove_flow_dir(
    flow_dir: &Utf8Path,
    flow_record: &FlowRecord,
    keep_params: bool,
) -> Result<(), FlowsPluginError> {
    info!("Removing flow directory {flow_dir}");

    let params_path = flow_dir.join(PARAMS_FILE);
    if params_path.exists() && keep_params {
        info!(
            "params.toml found in {flow_dir} and 'flows.params.keep_on_delete' is true; \
             removing files but keeping params.toml"
        );
        for entry in fs::read_dir(flow_dir).map_err(|e| io_error(flow_dir, e))? {
            let path = entry.map_err(|e| io_error(flow_dir, e))?.path();
            let Some(path) = Utf8Path::from_path(&path) else {
                continue;
            };
            if path == params_path {
                continue;
            }
            if path.is_dir() {
                fs::remove_dir_all(path).map_err(|e| io_error(path, e))?;
            } else {
                fs::remove_file(path).map_err(|e| io_error(path, e))?;
            }
        }
        info!("Successfully removed flow {flow_record}, params.toml retained");
    } else {
        fs::remove_dir_all(flow_dir).map_err(|e| io_error(flow_dir, e))?;
        info!("Successfully removed flow {flow_record}");
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tedge_test_utils::fs::TempTedgeDir;

    #[test]
    fn remove_directory_based_flow() {
        let ttd = TempTedgeDir::new();
        let mappers_dir = ttd.dir("mappers");
        let hello = mappers_dir.dir("local").dir("flows").dir("hello");
        hello.file("flow.toml");
        hello.file("main.js");
        let flow_record = FlowRecord::new("local/hello").unwrap();

        remove_flow(mappers_dir.utf8_path(), &flow_record, None, false).unwrap();
        assert!(!flow_record.flow_dir(mappers_dir.utf8_path()).exists());
    }

    #[test]
    fn remove_nested_directory_based_flow() {
        let ttd = TempTedgeDir::new();
        let mappers_dir = ttd.dir("mappers");
        let world = mappers_dir
            .dir("local")
            .dir("flows")
            .dir("hello")
            .dir("world");
        world.file("flow.toml");
        world.file("main.js");
        let flow_record = FlowRecord::new("local/hello/world").unwrap();

        remove_flow(mappers_dir.utf8_path(), &flow_record, None, false).unwrap();
        assert!(!flow_record.flow_dir(mappers_dir.utf8_path()).exists());
    }

    #[test]
    fn remove_file_based_flow() {
        // When no directory exists, remove the .toml file instead.
        let ttd = TempTedgeDir::new();
        let mappers_dir = ttd.dir("mappers");
        mappers_dir
            .dir("local")
            .dir("flows")
            .file("hello.toml")
            .with_toml_content(toml::toml! { version = "1.0" });
        let flow_record = FlowRecord::new("local/hello").unwrap();

        remove_flow(mappers_dir.utf8_path(), &flow_record, None, false).unwrap();
        assert!(!flow_record.flow_toml(mappers_dir.utf8_path()).exists());
    }

    #[test]
    fn directory_takes_priority_over_toml_file_on_name_collision() {
        // Both `hello/flow.toml` (directory-based) and `hello.toml` (file-based) exist.
        // Remove should delete only the directory and leave `hello.toml` untouched.
        let ttd = TempTedgeDir::new();
        let mappers_dir = ttd.dir("mappers");
        let flows = mappers_dir.dir("local").dir("flows");
        let hello_dir = flows.dir("hello");
        hello_dir.file("flow.toml");
        hello_dir.file("main.js");
        flows
            .file("hello.toml")
            .with_toml_content(toml::toml! { version = "1.0" });
        let flow_record = FlowRecord::new("local/hello").unwrap();

        remove_flow(mappers_dir.utf8_path(), &flow_record, None, false).unwrap();

        assert!(
            !flow_record.flow_dir(mappers_dir.utf8_path()).exists(),
            "directory should be removed"
        );
        assert!(
            flow_record.flow_toml(mappers_dir.utf8_path()).exists(),
            "hello.toml should be untouched"
        );
    }

    #[test]
    fn remove_nonexistent_flow_is_noop() {
        let ttd = TempTedgeDir::new();
        let mappers_dir = ttd.dir("mappers");

        // Neither a directory nor a .toml file — should succeed silently.
        let flow_record = FlowRecord::new("local/hello").unwrap();
        remove_flow(mappers_dir.utf8_path(), &flow_record, None, false).unwrap();
    }

    #[test]
    fn remove_directory_flow_keeps_params_toml() {
        let ttd = TempTedgeDir::new();
        let mappers_dir = ttd.dir("mappers");
        let hello = mappers_dir.dir("local").dir("flows").dir("hello");
        hello.file("flow.toml");
        hello.file("main.js");
        hello
            .file("params.toml")
            .with_toml_content(toml::toml! { x = 1 });
        hello.file("params.toml.template");
        let flow_record = FlowRecord::new("local/hello").unwrap();

        remove_flow(mappers_dir.utf8_path(), &flow_record, None, true).unwrap();

        let dir = flow_record.flow_dir(mappers_dir.utf8_path());
        assert!(dir.exists(), "directory should be kept");
        assert!(
            dir.join("params.toml").exists(),
            "params.toml should be kept"
        );
        assert!(!dir.join("main.js").exists());
        assert!(!dir.join("flow.toml").exists());
        assert!(!dir.join("params.toml.template").exists());
    }
}
