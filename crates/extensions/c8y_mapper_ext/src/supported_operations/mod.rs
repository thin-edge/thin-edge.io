//! [Supported Operations API]
//! (https://thin-edge.github.io/thin-edge.io/operate/c8y/supported-operations/#supported-operations-api).
//!
//! This module should encompass loading and saving supported operations from and to the filesystem.
//!
//! This module works with a c8y operations directory, which by default is `/etc/tedge/operations/c8y`. This directory
//! contains operation files for the main device, directories that contain operation files for child devices and
//! operation templates for all devices.
//!
//! The names of files in these directories should be the same as names of supported c8y operations, for example
//! `c8y_Restart`.
//!
//! For known c8y operations, the file can be empty. For custom c8y operations, the file should contain a
//! [`OnMessageExec`] section describing how c8y mapper should convert this c8y operation to a local thin-edge command.

pub mod operation;

use c8y_api::json_c8y_deserializer::C8yDeviceControlTopic;
use c8y_api::smartrest::payload::SmartrestPayload;
use c8y_api::smartrest::smartrest_serializer::declare_supported_operations;
use c8y_api::smartrest::topic::C8yTopic;
use operation::get_operation;
use operation::get_operations;
use operation::Operation;
use tedge_api::substitution::Record;
use tedge_config::TopicPrefix;
use tedge_mqtt_ext::MqttMessage;
use tedge_mqtt_ext::TopicFilter;
use tedge_utils::file;

use anyhow::ensure;
use anyhow::Context;
use camino::Utf8Path;
use camino::Utf8PathBuf;
use std::collections::BTreeMap;
use std::collections::HashMap;
use std::collections::HashSet;
use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;
use tracing::error;
use tracing::warn;

type ExternalId = String;
type ExternalIdRef = str;

type OperationName = String;
type OperationNameRef = str;

/// Used to hold and query supported operations for all devices.
pub struct SupportedOperations {
    /// External ID of the main device.
    ///
    /// Required because when given an external id of the main device when creating an operation, we want to create it
    /// in a main directory, instead of a subdirectory.
    pub device_id: String,

    /// Base c8y operations directory for all devices.
    ///
    /// By default `/etc/tedge/operations/c8y`. Contains operation files for the main device, operation templates, and
    /// directories with operation files for child devices.
    pub base_ops_dir: Arc<Utf8Path>,

    /// Currently loaded set of supported operations for all registered devices by its external id.
    pub operations_by_xid: HashMap<String, Operations>,
}

impl SupportedOperations {
    /// Add an operation to the supported operation set for a given device.
    ///
    /// Creates and writes new operation file to the filesystem.
    pub fn add_operation(
        &self,
        device_xid: &ExternalIdRef,
        c8y_operation_name: &OperationNameRef,
    ) -> Result<(), anyhow::Error> {
        let ops_file = if device_xid == self.device_id {
            self.base_ops_dir.join(c8y_operation_name)
        } else {
            self.base_ops_dir.join(device_xid).join(c8y_operation_name)
        };

        // Create directory for a device if it doesn't exist yet
        file::create_directory_with_defaults(ops_file.parent().expect("should never fail"))?;

        // if a template for such operation already exists on the main device, that means we should symlink to it,
        // because it should contain properties required for custom operation
        let operations = self
            .operations_by_xid
            .get(&self.device_id)
            .context("main device should be present")?;

        if let Some(template_name) =
            operations.get_template_name_by_operation_name(c8y_operation_name)
        {
            let template_path = self.base_ops_dir.join(template_name);
            file::create_symlink(template_path, &ops_file)?;
        } else {
            file::create_file_with_defaults(&ops_file, None)?;
        };

        Ok(())
    }

    /// Loads and saves a new supported operation set from a given directory.
    ///
    /// Matches the given directory and loads the operations to the device with an appropriate external id.
    ///
    /// All operation files from the given operation directory are loaded and set as the new supported operation set for
    /// a given device. Invalid operation files are ignored.
    ///
    /// If the supported operation set changed, `Ok(true)` is returned to denote that this change should be sent to the
    /// cloud.
    ///
    /// # Errors
    ///
    /// The function will return an error if the given device operations directory is not the same as or inside the base
    /// operations directory or if the directory doesn't exist.
    pub fn load_from_dir<C: Record + C8yPrefix>(
        &mut self,
        ops_dir: &Path,
        bridge_config: &C,
    ) -> Result<bool, anyhow::Error> {
        let device_xid = self.xid_from_path(ops_dir)?;

        self.load_all(&device_xid, bridge_config)
    }

    /// Load an operation with a given name from the c8y device operation directory.
    ///
    /// Returns `Err` if the operation file doesn't exist.
    ///
    /// If the supported operation set changed, `Ok(true)` is returned to denote that this change should be sent to the
    /// cloud.
    pub fn load<C: Record + C8yPrefix>(
        &mut self,
        device_xid: &ExternalIdRef,
        c8y_operation_name: &OperationNameRef,
        bridge_config: &C,
    ) -> Result<bool, anyhow::Error> {
        let ops_file = self.ops_file_name_for_device(device_xid, c8y_operation_name);

        let operation = get_operation(ops_file.as_std_path(), bridge_config)?;

        let current_operations =
            if let Some(current_operations) = self.operations_by_xid.get_mut(device_xid) {
                current_operations
            } else {
                self.operations_by_xid
                    .insert(device_xid.to_string(), Operations::default());
                self.operations_by_xid.get_mut(device_xid).unwrap()
            };

        let prev_operation = current_operations.insert_operation(operation);

        // even if the body of the operation is different, as long as it has the same name, supported operations message
        // will be the same, so we don't need to resend
        let modified = prev_operation.is_none();

        Ok(modified)
    }

    /// Loads all operations from a device c8y operations directory and saves a new supported operation set for a given
    /// device.
    ///
    /// All operation files from the given operation directory are loaded and set as the new supported operation set for
    /// a given device. Invalid operation files are ignored.
    ///
    /// If the supported operation set changed, `Ok(true)` is returned to denote that this change should be sent to the
    /// cloud.
    pub fn load_all<C: Record + C8yPrefix>(
        &mut self,
        device_xid: &ExternalIdRef,
        bridge_config: &C,
    ) -> Result<bool, anyhow::Error> {
        // load operations from the directory
        let dir = self.base_ops_dir_for_device(device_xid);
        let new_operations = Operations::try_new(dir, bridge_config)?;

        // the current supported operations set is empty
        // TODO: simplify
        let Some(current_operations) = self.operations_by_xid.get_mut(device_xid) else {
            self.operations_by_xid
                .insert(device_xid.to_string(), new_operations);
            return Ok(true);
        };

        // current operation set is not empty but it's different
        let modified = *current_operations != new_operations;
        *current_operations = new_operations;

        Ok(modified)
    }

    /// If a given directory is a c8y operations directory of a device, return that device's external id.
    pub fn xid_from_path(&self, ops_dir: &Path) -> Result<ExternalId, anyhow::Error> {
        ensure!(
            ops_dir.starts_with(self.base_ops_dir.as_std_path()),
            format!(
                "given path '{}' is not the same as or inside the base operations directory '{}'",
                ops_dir.to_string_lossy(),
                self.base_ops_dir
            )
        );

        ensure!(
            ops_dir.is_dir(),
            format!(
                "given path '{}' does not point to a directory",
                ops_dir.to_string_lossy(),
            )
        );

        let suffix = ops_dir.strip_prefix(self.base_ops_dir.as_std_path())?;

        // /etc/tedge/operations/c8y -> main device
        // /etc/tedge/operations/c8y/directory -> child device
        let device_xid = match suffix
            .to_str()
            .context("directory name should be a valid external id")?
        {
            "" => self.device_id.to_string(),
            filename => filename.to_string(),
        };

        Ok(device_xid)
    }

    /// Returns a directory path for c8y operations for the given device.
    fn base_ops_dir_for_device(&self, device_xid: &ExternalIdRef) -> Utf8PathBuf {
        if device_xid == self.device_id {
            self.base_ops_dir.to_path_buf()
        } else {
            self.base_ops_dir.join(device_xid)
        }
    }

    /// Returns a path for c8y operation file for a given device
    fn ops_file_name_for_device(
        &self,
        device_xid: &ExternalIdRef,
        c8y_operation_name: &OperationNameRef,
    ) -> Utf8PathBuf {
        if device_xid == self.device_id {
            self.base_ops_dir.join(c8y_operation_name).to_path_buf()
        } else {
            self.base_ops_dir.join(device_xid).join(c8y_operation_name)
        }
    }

    /// Create a [supported operations message][1] containing operations supported by the device.
    ///
    /// [1]: https://cumulocity.com/docs/smartrest/mqtt-static-templates/#114
    pub fn create_supported_operations(
        &self,
        device_xid: &ExternalIdRef,
        c8y_prefix: &TopicPrefix,
    ) -> Result<MqttMessage, anyhow::Error> {
        let payload = self
            .operations_by_xid
            .get(device_xid)
            .map(|o| o.create_smartrest_ops_message())
            .unwrap_or(Operations::default().create_smartrest_ops_message());

        let topic = if device_xid != self.device_id {
            C8yTopic::ChildSmartRestResponse(device_xid.into()).to_topic(c8y_prefix)?
        } else {
            C8yTopic::upstream_topic(c8y_prefix)
        };

        Ok(MqttMessage::new(&topic, payload.into_inner()))
    }

    pub fn get_operation_handlers(
        &self,
        device_xid: &ExternalIdRef,
        topic: &str,
        prefix: &TopicPrefix,
    ) -> Vec<(String, Operation)> {
        let handlers = self
            .operations_by_xid
            .get(device_xid)
            .map(|o| o.filter_by_topic(topic, prefix))
            .unwrap_or_default();

        handlers
    }

    pub fn get_json_custom_operation_topics(&self) -> Result<TopicFilter, OperationsError> {
        Ok(self
            .operations_by_xid
            .values()
            .flat_map(|o| o.operations.values())
            .filter(|operation| operation.on_fragment().is_some())
            .filter_map(|operation| operation.topic())
            .collect::<HashSet<String>>()
            .try_into()?)
    }

    pub fn get_smartrest_custom_operation_topics(&self) -> Result<TopicFilter, OperationsError> {
        Ok(self
            .operations_by_xid
            .values()
            .flat_map(|o| o.operations.values())
            .filter(|operation| operation.on_message().is_some())
            .filter_map(|operation| operation.topic())
            .collect::<HashSet<String>>()
            .try_into()?)
    }

    pub fn matching_smartrest_template(&self, operation_template: &str) -> Option<&Operation> {
        self.operations_by_xid
            .values()
            .flat_map(|o| o.operations.values())
            .find(|o| {
                o.template()
                    .is_some_and(|template| template == operation_template)
            })
    }

    /// Return operation name if `tedge_cmd` matches
    pub fn get_operation_name_by_workflow_operation(&self, command_name: &str) -> Option<String> {
        let matching_templates: Vec<&Operation> = self
            .operations_by_xid
            .get(&self.device_id)
            .map(|o| {
                o.templates
                    .iter()
                    .filter(|template| {
                        template
                            .workflow_operation()
                            .is_some_and(|operation| operation.eq(command_name))
                    })
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();

        if matching_templates.len() > 1 {
            warn!(
                "Found more than one template with the same `workflow.operation` field. Picking {}",
                matching_templates.first().unwrap().name
            );
        }

        matching_templates
            .first()
            .and_then(|template| template.on_fragment())
    }
}

/// Operations are derived by reading files subdirectories per cloud /etc/tedge/operations directory
/// Each operation is a file name in one of the subdirectories
/// The file name is the operation name
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Operations {
    operations: BTreeMap<OperationName, Operation>,
    templates: Vec<Operation>,
}

pub trait C8yPrefix {
    fn c8y_prefix(&self) -> &TopicPrefix;
}

impl Operations {
    /// Inserts a new operation.
    ///
    /// If an operation under such name was already present, the old value is returned.
    pub fn insert_operation(&mut self, operation: Operation) -> Option<Operation> {
        self.operations.insert(operation.name.clone(), operation)
    }

    /// Loads operations defined in the operations directory.
    ///
    /// Invalid operation files are ignored and logged.
    pub fn try_new<C: Record + C8yPrefix>(
        dir: impl AsRef<Path>,
        bridge_config: &C,
    ) -> Result<Self, OperationsError> {
        get_operations(dir.as_ref(), bridge_config)
    }

    pub fn add_template(&mut self, template: Operation) {
        self.templates.push(template);
    }

    pub fn get_operations_list(&self) -> Vec<&str> {
        self.operations.keys().map(String::as_str).collect()
    }

    fn filter_by_topic(&self, topic_name: &str, prefix: &TopicPrefix) -> Vec<(String, Operation)> {
        let mut vec: Vec<(String, Operation)> = Vec::new();
        for op in self.operations.values() {
            match (op.topic(), op.on_fragment()) {
                (None, Some(on_fragment)) if C8yDeviceControlTopic::name(prefix) == topic_name => {
                    vec.push((on_fragment, op.clone()))
                }
                (Some(topic), Some(on_fragment)) if topic == topic_name => {
                    vec.push((on_fragment, op.clone()))
                }
                _ => {}
            }
        }
        vec
    }

    pub fn topics_for_operations(&self) -> HashSet<String> {
        self.operations
            .values()
            .filter_map(|operation| operation.topic())
            .collect::<HashSet<String>>()
    }

    pub fn create_smartrest_ops_message(&self) -> SmartrestPayload {
        let ops = self.get_operations_list();
        declare_supported_operations(&ops)
    }

    pub fn get_template_name_by_operation_name(&self, operation_name: &str) -> Option<&str> {
        self.templates
            .iter()
            .find(|template| {
                template
                    .on_fragment()
                    .is_some_and(|name| name.eq(operation_name))
            })
            .map(|template| template.name.as_ref())
    }
}

#[derive(Debug, thiserror::Error)]
pub enum OperationsError {
    #[error("Failed to read directory: {dir}")]
    ReadDirError { dir: PathBuf },

    #[error(transparent)]
    FromIo(#[from] std::io::Error),

    #[error("Cannot extract the operation name from the path: {0}")]
    InvalidOperationName(PathBuf),

    #[error("Error while parsing operation file: '{0}': {1}.")]
    TomlError(PathBuf, #[source] toml::de::Error),

    #[error(transparent)]
    FromUtf8Error(#[from] std::string::FromUtf8Error),

    #[error(transparent)]
    FromMqttError(#[from] tedge_mqtt_ext::MqttError),
}

#[cfg(test)]
mod tests {
    use super::*;

    use operation::OnMessageExec;
    use std::str::FromStr;
    use test_case::test_case;

    #[test_case(
        r#"
        on_fragment = "c8y_Something"
        command = "echo 1"
        "#,
        r#"
        topic = "c8y/custom/one"
        on_fragment = "c8y_Something"
        command = "echo 2" 
        "#
    )]
    fn filter_by_topic(toml1: &str, toml2: &str) {
        let exec: OnMessageExec = toml::from_str(toml1).unwrap();
        let operation1 = Operation {
            name: "operation1".to_string(),
            exec: Some(exec),
        };

        let exec: OnMessageExec = toml::from_str(toml2).unwrap();
        let operation2 = Operation {
            name: "operation2".to_string(),
            exec: Some(exec),
        };

        let ops = Operations {
            operations: BTreeMap::from([
                (operation1.name.clone(), operation1.clone()),
                (operation2.name.clone(), operation2.clone()),
            ]),
            ..Default::default()
        };

        let prefix = TopicPrefix::from_str("c8y").unwrap();

        let filter_custom = ops.filter_by_topic("c8y/custom/one", &prefix);
        assert_eq!(
            filter_custom,
            vec![("c8y_Something".to_string(), operation2)]
        );

        let filter_default = ops.filter_by_topic("c8y/devicecontrol/notifications", &prefix);
        assert_eq!(
            filter_default,
            vec![("c8y_Something".to_string(), operation1)]
        );
    }
}
