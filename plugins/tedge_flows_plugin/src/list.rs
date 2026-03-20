use crate::get_flow_version;
use crate::FlowRecord;
use crate::PARAMS_FILE;

use camino::Utf8Path;
use glob::glob;
use tracing::info;
use tracing::warn;

#[derive(Debug)]
pub struct FlowEntry {
    pub flow_record: FlowRecord,
    pub version: String,
}

pub fn list_flows(mappers_dir: impl AsRef<Utf8Path>) {
    let flows = retrieve_flows(mappers_dir);
    for entry in flows {
        println!("{}\t{}", entry.flow_record, entry.version);
    }
}

pub fn retrieve_flows(mappers_dir: impl AsRef<Utf8Path>) -> Vec<FlowEntry> {
    let mut flows: Vec<FlowEntry> = Vec::new();

    let mappers_dir = mappers_dir.as_ref();
    info!("Listing flows from directory: {mappers_dir}");

    let pattern = format!("{mappers_dir}/*/flows");
    match glob(&pattern) {
        Ok(entries) => {
            for flows_dir in entries.filter_map(Result::ok) {
                let Some(path) = Utf8Path::from_path(flows_dir.as_path()).map(|p| p.to_path_buf())
                else {
                    warn!("Skipping non UTF8 path: {}", flows_dir.as_path().display());
                    continue;
                };
                // "flows" must be a directory
                if !path.is_dir() {
                    continue;
                }
                // The mapper name sits one level above the `flows` directory.
                let mapper_name = match path.parent().and_then(|p| p.file_name()) {
                    Some(name) => name,
                    None => continue,
                };
                info!("Listing mapper: '{mapper_name}'");
                list_mapper_flows(&path, mapper_name, &mut flows);
            }
        }
        Err(e) => warn!("Glob error for pattern {pattern}: {e}"),
    }

    // Deterministic output
    flows.sort_by(|a, b| {
        a.flow_record
            .mapper_name
            .cmp(&b.flow_record.mapper_name)
            .then(a.flow_record.flow_name.cmp(&b.flow_record.flow_name))
            .then(a.version.cmp(&b.version))
    });

    flows
}

/// Collect all flows from a mapper's flows directory using glob patterns.
///
/// Name rules:
/// - `flows_dir/flow.toml`              => name `flow`
/// - `flows_dir/hello.toml`             => name `hello`
/// - `flows_dir/hello/flow.toml`        => name `hello`
/// - `flows_dir/hello/world.toml`       => name `hello/world`
/// - `flows_dir/hello/world/flow.toml`  => name `hello/world`
/// - `flows_dir/hello/.toml`            => ignored (empty stem)
fn list_mapper_flows(flows_dir: &Utf8Path, mapper_name: &str, flows: &mut Vec<FlowEntry>) {
    let pattern = format!("{flows_dir}/**/*.toml");
    match glob(&pattern) {
        Ok(entries) => {
            for entry in entries.filter_map(Result::ok) {
                let Some(path) = Utf8Path::from_path(entry.as_path()).map(|p| p.to_path_buf())
                else {
                    warn!("Skipping non UTF8 path: {}", entry.as_path().display());
                    continue;
                };
                if let Some(entry) = create_flow_entry_from_toml_path(flows_dir, mapper_name, &path)
                {
                    flows.push(entry);
                }
            }
        }
        Err(e) => warn!("Glob error for pattern {pattern}: {e}"),
    }
}

fn create_flow_entry_from_toml_path(
    flows_dir: &Utf8Path,
    mapper_name: &str,
    path: &Utf8Path,
) -> Option<FlowEntry> {
    if !path.is_file() {
        return None;
    }
    let filename = path.file_name()?;
    if filename == PARAMS_FILE {
        return None;
    }

    let parent_dir = path
        .parent()
        .expect("glob pattern guarantees parent existence")
        .strip_prefix(flows_dir)
        .expect("glob pattern guarantees flows_dir existence");

    let flow_name = if filename == "flow.toml" {
        match parent_dir.as_str() {
            "" => "flow".to_string(),
            name => name.to_string(),
        }
    } else {
        let base_name = filename
            .strip_suffix(".toml")
            .expect("glob pattern guarantees .toml suffix");
        if base_name.is_empty() {
            warn!("Ignoring .toml file with empty stem at path: {}", path);
            return None;
        }
        parent_dir.join(base_name).into_string()
    };

    Some(FlowEntry {
        flow_record: FlowRecord {
            mapper_name: mapper_name.to_string(),
            flow_name,
        },
        version: get_flow_version(path),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use tedge_test_utils::fs::TempTedgeDir;

    #[test]
    fn missing_mappers_dir_returns_empty() {
        let ttd = TempTedgeDir::new();
        let non_existent = ttd.utf8_path().join("no-such-dir");
        let flows = retrieve_flows(non_existent);
        assert!(flows.is_empty());
    }

    #[test]
    fn flat_toml_file_is_listed() {
        // flows/hello.toml => name "hello"
        let ttd = TempTedgeDir::new();
        ttd.dir("local")
            .dir("flows")
            .file("hello.toml")
            .with_toml_content(toml::toml! { version = "1.0" });

        let flows = retrieve_flows(ttd.utf8_path());
        assert_eq!(entries(&flows), [("local", "hello", "1.0")]);
    }

    #[test]
    fn dir_with_flow_toml_is_listed() {
        // flows/hello/flow.toml => name "hello"
        let ttd = TempTedgeDir::new();
        ttd.dir("local")
            .dir("flows")
            .dir("hello")
            .file("flow.toml")
            .with_toml_content(toml::toml! { version = "1.0" });

        let flows = retrieve_flows(ttd.utf8_path());
        assert_eq!(entries(&flows), [("local", "hello", "1.0")]);
    }

    #[test]
    fn toml_in_subdirectory_is_listed() {
        // flows/hello/world.toml => name "hello/world"
        let ttd = TempTedgeDir::new();
        ttd.dir("local")
            .dir("flows")
            .dir("hello")
            .file("world.toml")
            .with_toml_content(toml::toml! { version = "1.0" });

        let flows = retrieve_flows(ttd.utf8_path());
        assert_eq!(entries(&flows), [("local", "hello/world", "1.0")]);
    }

    #[test]
    fn deeply_nested_flow_toml_is_listed() {
        // flows/hello/world/flow.toml => name "hello/world"
        let ttd = TempTedgeDir::new();
        ttd.dir("local")
            .dir("flows")
            .dir("hello")
            .dir("world")
            .file("flow.toml")
            .with_toml_content(toml::toml! { version = "1.0" });

        let flows = retrieve_flows(ttd.utf8_path());
        assert_eq!(entries(&flows), [("local", "hello/world", "1.0")]);
    }

    #[test]
    fn flow_toml_as_directory_is_ignored() {
        // If flow.toml is a directory rather than a file, the glob still
        // matches it, but it must be skipped.
        let ttd = TempTedgeDir::new();
        let flows_dir = ttd.dir("local").dir("flows");
        flows_dir.dir("hello").dir("flow.toml"); // directory, not a file
        flows_dir
            .file("world.toml")
            .with_toml_content(toml::toml! { version = "1.0" });

        let flows = retrieve_flows(ttd.utf8_path());
        assert_eq!(entries(&flows), [("local", "world", "1.0")]);
    }

    #[test]
    fn named_toml_as_directory_is_ignored() {
        // Same check for Pass 2: a directory named hello.toml must not be listed.
        let ttd = TempTedgeDir::new();
        let flows_dir = ttd.dir("local").dir("flows");
        flows_dir.dir("hello.toml"); // directory, not a file
        flows_dir
            .file("world.toml")
            .with_toml_content(toml::toml! { version = "1.0" });

        let flows = retrieve_flows(ttd.utf8_path());
        assert_eq!(entries(&flows), [("local", "world", "1.0")]);
    }

    #[test]
    fn flow_toml_at_top_level_is_named_flow() {
        // flows/flow.toml => name `flow`
        let ttd = TempTedgeDir::new();
        let flows_dir = ttd.dir("local").dir("flows");
        flows_dir
            .file("flow.toml")
            .with_toml_content(toml::toml! { version = "1.0" });
        flows_dir
            .file("hello.toml")
            .with_toml_content(toml::toml! { version = "2.0" });

        let flows = retrieve_flows(ttd.utf8_path());
        assert_eq!(
            entries(&flows),
            [("local", "flow", "1.0"), ("local", "hello", "2.0")]
        );
    }

    #[test]
    fn dot_toml_file_is_ignored() {
        // flows/hello/.toml => ignored (empty stem)
        let ttd = TempTedgeDir::new();
        let flows_dir = ttd.dir("local").dir("flows");
        flows_dir
            .dir("hello")
            .file(".toml")
            .with_toml_content(toml::toml! { version = "1.0" });

        let flows = retrieve_flows(ttd.utf8_path());
        assert!(flows.is_empty());
    }

    #[test]
    fn params_toml_is_ignored() {
        let ttd = TempTedgeDir::new();
        let flows_dir = ttd.dir("local").dir("flows");
        flows_dir.file("params.toml");
        flows_dir.dir("hello").file("params.toml");
        flows_dir
            .file("world.toml")
            .with_toml_content(toml::toml! { version = "1.0" });

        let flows = retrieve_flows(ttd.utf8_path());
        assert_eq!(entries(&flows), [("local", "world", "1.0")]);
    }

    #[test]
    fn non_toml_files_are_ignored() {
        let ttd = TempTedgeDir::new();
        let flows_dir = ttd.dir("local").dir("flows");
        flows_dir.file("script.js");
        flows_dir.file("readme.md");
        flows_dir.file("hello.toml.template");
        flows_dir.dir("sub").file("script.js");
        flows_dir
            .file("hello.toml")
            .with_toml_content(toml::toml! { version = "1.0" });

        let flows = retrieve_flows(ttd.utf8_path());
        assert_eq!(entries(&flows), [("local", "hello", "1.0")]);
    }

    #[test]
    fn duplicate_names_are_both_listed() {
        // Both hello.toml and hello/flow.toml exist — both appear in the listing.
        let ttd = TempTedgeDir::new();
        let flows_dir = ttd.dir("local").dir("flows");
        flows_dir
            .file("hello.toml")
            .with_toml_content(toml::toml! { version = "1.0" });
        flows_dir
            .dir("hello")
            .file("flow.toml")
            .with_toml_content(toml::toml! { version = "2.0" });

        let flows = retrieve_flows(ttd.utf8_path());
        // Sorted by (mapper, name, version): 1.0 < 2.0
        assert_eq!(
            entries(&flows),
            [("local", "hello", "1.0"), ("local", "hello", "2.0")]
        );
    }

    #[test]
    fn multiple_mappers_are_listed() {
        let ttd = TempTedgeDir::new();

        let c8y_flows = ttd.dir("c8y").dir("flows");
        c8y_flows
            .file("measurements.toml")
            .with_toml_content(toml::toml! { version = "1.0" });
        c8y_flows
            .dir("alarms")
            .file("flow.toml")
            .with_toml_content(toml::toml! { version = "2.0" });

        let local_flows = ttd.dir("local").dir("flows");
        local_flows
            .dir("hello")
            .dir("world")
            .file("flow.toml")
            .with_toml_content(toml::toml! { version = "3.0" });
        local_flows.file("params.toml");

        let flows = retrieve_flows(ttd.utf8_path());
        assert_eq!(
            entries(&flows),
            [
                ("c8y", "alarms", "2.0"),
                ("c8y", "measurements", "1.0"),
                ("local", "hello/world", "3.0"),
            ]
        );
    }

    /// Extract (mapper, name, version) tuples from a flow list for readable assertions.
    fn entries(flows: &[FlowEntry]) -> Vec<(&str, &str, &str)> {
        flows
            .iter()
            .map(|f| {
                (
                    f.flow_record.mapper_name.as_str(),
                    f.flow_record.flow_name.as_str(),
                    f.version.as_str(),
                )
            })
            .collect()
    }
}
