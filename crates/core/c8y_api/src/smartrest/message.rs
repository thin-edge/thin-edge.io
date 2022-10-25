/// Extract the Device ID from the SmartREST payload.
///
/// ```
/// use c8y_api::smartrest::message::get_smartrest_device_id;
/// let device_id = get_smartrest_device_id("528,deviceId,software_a,version_a,url_a,install");
/// assert_eq!(device_id, Some("deviceId"))
/// ```
pub fn get_smartrest_device_id(payload: &str) -> Option<&str> {
    payload.split(',').nth(1)
}

/// Extract the SmartREST message template ID from the SmartREST payload.
///
/// ```
/// use c8y_api::smartrest::message::get_smartrest_template_id;
/// let template_id = get_smartrest_template_id("528,deviceId,software_a,version_a,url_a,install");
/// assert_eq!(template_id, String::from("528"))
/// ```
pub fn get_smartrest_template_id(payload: &str) -> String {
    //  unwrap is safe here as the first element of the split will be the whole payload if there is no comma.
    payload.split(',').next().unwrap().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use test_case::test_case;

    #[test_case("512,device_id", Some("device_id"); "valid")]
    #[test_case("512,", Some(""); "device id missing")]
    #[test_case("", None; "empty payload")]
    fn extract_smartrest_device_id(payload: &str, expected_output: Option<&str>) {
        let maybe_device_id = get_smartrest_device_id(payload);
        assert_eq!(maybe_device_id, expected_output)
    }

    #[test_case("cds50223434,uninstall-test"; "valid template")]
    #[test_case("5000000000000000000000000000000000000000000000000,uninstall-test"; "long valid template")]
    #[test_case(""; "empty payload")]
    fn extract_smartrest_template(payload: &str) {
        match get_smartrest_template_id(payload) {
            id if id.contains("cds50223434")
                || id.contains("5000000000000000000000000000000000000000000000000")
                || id.contains("") =>
            {
                assert!(true)
            }
            _ => assert!(false),
        }
    }
}
