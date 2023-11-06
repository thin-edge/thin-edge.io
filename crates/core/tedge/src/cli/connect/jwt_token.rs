use crate::cli::connect::ConnectError;
use crate::cli::connect::CONNECTION_TIMEOUT;
use crate::cli::connect::RESPONSE_TIMEOUT;
use rumqttc::Event;
use rumqttc::Incoming;
use rumqttc::Outgoing;
use rumqttc::Packet;
use rumqttc::QoS::AtLeastOnce;
use tedge_config::TEdgeConfig;

pub(crate) fn get_connected_c8y_url(tedge_config: &TEdgeConfig) -> Result<String, ConnectError> {
    const C8Y_TOPIC_BUILTIN_JWT_TOKEN_UPSTREAM: &str = "c8y/s/uat";
    const C8Y_TOPIC_BUILTIN_JWT_TOKEN_DOWNSTREAM: &str = "c8y/s/dat";
    const CLIENT_ID: &str = "get_jwt_token_c8y";

    let mut mqtt_options = tedge_config
        .mqtt_config()?
        .with_session_name(CLIENT_ID)
        .with_clean_session(true)
        .rumqttc_options()?;
    mqtt_options.set_keep_alive(RESPONSE_TIMEOUT);

    let (mut client, mut connection) = rumqttc::Client::new(mqtt_options, 10);
    connection
        .eventloop
        .network_options
        .set_connection_timeout(CONNECTION_TIMEOUT.as_secs());
    let mut acknowledged = false;

    client.subscribe(C8Y_TOPIC_BUILTIN_JWT_TOKEN_DOWNSTREAM, AtLeastOnce)?;

    for event in connection.iter() {
        match event {
            Ok(Event::Incoming(Packet::SubAck(_))) => {
                // We are ready to get the response, hence send the request
                client.publish(
                    C8Y_TOPIC_BUILTIN_JWT_TOKEN_UPSTREAM,
                    rumqttc::QoS::AtMostOnce,
                    false,
                    "",
                )?;
            }
            Ok(Event::Incoming(Packet::PubAck(_))) => {
                // The request has been sent
                acknowledged = true;
            }
            Ok(Event::Incoming(Packet::Publish(response))) => {
                // We got a response
                let token = String::from_utf8(response.payload.to_vec()).unwrap();
                let connected_url = decode_jwt_token(token.as_str())?;
                return Ok(connected_url);
            }
            Ok(Event::Outgoing(Outgoing::PingReq)) => {
                // No messages have been received for a while
                println!("Local MQTT publish has timed out.");
                break;
            }
            Ok(Event::Incoming(Incoming::Disconnect)) => {
                eprintln!("ERROR: Disconnected");
                break;
            }
            Err(err) => {
                eprintln!("ERROR: {:?}", err);
                break;
            }
            _ => {}
        }
    }

    if acknowledged {
        // The request has been sent but without a response
        println!("\nThe request has been sent, however, no response.");
        Err(ConnectError::TimeoutElapsedError)
    } else {
        // The request has not even been sent
        println!("\nMake sure mosquitto is running.");
        Err(ConnectError::TimeoutElapsedError)
    }
}

pub(crate) fn decode_jwt_token(token: &str) -> Result<String, ConnectError> {
    // JWT token format: <header>.<payload>.<signature>. Thus, we want only <paylaod>.
    let payload = token
        .split_terminator('.')
        .nth(1)
        .ok_or(ConnectError::InvalidJWTToken {
            token: token.to_string(),
            reason: "JWT token format must be <header>.<payload>.<signature>.".to_string(),
        })?;

    let decoded = base64::decode(payload).map_err(|_| ConnectError::InvalidJWTToken {
        token: token.to_string(),
        reason: "Cannot decode the payload of JWT token by Base64.".to_string(),
    })?;

    let json: serde_json::Value =
        serde_json::from_slice(decoded.as_slice()).map_err(|_| ConnectError::InvalidJWTToken {
            token: token.to_string(),
            reason: "The payload of JWT token is not JSON.".to_string(),
        })?;

    let tenant_url = json["iss"].as_str().ok_or(ConnectError::InvalidJWTToken {
        token: token.to_string(),
        reason: "The JSON decoded from JWT token doesn't contain 'iss' field.".to_string(),
    })?;

    Ok(tenant_url.to_string())
}

#[cfg(test)]
mod test {
    use super::*;
    use test_case::test_case;

    #[test]
    fn check_decode_valid_jwt_token() {
        let token = "eyJhbGciOiJSUzI1NiIsInR5cCI6IkpXVCJ9.eyJqdGkiOm51bGwsImlzcyI6InRlc3QuY3VtdWxvY2l0eS5jb20iLCJhdWQiOiJ0ZXN0LmN1bXVsb2NpdHkuY29tIiwic3ViIjoiZGV2aWNlX3Rlc3QwMDA1IiwidGNpIjoiZGV2aWNlX3Rva2VuX2NvbmZpZyIsImlhdCI6MTYzODQ0Mjk5NywibmJmIjoxNjM4NDQyOTk3LCJleHAiOjE2Mzg0NDY1OTcsInRmYSI6ZmFsc2UsInRlbiI6InQzMTcwNDgiLCJ4c3JmVG9rZW4iOiJLc2VBVUZBTGF1aUplZFFNR2ZzRiJ9.JUYtU9FVWlOWUPJXawFzKNiHD4HoEEWmvKdU1k9L2UF2ofRA2zAdcLH4mxaaspt4suyyZbPL6cS6c9MROG3YCsnqle2NSoYw8mxqncFECWsDS8lwCRTG4402iPTETfWpo9uXw2pFryBoJMAvNzt1qsXXn8EXSYxjzgj0YyxSANypm7PL1kMaprdLuUML_9Cwxf7Z6CRyWkZWWmnQ3lYgV5KMGW7HznkkqcmUCvuXKrHhVL5RkmzE1WyL4ndpGEPFEv9VYmEvFYA8wVHSuw5iVZIFp5lQldDdy_8U-N80xnf3fqZ6Q_wnVm8cga77vIgcf9zK5rSCdehvolM48uM4_w";
        let expected_url = "test.cumulocity.com";
        assert_eq!(decode_jwt_token(token).unwrap(), expected_url.to_string());
    }

    #[test_case(
    "dGVzdC5jdW11bG9jaXR5LmNvbQ==",
    "The JWT token received from Cumulocity is invalid.\n\
    Token: dGVzdC5jdW11bG9jaXR5LmNvbQ==\n\
    Reason: JWT token format must be <header>.<payload>.<signature>."
    ; "not jwt token"
    )]
    #[test_case(
    "aaa.bbb.ccc",
    "The JWT token received from Cumulocity is invalid.\n\
    Token: aaa.bbb.ccc\n\
    Reason: Cannot decode the payload of JWT token by Base64."
    ; "payload is not base64 encoded"
    )]
    #[test_case(
    "aaa.dGVzdC5jdW11bG9jaXR5LmNvbQ==.ccc",
    "The JWT token received from Cumulocity is invalid.\n\
    Token: aaa.dGVzdC5jdW11bG9jaXR5LmNvbQ==.ccc\n\
    Reason: The payload of JWT token is not JSON."
    ; "payload is not json"
    )]
    #[test_case(
    "aaa.eyJqdGkiOm51bGwsImF1ZCI6InRlc3QuY3VtdWxvY2l0eS5jb20iLCJzdWIiOiJkZXZpY2VfdGVzdDAwMDUiLCJ0Y2kiOiJkZXZpY2VfdG9rZW5fY29uZmlnIiwiaWF0IjoxNjM4NDQyOTk3LCJuYmYiOjE2Mzg0NDI5OTcsImV4cCI6MTYzODQ0NjU5NywidGZhIjpmYWxzZSwidGVuIjoidDMxNzA0OCIsInhzcmZUb2tlbiI6IktzZUFVRkFMYXVpSmVkUU1HZnNGIn0=.ccc",
    "The JWT token received from Cumulocity is invalid.\n\
    Token: aaa.eyJqdGkiOm51bGwsImF1ZCI6InRlc3QuY3VtdWxvY2l0eS5jb20iLCJzdWIiOiJkZXZpY2VfdGVzdDAwMDUiLCJ0Y2kiOiJkZXZpY2VfdG9rZW5fY29uZmlnIiwiaWF0IjoxNjM4NDQyOTk3LCJuYmYiOjE2Mzg0NDI5OTcsImV4cCI6MTYzODQ0NjU5NywidGZhIjpmYWxzZSwidGVuIjoidDMxNzA0OCIsInhzcmZUb2tlbiI6IktzZUFVRkFMYXVpSmVkUU1HZnNGIn0=.ccc\n\
    Reason: The JSON decoded from JWT token doesn't contain 'iss' field."
    ; "payload is json but not contains iss field"
    )]
    fn check_decode_invalid_jwt_token(input: &str, expected_error_msg: &str) {
        match decode_jwt_token(input) {
            Ok(_) => panic!("This test should result in an error"),
            Err(err) => {
                let error_msg = format!("{}", err);
                assert_eq!(error_msg, expected_error_msg)
            }
        }
    }
}
