mod error;
mod messages;
mod software;

pub use error::*;
pub use software::*;
pub use messages::{
    Jsonify,
    SoftwareListRequest,
    SoftwareListResponse,
    SoftwareUpdateRequest,
    SoftwareUpdateResponse,
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn creating_a_software_list_request() {
        let request = SoftwareListRequest::new(1);

        let expected_json = r#"{"id":1}"#;
        let actual_json = request.to_json().expect("Failed to serialize");
        assert_eq!(actual_json, expected_json);
    }

    #[test]
    fn using_a_software_list_request() {
        let json_request = r#"{"id":123}"#;
        let request = SoftwareListRequest::from_json(json_request).expect("Failed to deserialize");

        assert_eq!(request.id, 123);
    }

    #[test]
    fn creating_a_software_list_response() {
        let request = SoftwareListRequest::new(1);
        let mut response = SoftwareListResponse::new(&request);

        response.add_modules("debian", vec![
            SoftwareModule { name: "a".to_string(), version: None, url: None },
            SoftwareModule { name: "b".to_string(), version: Some("1.0".to_string()), url: None },
            SoftwareModule { name: "c".to_string(), version: None, url: Some("https://foobar.io/c.deb".to_string()) },
            SoftwareModule { name: "d".to_string(), version: Some("beta".to_string()), url: Some("https://foobar.io/d.deb".to_string()) },
        ]);

        response.add_modules("apama", vec![
            SoftwareModule { name: "m".to_string(), version: None, url: Some("https://foobar.io/m.epl".to_string()) },
        ]);

        let expected_json = r#"{
            "id":1,
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
            "id": 123,
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

        let response = SoftwareListResponse::from_json(json_response).expect("Failed to deserialize");

        assert_eq!(response.id(), 123);
        assert_eq!(response.error(), None);
    }

    #[test]
    fn creating_a_software_list_error() {
        let request = SoftwareListRequest::new(123);
        let mut response = SoftwareListResponse::new(&request);

        response.set_error("Request_timed-out");

        let expected_json = r#"{
            "id": 123,
            "status": "failed",
            "reason": "Request_timed-out"
        }"#;

        let actual_json = response.to_json().expect("Failed to serialize");
        assert_eq!(actual_json, remove_whitespace(expected_json));
    }

    #[test]
    fn using_a_software_list_error() {
        let json_response = r#"{
            "id": 123,
            "status": "failed",
            "reason": "Request timed-out"
        }"#;
        let response = SoftwareListResponse::from_json(json_response).expect("Failed to deserialize");

        assert_eq!(response.id(), 123);
        assert_eq!(response.error(), Some("Request timed-out".into()));
    }

    #[test]
    fn creating_a_software_update_request() {
        let mut request = SoftwareUpdateRequest::new(123);

        request.add_updates("debian", vec![
            SoftwareModuleUpdate::install(SoftwareModule { name: "nodered".to_string(), version: Some("1.0.0".to_string()), url: None }),
            SoftwareModuleUpdate::install(SoftwareModule { name: "collectd".to_string(), version: Some("5.7".to_string()), url: Some("https://collectd.org/download/collectd-tarballs/collectd-5.12.0.tar.bz2".to_string()) }),
        ]);

        request.add_updates("docker", vec![
            SoftwareModuleUpdate::install(SoftwareModule { name: "nginx".to_string(), version: Some("1.21.0".to_string()), url: None }),
            SoftwareModuleUpdate::remove(SoftwareModule { name: "mongodb".to_string(), version: Some("4.4.6".to_string()), url: None }),
        ]);

        let expected_json = r#"{
            "id": 123,
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
    fn creating_a_software_update_response() {
        let request = SoftwareUpdateRequest::new(123);
        let response = SoftwareUpdateResponse::new(&request);

        let expected_json = r#"{
            "id": 123,
            "status": "executing"
        }"#;

        let actual_json = response.to_json().expect("Failed to serialize");
        assert_eq!(actual_json, remove_whitespace(expected_json));
    }

    #[test]
    fn finalizing_a_software_update_response() {
        let request = SoftwareUpdateRequest::new(123);
        let mut response = SoftwareUpdateResponse::new(&request);

        response.add_modules("debian", vec![
            SoftwareModule { name: "nodered".to_string(), version: Some("1.0.0".to_string()), url: None },
            SoftwareModule { name: "collectd".to_string(), version: Some("5.7".to_string()), url: None },
        ]);

        response.add_modules("docker", vec![
            SoftwareModule { name: "nginx".to_string(), version: Some("1.21.0".to_string()), url: None },
            SoftwareModule { name: "mongodb".to_string(), version: Some("4.4.6".to_string()), url: None },
        ]);

        let expected_json = r#"{
            "id": 123,
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
        let request = SoftwareUpdateRequest::new(123);
        let mut response = SoftwareUpdateResponse::new(&request);

        response.add_errors("debian", vec![
            SoftwareError::Install {
                module: SoftwareModule {
                    name: "collectd".to_string(),
                    version: Some("5.7".to_string()),
                    url: None },
                reason: "Network timeout".to_string(),
            },
        ]);

        response.add_errors("docker", vec![
            SoftwareError::Remove {
                module: SoftwareModule {
                    name: "mongodb".to_string(),
                    version: Some("4.4.6".to_string()),
                    url: None
                },
                reason: "Other components dependent on it".to_string(),
            },
        ]);

        response.add_modules("debian", vec![
            SoftwareModule { name: "nodered".to_string(), version: Some("1.0.0".to_string()), url: None },
        ]);

        response.add_modules("docker", vec![
            SoftwareModule { name: "nginx".to_string(), version: Some("1.21.0".to_string()), url: None },
            SoftwareModule { name: "mongodb".to_string(), version: Some("4.4.6".to_string()), url: None },
        ]);

        let expected_json = r#"{
            "id": 123,
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
        assert_eq!(remove_whitespace(&actual_json), remove_whitespace(expected_json));
    }

    fn remove_whitespace(s: &str) -> String {
        let mut s = String::from(s);
        s.retain(|c| !c.is_whitespace());
        s
    }
}
