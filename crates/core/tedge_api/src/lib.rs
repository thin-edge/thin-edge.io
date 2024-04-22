pub mod alarm;
pub mod commands;
pub mod entity_store;
pub mod error;
pub mod event;
pub mod health;
pub mod measurement;
pub mod mqtt_topics;
pub mod path;
mod software;
mod store;
pub mod workflow;

pub use commands::CommandStatus;
pub use commands::Jsonify;
pub use commands::OperationStatus;
pub use commands::RestartCommand;
pub use commands::SoftwareListCommand;
pub use commands::SoftwareUpdateCommand;
pub use download::*;
pub use entity_store::EntityStore;
pub use error::*;
pub use health::*;
pub use software::*;
pub use store::pending_entity_store;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mqtt_topics::Channel;
    use crate::mqtt_topics::EntityTopicId;
    use crate::mqtt_topics::MqttSchema;
    use crate::mqtt_topics::OperationType;
    use mqtt_channel::MqttMessage;
    use mqtt_channel::QoS;
    use mqtt_channel::Topic;

    #[test]
    fn topic_names() {
        // There are two topics for each kind of commands,
        // one for the metadata, the other for the command instances
        let mqtt_schema = MqttSchema::default();
        let device = EntityTopicId::default_main_device();
        let cmd_id = "abc".to_string();

        assert_eq!(
            SoftwareListCommand::capability_message(&mqtt_schema, &device).topic,
            Topic::new_unchecked("te/device/main///cmd/software_list")
        );
        assert_eq!(
            SoftwareListCommand::new(&device, cmd_id.clone())
                .command_message(&mqtt_schema)
                .topic,
            Topic::new_unchecked("te/device/main///cmd/software_list/abc")
        );

        assert_eq!(
            SoftwareUpdateCommand::capability_message(&mqtt_schema, &device).topic,
            Topic::new_unchecked("te/device/main///cmd/software_update")
        );
        assert_eq!(
            SoftwareUpdateCommand::new(&device, cmd_id.clone())
                .command_message(&mqtt_schema)
                .topic,
            Topic::new_unchecked("te/device/main///cmd/software_update/abc")
        );

        assert_eq!(
            RestartCommand::capability_message(&mqtt_schema, &device).topic,
            Topic::new_unchecked("te/device/main///cmd/restart")
        );
        assert_eq!(
            RestartCommand::new(&device, cmd_id.clone())
                .command_message(&mqtt_schema)
                .topic,
            Topic::new_unchecked("te/device/main///cmd/restart/abc")
        );
    }

    #[test]
    fn creating_a_software_list_request() {
        let mqtt_schema = MqttSchema::default();
        let device = EntityTopicId::default_child_device("abc").unwrap();
        let request = SoftwareListCommand::new(&device, "1".to_string());

        let expected_msg = MqttMessage {
            topic: Topic::new_unchecked("te/device/abc///cmd/software_list/1"),
            payload: r#"{"status":"init"}"#.to_string().into(),
            qos: QoS::AtLeastOnce,
            retain: true,
        };
        let actual_msg = request.command_message(&mqtt_schema);
        assert_eq!(actual_msg, expected_msg);
    }

    #[test]
    fn using_a_software_list_request() {
        let device = EntityTopicId::default_child_device("abc").unwrap();
        let cmd_id = "123".to_string();
        let json_request = r#"{ "status": "init" }"#.as_bytes();
        let request = SoftwareListCommand::try_from(device, cmd_id, json_request);

        assert!(request.is_ok());
        let request = request.unwrap();
        assert!(request.is_some());
        let request = request.unwrap();
        assert_eq!(request.cmd_id, "123");
        assert_eq!(request.status(), CommandStatus::Init);
    }

    #[test]
    fn clearing_a_software_list_request() {
        let device = EntityTopicId::default_child_device("abc").unwrap();
        let cmd_id = "123".to_string();
        let payload = "".as_bytes();
        let request = SoftwareListCommand::try_from(device, cmd_id, payload);

        assert!(request.is_ok());
        assert!(request.unwrap().is_none());
    }

    #[test]
    fn creating_a_software_list_response() {
        let device = EntityTopicId::default_child_device("abc").unwrap();
        let mut response = SoftwareListCommand::new(&device, "1".to_string())
            .with_status(CommandStatus::Successful);

        response.add_modules(
            "debian".to_string(),
            vec![
                SoftwareModule {
                    module_type: Some("debian".to_string()),
                    name: "a".to_string(),
                    version: None,
                    url: None,
                    file_path: None,
                },
                SoftwareModule {
                    module_type: Some("debian".to_string()),
                    name: "b".to_string(),
                    version: Some("1.0".to_string()),
                    url: None,
                    file_path: None,
                },
                SoftwareModule {
                    module_type: Some("debian".to_string()),
                    name: "c".to_string(),
                    version: None,
                    url: Some("https://foobar.io/c.deb".into()),
                    file_path: None,
                },
                SoftwareModule {
                    module_type: Some("debian".to_string()),
                    name: "d".to_string(),
                    version: Some("beta".to_string()),
                    url: Some("https://foobar.io/d.deb".into()),
                    file_path: None,
                },
            ],
        );

        response.add_modules(
            "apama".to_string(),
            vec![SoftwareModule {
                module_type: Some("apama".to_string()),
                name: "m".to_string(),
                version: None,
                url: Some("https://foobar.io/m.epl".into()),
                file_path: None,
            }],
        );

        let message = response.command_message(&MqttSchema::default());

        let expected_json = r#"{
            "status":"successful",
            "currentSoftwareList":[
                {"type":"debian", "modules":[
                    {"name":"a"},
                    {"name":"b","version":"1.0"},
                    {"name":"c","url":"https://foobar.io/c.deb"},
                    {"name":"d","version":"beta","url":"https://foobar.io/d.deb"}
                ]},
                {"type":"apama","modules":[
                    {"name":"m","url":"https://foobar.io/m.epl"}
                ]}
            ]}"#;
        let actual_json = message.payload_str().unwrap();
        assert_eq!(actual_json, remove_whitespace(expected_json));
    }

    #[test]
    fn using_a_software_list_response() {
        let device = EntityTopicId::default_child_device("abc").unwrap();
        let cmd_id = "123".to_string();
        let json_response = r#"{
            "status": "successful",
            "currentSoftwareList":[
                {"type":"debian", "modules":[
                    {"name":"a"},
                    {"name":"b","version":"1.0"},
                    {"name":"c","url":"https://foobar.io/c.deb"},
                    {"name":"d","version":"beta","url":"https://foobar.io/d.deb"}
                ]},
                {"type":"apama","modules":[
                    {"name":"m","url":"https://foobar.io/m.epl"}
                ]}
            ]}"#;

        let response = SoftwareListCommand::try_from(device, cmd_id, json_response.as_bytes());
        assert!(response.is_ok());
        let response = response.unwrap();
        assert!(response.is_some());
        let response = response.unwrap();
        assert_eq!(response.cmd_id, "123");
        assert_eq!(response.status(), CommandStatus::Successful);

        // The mapper can use then the current list of modules
        assert_eq!(
            response.modules(),
            vec![
                SoftwareModule {
                    module_type: Some("debian".to_string()),
                    name: "a".to_string(),
                    version: None,
                    url: None,
                    file_path: None,
                },
                SoftwareModule {
                    module_type: Some("debian".to_string()),
                    name: "b".to_string(),
                    version: Some("1.0".to_string()),
                    url: None,
                    file_path: None,
                },
                SoftwareModule {
                    module_type: Some("debian".to_string()),
                    name: "c".to_string(),
                    version: None,
                    url: Some("https://foobar.io/c.deb".into()),
                    file_path: None,
                },
                SoftwareModule {
                    module_type: Some("debian".to_string()),
                    name: "d".to_string(),
                    version: Some("beta".to_string()),
                    url: Some("https://foobar.io/d.deb".into()),
                    file_path: None,
                },
                SoftwareModule {
                    module_type: Some("apama".to_string()),
                    name: "m".to_string(),
                    version: None,
                    url: Some("https://foobar.io/m.epl".into()),
                    file_path: None,
                },
            ]
        );
    }

    #[test]
    fn creating_a_software_list_error() {
        let device = EntityTopicId::default_child_device("abc").unwrap();
        let response = SoftwareListCommand::new(&device, "123".to_string());
        let response = response.with_error("Request_timed-out".to_string());
        let message = response.command_message(&MqttSchema::default());

        let expected_json = r#"{
            "status": "failed",
            "reason": "Request_timed-out"
        }"#;

        let actual_json = message.payload_str().unwrap();
        assert_eq!(actual_json, remove_whitespace(expected_json));
    }

    #[test]
    fn using_a_software_list_error() {
        let device = EntityTopicId::default_child_device("abc").unwrap();
        let cmd_id = "123".to_string();
        let json_response = r#"{
            "status": "failed",
            "reason": "Request timed-out"
        }"#;
        let response = SoftwareListCommand::try_from(device, cmd_id, json_response.as_bytes());
        assert!(response.is_ok());
        let response = response.unwrap();
        assert!(response.is_some());
        let response = response.unwrap();
        assert_eq!(response.cmd_id, "123");
        assert_eq!(
            response.status(),
            CommandStatus::Failed {
                reason: "Request timed-out".to_string()
            }
        );
        assert_eq!(response.modules(), vec![]);
    }

    #[test]
    fn creating_a_software_update_request() {
        let device = EntityTopicId::default_child_device("abc").unwrap();
        let cmd_id = "123".to_string();
        let mut request = SoftwareUpdateCommand::new(&device, cmd_id);

        request.add_updates(
            "debian",
            vec![
                SoftwareModuleUpdate::install(SoftwareModule {
                    module_type: Some("debian".to_string()),
                    name: "nodered".to_string(),
                    version: Some("1.0.0".to_string()),
                    url: None,
                    file_path: None,
                }),
                SoftwareModuleUpdate::install(SoftwareModule {
                    module_type: Some("debian".to_string()),
                    name: "collectd".to_string(),
                    version: Some("5.7".to_string()),
                    url: Some(
                        "https://collectd.org/download/collectd-tarballs/collectd-5.12.0.tar.bz2"
                            .into(),
                    ),
                    file_path: None,
                }),
            ],
        );

        request.add_updates(
            "docker",
            vec![
                SoftwareModuleUpdate::install(SoftwareModule {
                    module_type: Some("docker".to_string()),
                    name: "nginx".to_string(),
                    version: Some("1.21.0".to_string()),
                    url: None,
                    file_path: None,
                }),
                SoftwareModuleUpdate::remove(SoftwareModule {
                    module_type: Some("docker".to_string()),
                    name: "mongodb".to_string(),
                    version: Some("4.4.6".to_string()),
                    url: None,
                    file_path: None,
                }),
            ],
        );

        let expected_json = r#"{
            "status": "init",
            "updateList": [
                {
                    "type": "debian",
                    "modules": [
                        {
                            "name": "nodered",
                            "version": "1.0.0",
                            "action": "install"
                        },
                        {
                            "name": "collectd",
                            "version": "5.7",
                            "url": "https://collectd.org/download/collectd-tarballs/collectd-5.12.0.tar.bz2",
                            "action": "install"
                        }
                    ]
                },
                {
                    "type": "docker",
                    "modules": [
                        {
                            "name": "nginx",
                            "version": "1.21.0",
                            "action": "install"
                        },
                        {
                            "name": "mongodb",
                            "version": "4.4.6",
                            "action": "remove"
                        }
                    ]
                }
            ]
        }"#;
        let actual_json = request.payload.to_json();
        assert_eq!(actual_json, remove_whitespace(expected_json));
    }

    #[test]
    fn creating_a_software_update_request_grouping_updates_per_plugin() {
        let device = EntityTopicId::default_child_device("abc").unwrap();
        let cmd_id = "123".to_string();
        let mut request = SoftwareUpdateCommand::new(&device, cmd_id);

        request.add_update(SoftwareModuleUpdate::install(SoftwareModule {
            module_type: Some("debian".to_string()),
            name: "nodered".to_string(),
            version: Some("1.0.0".to_string()),
            url: None,
            file_path: None,
        }));
        request.add_update(SoftwareModuleUpdate::install(SoftwareModule {
            module_type: Some("docker".to_string()),
            name: "nginx".to_string(),
            version: Some("1.21.0".to_string()),
            url: None,
            file_path: None,
        }));
        request.add_update(SoftwareModuleUpdate::install(SoftwareModule {
            module_type: Some("debian".to_string()),
            name: "collectd".to_string(),
            version: Some("5.7".to_string()),
            url: Some(
                "https://collectd.org/download/collectd-tarballs/collectd-5.12.0.tar.bz2".into(),
            ),
            file_path: None,
        }));
        request.add_update(SoftwareModuleUpdate::remove(SoftwareModule {
            module_type: Some("docker".to_string()),
            name: "mongodb".to_string(),
            version: Some("4.4.6".to_string()),
            url: None,
            file_path: None,
        }));

        let expected_json = r#"{
            "status": "init",
            "updateList": [
                {
                    "type": "debian",
                    "modules": [
                        {
                            "name": "nodered",
                            "version": "1.0.0",
                            "action": "install"
                        },
                        {
                            "name": "collectd",
                            "version": "5.7",
                            "url": "https://collectd.org/download/collectd-tarballs/collectd-5.12.0.tar.bz2",
                            "action": "install"
                        }
                    ]
                },
                {
                    "type": "docker",
                    "modules": [
                        {
                            "name": "nginx",
                            "version": "1.21.0",
                            "action": "install"
                        },
                        {
                            "name": "mongodb",
                            "version": "4.4.6",
                            "action": "remove"
                        }
                    ]
                }
            ]
        }"#;
        let actual_json = request.payload.to_json();
        assert_eq!(actual_json, remove_whitespace(expected_json));
    }

    #[test]
    fn creating_a_software_update_request_grouping_updates_per_plugin_using_default() {
        let device = EntityTopicId::default_child_device("abc").unwrap();
        let cmd_id = "123".to_string();
        let mut request = SoftwareUpdateCommand::new(&device, cmd_id);

        request.add_update(SoftwareModuleUpdate::install(SoftwareModule {
            module_type: None, // I.e. default
            name: "nodered".to_string(),
            version: Some("1.0.0".to_string()),
            url: None,
            file_path: None,
        }));
        request.add_update(SoftwareModuleUpdate::install(SoftwareModule {
            module_type: Some("".to_string()), // I.e. default
            name: "nginx".to_string(),
            version: Some("1.21.0".to_string()),
            url: None,
            file_path: None,
        }));
        request.add_update(SoftwareModuleUpdate::install(SoftwareModule {
            module_type: Some("default".to_string()), // I.e. default
            name: "collectd".to_string(),
            version: Some("5.7".to_string()),
            url: None,
            file_path: None,
        }));
        request.add_update(SoftwareModuleUpdate::remove(SoftwareModule {
            module_type: Some("debian".to_string()), // Unless specified otherwise, this is not the default
            name: "mongodb".to_string(),
            version: Some("4.4.6".to_string()),
            url: None,
            file_path: None,
        }));

        let expected_json = r#"{
            "status": "init",
            "updateList": [
                {
                    "type": "default",
                    "modules": [
                        {
                            "name": "nodered",
                            "version": "1.0.0",
                            "action": "install"
                        },
                        {
                            "name": "nginx",
                            "version": "1.21.0",
                            "action": "install"
                        },
                        {
                            "name": "collectd",
                            "version": "5.7",
                            "action": "install"
                        }
                    ]
                },
                {
                    "type": "debian",
                    "modules": [
                        {
                            "name": "mongodb",
                            "version": "4.4.6",
                            "action": "remove"
                        }
                    ]
                }
            ]
        }"#;
        let actual_json = request.payload.to_json();
        assert_eq!(actual_json, remove_whitespace(expected_json));
    }

    #[test]
    fn using_a_software_update_request() {
        let mqtt_schema = MqttSchema::default();
        let topic = Topic::new_unchecked("te/device/abc///cmd/software_update/123");
        let json_request = r#"{
            "status": "init",
            "updateList": [
                {
                    "type": "debian",
                    "modules": [
                        {
                            "name": "nodered",
                            "version": "1.0.0",
                            "action": "install"
                        },
                        {
                            "name": "collectd",
                            "version": "5.7",
                            "url": "https://collectd.org/download/collectd-tarballs/collectd-5.12.0.tar.bz2",
                            "action": "install"
                        }
                    ]
                },
                {
                    "type": "docker",
                    "modules": [
                        {
                            "name": "nginx",
                            "version": "1.21.0",
                            "action": "install"
                        },
                        {
                            "name": "mongodb",
                            "version": "4.4.6",
                            "action": "remove"
                        }
                    ]
                }
            ]
        }"#;
        let (device, cmd) = mqtt_schema.entity_channel_of(&topic).unwrap();
        assert_eq!(
            cmd,
            Channel::Command {
                operation: OperationType::SoftwareUpdate,
                cmd_id: "123".to_string()
            }
        );
        let request =
            SoftwareUpdateCommand::try_from(device, "123".to_string(), json_request.as_bytes())
                .expect("Failed to deserialize")
                .expect("Some command");

        assert_eq!(
            request.modules_types(),
            vec!["debian".to_string(), "docker".to_string(),]
        );

        assert_eq!(
            request.updates_for("debian"),
            vec![
                SoftwareModuleUpdate::install(SoftwareModule {
                    module_type: Some("debian".to_string()),
                    name: "nodered".to_string(),
                    version: Some("1.0.0".to_string()),
                    url: None,
                    file_path: None,
                }),
                SoftwareModuleUpdate::install(SoftwareModule {
                    module_type: Some("debian".to_string()),
                    name: "collectd".to_string(),
                    version: Some("5.7".to_string()),
                    url: Some(
                        "https://collectd.org/download/collectd-tarballs/collectd-5.12.0.tar.bz2"
                            .into(),
                    ),
                    file_path: None,
                }),
            ]
        );

        assert_eq!(
            request.updates_for("docker"),
            vec![
                SoftwareModuleUpdate::install(SoftwareModule {
                    module_type: Some("docker".to_string()),
                    name: "nginx".to_string(),
                    version: Some("1.21.0".to_string()),
                    url: None,
                    file_path: None,
                }),
                SoftwareModuleUpdate::remove(SoftwareModule {
                    module_type: Some("docker".to_string()),
                    name: "mongodb".to_string(),
                    version: Some("4.4.6".to_string()),
                    url: None,
                    file_path: None,
                }),
            ]
        );
    }

    #[test]
    fn creating_a_software_update_response() {
        let device = EntityTopicId::default_child_device("abc").unwrap();
        let cmd_id = "123".to_string();
        let request = SoftwareUpdateCommand::new(&device, cmd_id);

        let response = request.with_status(CommandStatus::Executing);

        let expected_json = r#"{
            "status": "executing"
        }"#;

        let actual_json = response.payload.to_json();
        assert_eq!(actual_json, remove_whitespace(expected_json));
    }

    #[test]
    fn using_a_software_update_executing_response() {
        let device = EntityTopicId::default_child_device("abc").unwrap();
        let cmd_id = "123".to_string();
        let json_response = r#"{
                "status": "executing"
            }"#;
        let response = SoftwareUpdateCommand::try_from(device, cmd_id, json_response.as_bytes())
            .expect("Failed to deserialize")
            .expect("some command");
        assert_eq!(response.cmd_id, "123".to_string());
        assert_eq!(response.status(), CommandStatus::Executing);
    }

    #[test]
    fn finalizing_a_software_update_response() {
        let device = EntityTopicId::default_child_device("abc").unwrap();
        let cmd_id = "123".to_string();
        let json_request = r#"{
            "status": "init",
            "updateList": [
                {
                    "type": "debian",
                    "modules": [
                        {
                            "name": "nodered",
                            "version": "1.0.0",
                            "action": "install"
                        },
                        {
                            "name": "collectd",
                            "version": "5.7",
                            "url": "https://collectd.org/download/collectd-tarballs/collectd-5.12.0.tar.bz2",
                            "action": "install"
                        }
                    ]
                },
                {
                    "type": "docker",
                    "modules": [
                        {
                            "name": "nginx",
                            "version": "1.21.0",
                            "action": "install"
                        },
                        {
                            "name": "mongodb",
                            "version": "4.4.6",
                            "action": "remove"
                        }
                    ]
                }
            ]
        }"#;
        let request = SoftwareUpdateCommand::try_from(device, cmd_id, json_request.as_bytes())
            .expect("Failed to deserialize")
            .expect("Some command");
        let response = request.with_status(CommandStatus::Successful);

        let expected_json = r#"{
                "status": "successful",
                "updateList": [
                    {
                        "type": "debian",
                        "modules": [
                            {
                                "name": "nodered",
                                "version": "1.0.0",
                                "action": "install"
                            },
                            {
                                "name": "collectd",
                                "version": "5.7",
                                "url": "https://collectd.org/download/collectd-tarballs/collectd-5.12.0.tar.bz2",
                                "action": "install"
                            }
                        ]
                    },
                    {
                        "type": "docker",
                        "modules": [
                            {
                                "name": "nginx",
                                "version": "1.21.0",
                                "action": "install"
                            },
                            {
                                "name": "mongodb",
                                "version": "4.4.6",
                                "action": "remove"
                            }
                        ]
                    }
                ]
            }"#;

        let actual_json = response.payload.to_json();
        assert_eq!(actual_json, remove_whitespace(expected_json));
    }

    #[test]
    fn finalizing_a_software_update_error() {
        let device = EntityTopicId::default_child_device("abc").unwrap();
        let cmd_id = "123".to_string();
        let json_request = r#"{
            "status": "init",
            "updateList": [
                {
                    "type": "debian",
                    "modules": [
                        {
                            "name": "nodered",
                            "version": "1.0.0",
                            "action": "install"
                        },
                        {
                            "name": "collectd",
                            "version": "5.7",
                            "url": "https://collectd.org/download/collectd-tarballs/collectd-5.12.0.tar.bz2",
                            "action": "install"
                        }
                    ]
                },
                {
                    "type": "docker",
                    "modules": [
                        {
                            "name": "nginx",
                            "version": "1.21.0",
                            "action": "install"
                        },
                        {
                            "name": "mongodb",
                            "version": "4.4.6",
                            "action": "remove"
                        }
                    ]
                }
            ]
        }"#;
        let request = SoftwareUpdateCommand::try_from(device, cmd_id, json_request.as_bytes())
            .expect("Failed to deserialize")
            .expect("Some command");
        let mut response = request.with_error(
            "2 errors: fail to install [ collectd ] fail to remove [ mongodb ]".to_string(),
        );
        response.add_errors(
            "debian",
            vec![SoftwareError::Install {
                module: Box::new(SoftwareModule {
                    module_type: Some("debian".to_string()),
                    name: "collectd".to_string(),
                    version: Some("5.7".to_string()),
                    url: None,
                    file_path: None,
                }),
                reason: "Network timeout".to_string(),
            }],
        );

        response.add_errors(
            "docker",
            vec![SoftwareError::Remove {
                module: Box::new(SoftwareModule {
                    module_type: Some("docker".to_string()),
                    name: "mongodb".to_string(),
                    version: Some("4.4.6".to_string()),
                    url: None,
                    file_path: None,
                }),
                reason: "Other components dependent on it".to_string(),
            }],
        );

        let expected_json = r#"{
                "status":"failed",
                "reason":"2 errors: fail to install [ collectd ] fail to remove [ mongodb ]",
                "updateList": [
                    {
                        "type": "debian",
                        "modules": [
                            {
                                "name": "nodered",
                                "version": "1.0.0",
                                "action": "install"
                            },
                            {
                                "name": "collectd",
                                "version": "5.7",
                                "url": "https://collectd.org/download/collectd-tarballs/collectd-5.12.0.tar.bz2",
                                "action": "install"
                            }
                        ]
                    },
                    {
                        "type": "docker",
                        "modules": [
                            {
                                "name": "nginx",
                                "version": "1.21.0",
                                "action": "install"
                            },
                            {
                                "name": "mongodb",
                                "version": "4.4.6",
                                "action": "remove"
                            }
                        ]
                    }
                ],
                "failures":[
                    {
                        "type":"debian",
                        "modules": [
                            {
                                "name":"collectd",
                                "version":"5.7",
                                "action":"install",
                                "reason":"Network timeout"
                            }
                        ]
                    },
                    {
                        "type":"docker",
                        "modules": [
                            {
                                "name": "mongodb",
                                "version": "4.4.6",
                                "action":"remove",
                                "reason":"Other components dependent on it"
                            }
                        ]
                    }
                ]
            }"#;

        let actual_json = response.payload.to_json();
        assert_eq!(
            remove_whitespace(&actual_json),
            remove_whitespace(expected_json)
        );
    }
    /*

        #[test]
        fn using_a_software_update_response() {
            let json_response = r#"{
                    "id": "123",
                    "status":"failed",
                    "reason":"2 errors: fail to install [ collectd ] fail to remove [ mongodb ]",
                    "currentSoftwareList": [
                        {
                            "type": "debian",
                            "modules": [
                                {
                                    "name": "nodered",
                                    "version": "1.0.0"
                                }
                            ]
                        },
                        {
                            "type": "docker",
                            "modules": [
                                {
                                    "name": "nginx",
                                    "version": "1.21.0"
                                },
                                {
                                    "name": "mongodb",
                                    "version": "4.4.6"
                                }
                            ]
                        }
                    ],
                    "failures": [
                        {
                            "type":"debian",
                            "modules": [
                                {
                                    "name":"collectd",
                                    "version":"5.7",
                                    "action":"install",
                                    "reason":"Network timeout"
                                }
                            ]
                        },
                        {
                            "type":"docker",
                            "modules": [
                                {
                                    "name": "mongodb",
                                    "version": "4.4.6",
                                    "action":"remove",
                                    "reason":"Other components dependent on it"
                                }
                            ]
                        }
                    ]
                }"#;
            let response =
                SoftwareUpdateCommandPayload::from_json(json_response).expect("Failed to deserialize");

            assert_eq!(response.id(), "123");
            assert_eq!(response.status(), OperationStatus::Failed);
            assert_eq!(
                response.error(),
                Some("2 errors: fail to install [ collectd ] fail to remove [ mongodb ]".into())
            );

            // The C8Y mapper doesn't use the failures list
            // => no support for now
        }
    */
    fn remove_whitespace(s: &str) -> String {
        let mut s = String::from(s);
        s.retain(|c| !c.is_whitespace());
        s
    }
}
