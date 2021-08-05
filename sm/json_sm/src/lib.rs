mod error;
mod messages;
mod software;

pub use error::*;
pub use messages::{
    software_filter_topic, Jsonify, SoftwareListRequest, SoftwareListResponse,
    SoftwareOperationStatus, SoftwareRequestResponse, SoftwareUpdateRequest,
    SoftwareUpdateResponse,
};
pub use software::*;

#[cfg(test)]
mod tests {
    use super::*;
    use regex::Regex;

    #[test]
    fn topic_names() {
        // There are two topics for each kind of requests,
        // one for the requests, the other for the responses
        assert_eq!(
            SoftwareListRequest::topic_name(),
            "tedge/commands/req/software/list"
        );
        assert_eq!(
            SoftwareListResponse::topic_name(),
            "tedge/commands/res/software/list"
        );
        assert_eq!(
            SoftwareUpdateRequest::topic_name(),
            "tedge/commands/req/software/update"
        );
        assert_eq!(
            SoftwareUpdateResponse::topic_name(),
            "tedge/commands/res/software/update"
        );
    }

    #[test]
    fn creating_a_software_list_request() {
        let request = SoftwareListRequest::new_with_id("1");

        let expected_json = r#"{"id":"1"}"#;
        let actual_json = request.to_json().expect("Failed to serialize");
        assert_eq!(actual_json, expected_json);
    }

    #[test]
    fn creating_a_software_list_request_with_generated_id() {
        let request = SoftwareListRequest::new();
        let generated_id = request.id;

        // The generated id is a nanoid of 21 characters from A-Za-z0-9_~
        let re = Regex::new(r"[A-Za-z0-9_~-]{21,21}").unwrap();
        assert!(re.is_match(&generated_id));
    }

    #[test]
    fn using_a_software_list_request() {
        let json_request = r#"{"id":"123"}"#;
        let request = SoftwareListRequest::from_json(json_request).expect("Failed to deserialize");

        assert_eq!(request.id, "123");
    }

    #[test]
    fn creating_a_software_list_response() {
        let request = SoftwareListRequest::new_with_id("1");
        let mut response = SoftwareListResponse::new(&request);

        response.add_modules(
            "debian",
            vec![
                SoftwareModule {
                    module_type: Some("debian".to_string()),
                    name: "a".to_string(),
                    version: None,
                    url: None,
                },
                SoftwareModule {
                    module_type: Some("debian".to_string()),
                    name: "b".to_string(),
                    version: Some("1.0".to_string()),
                    url: None,
                },
                SoftwareModule {
                    module_type: Some("debian".to_string()),
                    name: "c".to_string(),
                    version: None,
                    url: Some("https://foobar.io/c.deb".to_string()),
                },
                SoftwareModule {
                    module_type: Some("debian".to_string()),
                    name: "d".to_string(),
                    version: Some("beta".to_string()),
                    url: Some("https://foobar.io/d.deb".to_string()),
                },
            ],
        );

        response.add_modules(
            "apama",
            vec![SoftwareModule {
                module_type: Some("apama".to_string()),
                name: "m".to_string(),
                version: None,
                url: Some("https://foobar.io/m.epl".to_string()),
            }],
        );

        let expected_json = r#"{
            "id":"1",
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
        let actual_json = response.to_json().expect("Failed to serialize");
        assert_eq!(actual_json, remove_whitespace(expected_json));
    }

    #[test]
    fn using_a_software_list_response() {
        let json_response = r#"{
            "id": "123",
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

        let response =
            SoftwareListResponse::from_json(json_response).expect("Failed to deserialize");

        assert_eq!(response.id(), "123");
        assert_eq!(response.status(), SoftwareOperationStatus::Successful);
        assert_eq!(response.error(), None);

        // The mapper can use then the current list of modules
        assert_eq!(
            response.modules(),
            vec![
                SoftwareModule {
                    module_type: Some("debian".to_string()),
                    name: "a".to_string(),
                    version: None,
                    url: None
                },
                SoftwareModule {
                    module_type: Some("debian".to_string()),
                    name: "b".to_string(),
                    version: Some("1.0".to_string()),
                    url: None
                },
                SoftwareModule {
                    module_type: Some("debian".to_string()),
                    name: "c".to_string(),
                    version: None,
                    url: Some("https://foobar.io/c.deb".to_string())
                },
                SoftwareModule {
                    module_type: Some("debian".to_string()),
                    name: "d".to_string(),
                    version: Some("beta".to_string()),
                    url: Some("https://foobar.io/d.deb".to_string())
                },
                SoftwareModule {
                    module_type: Some("apama".to_string()),
                    name: "m".to_string(),
                    version: None,
                    url: Some("https://foobar.io/m.epl".to_string())
                },
            ]
        );
    }

    #[test]
    fn creating_a_software_list_error() {
        let request = SoftwareListRequest::new_with_id("123");
        let mut response = SoftwareListResponse::new(&request);

        response.set_error("Request_timed-out");

        let expected_json = r#"{
            "id": "123",
            "status": "failed",
            "reason": "Request_timed-out"
        }"#;

        let actual_json = response.to_json().expect("Failed to serialize");
        assert_eq!(actual_json, remove_whitespace(expected_json));
    }

    #[test]
    fn using_a_software_list_error() {
        let json_response = r#"{
            "id": "123",
            "status": "failed",
            "reason": "Request timed-out"
        }"#;
        let response =
            SoftwareListResponse::from_json(json_response).expect("Failed to deserialize");

        assert_eq!(response.id(), "123");
        assert_eq!(response.status(), SoftwareOperationStatus::Failed);
        assert_eq!(response.error(), Some("Request timed-out".into()));
        assert_eq!(response.modules(), vec![]);
    }

    #[test]
    fn creating_a_software_update_request() {
        let mut request = SoftwareUpdateRequest::new_with_id("123");

        request.add_updates(
            "debian",
            vec![
                SoftwareModuleUpdate::install(SoftwareModule {
                    module_type: Some("debian".to_string()),
                    name: "nodered".to_string(),
                    version: Some("1.0.0".to_string()),
                    url: None,
                }),
                SoftwareModuleUpdate::install(SoftwareModule {
                    module_type: Some("debian".to_string()),
                    name: "collectd".to_string(),
                    version: Some("5.7".to_string()),
                    url: Some(
                        "https://collectd.org/download/collectd-tarballs/collectd-5.12.0.tar.bz2"
                            .to_string(),
                    ),
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
                }),
                SoftwareModuleUpdate::remove(SoftwareModule {
                    module_type: Some("docker".to_string()),
                    name: "mongodb".to_string(),
                    version: Some("4.4.6".to_string()),
                    url: None,
                }),
            ],
        );

        let expected_json = r#"{
            "id": "123",
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
        let actual_json = request.to_json().expect("Failed to serialize");
        assert_eq!(actual_json, remove_whitespace(expected_json));
    }

    #[test]
    fn creating_a_software_update_request_grouping_updates_per_plugin() {
        let mut request = SoftwareUpdateRequest::new_with_id("123");

        request.add_update(SoftwareModuleUpdate::install(SoftwareModule {
            module_type: Some("debian".to_string()),
            name: "nodered".to_string(),
            version: Some("1.0.0".to_string()),
            url: None,
        }));
        request.add_update(SoftwareModuleUpdate::install(SoftwareModule {
            module_type: Some("docker".to_string()),
            name: "nginx".to_string(),
            version: Some("1.21.0".to_string()),
            url: None,
        }));
        request.add_update(SoftwareModuleUpdate::install(SoftwareModule {
            module_type: Some("debian".to_string()),
            name: "collectd".to_string(),
            version: Some("5.7".to_string()),
            url: Some(
                "https://collectd.org/download/collectd-tarballs/collectd-5.12.0.tar.bz2"
                    .to_string(),
            ),
        }));
        request.add_update(SoftwareModuleUpdate::remove(SoftwareModule {
            module_type: Some("docker".to_string()),
            name: "mongodb".to_string(),
            version: Some("4.4.6".to_string()),
            url: None,
        }));

        let expected_json = r#"{
            "id": "123",
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
        let actual_json = request.to_json().expect("Failed to serialize");
        assert_eq!(actual_json, remove_whitespace(expected_json));
    }

    #[test]
    fn creating_a_software_update_request_grouping_updates_per_plugin_using_default() {
        let mut request = SoftwareUpdateRequest::new_with_id("123");

        request.add_update(SoftwareModuleUpdate::install(SoftwareModule {
            module_type: None, // I.e. default
            name: "nodered".to_string(),
            version: Some("1.0.0".to_string()),
            url: None,
        }));
        request.add_update(SoftwareModuleUpdate::install(SoftwareModule {
            module_type: Some("".to_string()), // I.e. default
            name: "nginx".to_string(),
            version: Some("1.21.0".to_string()),
            url: None,
        }));
        request.add_update(SoftwareModuleUpdate::install(SoftwareModule {
            module_type: Some("default".to_string()), // I.e. default
            name: "collectd".to_string(),
            version: Some("5.7".to_string()),
            url: None,
        }));
        request.add_update(SoftwareModuleUpdate::remove(SoftwareModule {
            module_type: Some("debian".to_string()), // Unless specified otherwise, this is not the default
            name: "mongodb".to_string(),
            version: Some("4.4.6".to_string()),
            url: None,
        }));

        let expected_json = r#"{
            "id": "123",
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
        let actual_json = request.to_json().expect("Failed to serialize");
        assert_eq!(actual_json, remove_whitespace(expected_json));
    }

    #[test]
    fn creating_a_software_update_request_with_generated_id() {
        let request = SoftwareUpdateRequest::new();
        let generated_id = request.id;

        // The generated id is a nanoid of 21 characters from A-Za-z0-9_~
        let re = Regex::new(r"[A-Za-z0-9_~-]{21,21}").unwrap();
        assert!(re.is_match(&generated_id));
    }

    #[test]
    fn using_a_software_update_request() {
        let json_request = r#"{
            "id": "123",
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
        let request =
            SoftwareUpdateRequest::from_json(json_request).expect("Failed to deserialize");

        assert_eq!(request.id, "123");

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
                }),
                SoftwareModuleUpdate::install(SoftwareModule {
                    module_type: Some("debian".to_string()),
                    name: "collectd".to_string(),
                    version: Some("5.7".to_string()),
                    url: Some(
                        "https://collectd.org/download/collectd-tarballs/collectd-5.12.0.tar.bz2"
                            .to_string(),
                    ),
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
                }),
                SoftwareModuleUpdate::remove(SoftwareModule {
                    module_type: Some("docker".to_string()),
                    name: "mongodb".to_string(),
                    version: Some("4.4.6".to_string()),
                    url: None,
                }),
            ]
        );
    }

    #[test]
    fn creating_a_software_update_response() {
        let request = SoftwareUpdateRequest::new_with_id("123");
        let response = SoftwareUpdateResponse::new(&request);

        let expected_json = r#"{
            "id": "123",
            "status": "executing"
        }"#;

        let actual_json = response.to_json().expect("Failed to serialize");
        assert_eq!(actual_json, remove_whitespace(expected_json));
    }

    #[test]
    fn using_a_software_update_executing_response() {
        let json_response = r#"{
            "id": "123",
            "status": "executing"
        }"#;
        let response =
            SoftwareUpdateResponse::from_json(json_response).expect("Failed to deserialize");

        assert_eq!(response.id(), "123".to_string());
        assert_eq!(response.status(), SoftwareOperationStatus::Executing);
        assert_eq!(response.error(), None);
        assert_eq!(response.modules(), vec![]);
    }

    #[test]
    fn finalizing_a_software_update_response() {
        let request = SoftwareUpdateRequest::new_with_id("123");
        let mut response = SoftwareUpdateResponse::new(&request);

        response.add_modules(
            "debian",
            vec![
                SoftwareModule {
                    module_type: Some("debian".to_string()),
                    name: "nodered".to_string(),
                    version: Some("1.0.0".to_string()),
                    url: None,
                },
                SoftwareModule {
                    module_type: Some("debian".to_string()),
                    name: "collectd".to_string(),
                    version: Some("5.7".to_string()),
                    url: None,
                },
            ],
        );

        response.add_modules(
            "docker",
            vec![
                SoftwareModule {
                    module_type: Some("docker".to_string()),
                    name: "nginx".to_string(),
                    version: Some("1.21.0".to_string()),
                    url: None,
                },
                SoftwareModule {
                    module_type: Some("docker".to_string()),
                    name: "mongodb".to_string(),
                    version: Some("4.4.6".to_string()),
                    url: None,
                },
            ],
        );

        let expected_json = r#"{
            "id": "123",
            "status": "successful",
            "currentSoftwareList": [
                {
                    "type": "debian",
                    "modules": [
                        {
                            "name": "nodered",
                            "version": "1.0.0"
                        },
                        {
                            "name": "collectd",
                            "version": "5.7"
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
            ]
        }"#;

        let actual_json = response.to_json().expect("Failed to serialize");
        assert_eq!(actual_json, remove_whitespace(expected_json));
    }

    #[test]
    fn finalizing_a_software_update_error() {
        let request = SoftwareUpdateRequest::new_with_id("123");
        let mut response = SoftwareUpdateResponse::new(&request);

        response.add_errors(
            "debian",
            vec![SoftwareError::Install {
                module: SoftwareModule {
                    module_type: Some("debian".to_string()),
                    name: "collectd".to_string(),
                    version: Some("5.7".to_string()),
                    url: None,
                },
                reason: "Network timeout".to_string(),
            }],
        );

        response.add_errors(
            "docker",
            vec![SoftwareError::Remove {
                module: SoftwareModule {
                    module_type: Some("docker".to_string()),
                    name: "mongodb".to_string(),
                    version: Some("4.4.6".to_string()),
                    url: None,
                },
                reason: "Other components dependent on it".to_string(),
            }],
        );

        response.add_modules(
            "debian",
            vec![SoftwareModule {
                module_type: Some("debian".to_string()),
                name: "nodered".to_string(),
                version: Some("1.0.0".to_string()),
                url: None,
            }],
        );

        response.add_modules(
            "docker",
            vec![
                SoftwareModule {
                    module_type: Some("docker".to_string()),
                    name: "nginx".to_string(),
                    version: Some("1.21.0".to_string()),
                    url: None,
                },
                SoftwareModule {
                    module_type: Some("docker".to_string()),
                    name: "mongodb".to_string(),
                    version: Some("4.4.6".to_string()),
                    url: None,
                },
            ],
        );

        let expected_json = r#"{
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

        let actual_json = response.to_json().expect("Failed to serialize");
        assert_eq!(
            remove_whitespace(&actual_json),
            remove_whitespace(expected_json)
        );
    }

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
            SoftwareUpdateResponse::from_json(json_response).expect("Failed to deserialize");

        assert_eq!(response.id(), "123");
        assert_eq!(response.status(), SoftwareOperationStatus::Failed);
        assert_eq!(
            response.error(),
            Some("2 errors: fail to install [ collectd ] fail to remove [ mongodb ]".into())
        );

        // The C8Y mapper doesn't use the failures list
        // => no support for now

        // The mapper can request the updated list of modules
        assert_eq!(
            response.modules(),
            vec![
                SoftwareModule {
                    module_type: Some("debian".to_string()),
                    name: "nodered".to_string(),
                    version: Some("1.0.0".to_string()),
                    url: None
                },
                SoftwareModule {
                    module_type: Some("docker".to_string()),
                    name: "nginx".to_string(),
                    version: Some("1.21.0".to_string()),
                    url: None
                },
                SoftwareModule {
                    module_type: Some("docker".to_string()),
                    name: "mongodb".to_string(),
                    version: Some("4.4.6".to_string()),
                    url: None
                },
            ]
        );
    }

    fn remove_whitespace(s: &str) -> String {
        let mut s = String::from(s);
        s.retain(|c| !c.is_whitespace());
        s
    }
}
