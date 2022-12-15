use tracing::error;

// The actual limit defined by c8y is 16184 including header and body.
// We need a buffer, therefore here sets smaller size than the actual limit.
pub const MAX_PAYLOAD_LIMIT_IN_BYTES: usize = 16000;

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

/// Sanitize the input to be SmartREST compatible.
/// If the input contains invalid UTF-8, it returns an empty String.
/// - Remove all control characters except for `\n`, `\t`, `\r`.
/// - Double quote is escaped as `\"`.
/// - Strip the input according to `max_size`.
pub fn sanitize_for_smartrest(input: Vec<u8>, max_size: usize) -> String {
    String::from_utf8(input)
        .unwrap_or_else(|err| {
            error!("The input contains invalid UTF-8: {err}");
            String::default()
        })
        .chars()
        .filter(|&c| c == '\r' || c == '\n' || c == '\t' || !c.is_control())
        .collect::<String>()
        .replace('"', "\"\"")
        .chars()
        .scan(0, |bytes_count, c| {
            *bytes_count += c.len_utf8();
            Some((*bytes_count, c))
        })
        .take_while(|(size, _)| size <= &max_size)
        .map(|(_, char)| char)
        .collect()
}

/// Get the last line of input.
/// If the input is empty or contains invalid UTF-8, it returns an empty String.
/// The last line is ensured to be SmartREST compatible.
pub fn get_last_line_for_smartrest(input: Vec<u8>, max_size: usize) -> String {
    let input_string = String::from_utf8(input).unwrap_or_else(|err| {
        error!("The input contains invalid UTF-8: {err}");
        String::default()
    });
    let last_line = input_string.lines().last().unwrap_or_default();
    sanitize_for_smartrest(last_line.as_bytes().to_vec(), max_size)
}

#[cfg(test)]
mod tests {
    use super::*;
    use regex::Regex;
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

    #[test]
    fn selected_control_chars_remain() {
        let input = vec![0x00, 0x09, 0x0A, 0x0D]; // NULL, \t, \n, \r
        let sanitized = sanitize_for_smartrest(input, MAX_PAYLOAD_LIMIT_IN_BYTES);
        assert_eq!(sanitized, "\t\n\r".to_string());
    }

    #[test]
    fn control_chars_are_removed() {
        let input = generate_test_vec_u8();
        let sanitized = sanitize_for_smartrest(input, MAX_PAYLOAD_LIMIT_IN_BYTES);
        let re = Regex::new(r"[^\x20-\x7E\xA0-\xFF\t\n\r]").unwrap();
        assert!(!re.is_match(&sanitized));
    }

    #[test]
    fn invalid_utf8_is_contained() {
        let invalid_sparkle_heart = vec![0, 159, 146, 150];
        let sanitized = sanitize_for_smartrest(invalid_sparkle_heart, MAX_PAYLOAD_LIMIT_IN_BYTES);
        assert_eq!(sanitized, "".to_string());
    }

    #[test]
    fn invalid_utf8_is_contained_last_line() {
        let invalid_sparkle_heart = vec![0, 159, 146, 150];
        let last_line =
            get_last_line_for_smartrest(invalid_sparkle_heart, MAX_PAYLOAD_LIMIT_IN_BYTES);
        assert_eq!(last_line, "".to_string());
    }

    #[test_case("foo bar baz\n", "foo bar baz\n"; "standard")]
    #[test_case("foo\r\nbar\tbaz\0\"", "foo\r\nbar\tbaz\"\""; "with control chars")]
    #[test_case("baz", "baz"; "no new line")]
    #[test_case("", ""; "empty")]
    #[test_case("こんにちは", "こんにちは"; "no ascii")]
    #[test_case("こんにちは\"\n\0こんにちは", "こんにちは\"\"\nこんにちは"; "no ascii and control chars")]
    fn u8_vec_is_sanitized(input: &str, expected_output: &str) {
        let vec_u8 = input.as_bytes().to_vec();
        let sanitized = sanitize_for_smartrest(vec_u8, MAX_PAYLOAD_LIMIT_IN_BYTES);
        assert_eq!(sanitized, expected_output.to_string());
    }

    #[test_case("Hello World", "Hello Worl"; "ascii")]
    #[test_case("こんにちは", "こんに"; "no ascii")]
    fn size_u8_vec_is_stripped(input: &str, expected_output: &str) {
        let vec_u8 = input.as_bytes().to_vec();
        let stripped = sanitize_for_smartrest(vec_u8, 10);
        assert_eq!(stripped, expected_output.to_string());
    }

    #[test_case("foo\r\nbar\n\nbaz\n", "baz"; "standard")]
    #[test_case("baz", "baz"; "no new line")]
    #[test_case("", ""; "empty")]
    #[test_case("おはよう\nこんにちは\n", "こんに"; "no ascii")]
    fn return_last_line_of_vec_u8(input: &str, expected_output: &str) {
        let vec_u8 = input.as_bytes().to_vec();
        let last_line = get_last_line_for_smartrest(vec_u8, 10);
        assert_eq!(last_line.as_str(), expected_output);
    }

    fn generate_test_vec_u8() -> Vec<u8> {
        let mut vec: Vec<u8> = Vec::new();
        for i in 0x00..0xff {
            vec.push(u8::from(i))
        }
        vec
    }
}
