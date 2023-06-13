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
/// - Double quote is escaped as `\"\"`.
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

/// Generate a failure reason of operation with the following format.
/// <Last line of input>
/// (line break)
/// <All records on input>
///
/// If the input has only one line, returns the line only.
/// If the input is empty or contains invalid UTF-8, it returns an empty String.
/// The output is ensured to be SmartREST compatible.
pub fn get_failure_reason_for_smartrest(input: Vec<u8>, max_size: usize) -> String {
    let input_string = String::from_utf8(input).unwrap_or_else(|err| {
        error!("The input contains invalid UTF-8: {err}");
        String::default()
    });
    let last_line = input_string.lines().last().unwrap_or_default();
    let failure_reason = match input_string.lines().count() {
        0 | 1 => last_line.to_string(),
        _ => format!("{}\n\n{}", last_line, input_string.as_str()),
    };
    sanitize_for_smartrest(failure_reason.as_bytes().to_vec(), max_size)
}

/// Split MQTT message payload to multiple SmartREST messages.
///
/// ```
/// use c8y_api::smartrest::message::collect_smartrest_messages;
/// let data = "511,device,echo hello\n511,device,\"echo hello\necho world\"";
/// let messages = collect_smartrest_messages(data);
/// assert_eq!(messages[0], "511,device,echo hello");
/// assert_eq!(messages[1], "511,device,\"echo hello\necho world\"");
/// ```
pub fn collect_smartrest_messages(data: &str) -> Vec<String> {
    let mut stack: Vec<char> = Vec::new();
    let mut smartrest_messages: Vec<String> = Vec::new();
    let mut is_inside = false; // Inside an outermost double quote block or not.
    let mut maybe_escaped = false; // The previous char is backslash or not.

    for c in data.chars() {
        if c == '\\' {
            stack.push(c);
            maybe_escaped = true;
        } else if c == '"' {
            if maybe_escaped {
                stack.pop(); // Remove the literal backslash to avoid the backslash also escaped.
                stack.push(c);
            } else {
                stack.push(c);
                if is_inside {
                    // End of the outermost block
                    is_inside = false;
                } else {
                    // Beginning of the outermost block
                    is_inside = true;
                }
            }
            maybe_escaped = false;
        } else if c == '\n' {
            if is_inside {
                // Still inside of the SmartREST message
                stack.push(c);
            } else {
                // End of the SmartREST message
                let message = stack.iter().collect();
                smartrest_messages.push(message);
                stack.clear();
            }
            maybe_escaped = false;
        } else {
            stack.push(c);
            maybe_escaped = false;
        }
    }

    // The last message doesn't end with `\n`.
    if !stack.is_empty() {
        let message = stack.iter().collect();
        smartrest_messages.push(message);
        stack.clear();
    }

    smartrest_messages
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
    #[test_case("106"; "106 but no child devices")]
    fn extract_smartrest_template(payload: &str) {
        let id = get_smartrest_template_id(payload);
        if !(id.contains("cds50223434")
            || id.contains("5000000000000000000000000000000000000000000000000")
            || id.contains("106")
            || id.contains(""))
        {
            panic!("invalid id");
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
        let input: Vec<u8> = (0x00..0xff).collect();
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
            get_failure_reason_for_smartrest(invalid_sparkle_heart, MAX_PAYLOAD_LIMIT_IN_BYTES);
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

    #[test_case("foo bar bye", "foo bar by"; "ascii")]
    #[test_case("こんにちは", "こんに"; "no ascii")]
    fn size_u8_vec_is_stripped(input: &str, expected_output: &str) {
        let vec_u8 = input.as_bytes().to_vec();
        let stripped = sanitize_for_smartrest(vec_u8, 10);
        assert_eq!(stripped, expected_output.to_string());
    }

    #[test_case("baz\n", "baz"; "one line")]
    #[test_case("baz", "baz"; "no new line")]
    #[test_case("foo\r\nbar\n\nbaz\n", "baz\n\nfoo\r\n"; "multiline")]
    #[test_case("", ""; "empty")]
    #[test_case("おはよう\nこんにちは\n", "こんに"; "no ascii")]
    #[test_case("あ\nい\nう\nえ\nお\n", "お\n\nあ\n"; "no ascii2")]
    fn return_formatted_text_for_failure_reason_from_vec_u8(input: &str, expected_output: &str) {
        let vec_u8 = input.as_bytes().to_vec();
        let last_line = get_failure_reason_for_smartrest(vec_u8, 10);
        assert_eq!(last_line.as_str(), expected_output);
    }

    #[test]
    fn split_single_smartrest_message() {
        let data = r#"528,DeviceSerial,softwareA,1.0,url1,install,softwareB,2.0,url2,install"#;
        let message = collect_smartrest_messages(data);
        assert_eq!(
            message[0],
            r#"528,DeviceSerial,softwareA,1.0,url1,install,softwareB,2.0,url2,install"#
        );
    }

    #[test]
    fn split_multiple_smartrest_messages() {
        let data = r#"511,device,echo hello
511,device,"echo hello
echo world"
511,device,"echo \"hello\""
511,device,"echo \"hello\"; echo \"world\""
511,device,echo hello; echo world
511,device,"echo hello,world"
511,device,"echo \\"hello\\""
528,DeviceSerial,softwareA,1.0,url1,install,softwareB,2.0,url2,install
524,DeviceSerial,http://www.my.url,type
524,DeviceSerial,"something",http://www.my.url,type
511,device,511,rina0005,echo \\\"#;

        let messages = collect_smartrest_messages(data);

        assert_eq!(messages[0], r#"511,device,echo hello"#);
        assert_eq!(
            messages[1],
            r#"511,device,"echo hello
echo world""#
        );
        assert_eq!(messages[2], r#"511,device,"echo "hello"""#);
        assert_eq!(messages[3], r#"511,device,"echo "hello"; echo "world"""#);
        assert_eq!(messages[4], r#"511,device,echo hello; echo world"#);
        assert_eq!(messages[5], r#"511,device,"echo hello,world""#);
        assert_eq!(messages[6], r#"511,device,"echo \"hello\"""#);
        assert_eq!(
            messages[7],
            r#"528,DeviceSerial,softwareA,1.0,url1,install,softwareB,2.0,url2,install"#
        );
        assert_eq!(messages[8], r#"524,DeviceSerial,http://www.my.url,type"#);
        assert_eq!(
            messages[9],
            r#"524,DeviceSerial,"something",http://www.my.url,type"#
        );
        assert_eq!(messages[10], r#"511,device,511,rina0005,echo \\\"#);
    }
}
