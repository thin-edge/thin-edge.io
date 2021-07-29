mod error;
mod messages;
mod software;

pub use error::*;
pub use software::SoftwareModule;
pub use messages::{
    Jsonify,
    SoftwareListRequest,
    SoftwareListResponse,
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
                {"type":"debian", "list":[
                    {"name":"a"},
                    {"name":"b","version":"1.0"},
                    {"name":"c","url":"https://foobar.io/c.deb"},
                    {"name":"d","version":"beta","url":"https://foobar.io/d.deb"}
                ]},
                {"type":"apama","list":[
                    {"name":"m","url":"https://foobar.io/m.epl"}
                ]}
            ]}"#;
        let actual_json = response.to_json().expect("Failed to serialize");
        assert_eq!(actual_json, remove_whitespace(expected_json));
    }

    fn remove_whitespace(s: &str) -> String {
        let mut s = String::from(s);
        s.retain(|c| !c.is_whitespace());
        s
    }
}
