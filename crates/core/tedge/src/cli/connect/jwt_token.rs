use crate::cli::connect::ConnectError;
use crate::cli::connect::CONNECTION_TIMEOUT;
use crate::cli::connect::RESPONSE_TIMEOUT;
use anyhow::anyhow;
use base64::prelude::*;
use rumqttc::Event;
use rumqttc::Incoming;
use rumqttc::Outgoing;
use rumqttc::Packet;
use rumqttc::QoS::AtLeastOnce;
use tedge_config::TEdgeConfig;

pub(crate) async fn get_connected_c8y_url(
    tedge_config: &TEdgeConfig,
    c8y_prefix: Option<&str>,
) -> Result<String, ConnectError> {
    let prefix = &tedge_config.c8y.try_get(c8y_prefix)?.bridge.topic_prefix;
    let c8y_topic_builtin_jwt_token_upstream = format!("{prefix}/s/uat");
    let c8y_topic_builtin_jwt_token_downstream = format!("{prefix}/s/dat");
    const CLIENT_ID: &str = "get_jwt_token_c8y";

    let mut mqtt_options = tedge_config
        .mqtt_config()?
        .with_session_name(CLIENT_ID)
        .with_clean_session(true)
        .rumqttc_options()?;
    mqtt_options.set_keep_alive(RESPONSE_TIMEOUT);

    let (client, mut event_loop) = rumqttc::AsyncClient::new(mqtt_options, 10);
    event_loop
        .network_options
        .set_connection_timeout(CONNECTION_TIMEOUT.as_secs());
    let mut acknowledged = false;
    let mut c8y_url: Option<String> = None;

    client
        .subscribe(c8y_topic_builtin_jwt_token_downstream, AtLeastOnce)
        .await?;
    let mut err = None;

    loop {
        match event_loop.poll().await {
            Ok(Event::Incoming(Packet::SubAck(_))) => {
                // We are ready to get the response, hence send the request
                client
                    .publish(
                        &c8y_topic_builtin_jwt_token_upstream,
                        rumqttc::QoS::AtMostOnce,
                        false,
                        "",
                    )
                    .await?;
            }
            Ok(Event::Incoming(Packet::PubAck(_))) => {
                // The request has been sent
                acknowledged = true;
            }
            Ok(Event::Incoming(Packet::Publish(response))) => {
                // We got a response
                let token = String::from_utf8(response.payload.to_vec()).unwrap();
                let connected_url = decode_jwt_token(token.as_str())?;
                c8y_url = Some(connected_url);
                break;
            }
            Ok(Event::Outgoing(Outgoing::PingReq)) => {
                let rest = if acknowledged {
                    // The request has been sent but without a response
                    "The request has been sent, however, no response was received"
                } else {
                    // The request has not even been sent
                    "Make sure mosquitto is running."
                };
                // No messages have been received for a while
                err = Some(anyhow!("Timed out obtaining Cumulocity URL. {rest}"));
                break;
            }
            Ok(Event::Incoming(Incoming::Disconnect)) => {
                err = Some(anyhow!(
                    "Client was disconnected from mosquitto while obtaining Cumulocity URL."
                ));
                break;
            }
            Err(e) => {
                err = Some(anyhow::Error::from(e).context(
                    "Client failed to connect to mosquitto while obtaining Cumulocity URL",
                ));
                break;
            }
            _ => {}
        }
    }

    // Cleanly disconnect client
    client.disconnect().await?;
    loop {
        match event_loop.poll().await {
            Ok(Event::Outgoing(Outgoing::Disconnect)) | Err(_) => break,
            _ => {}
        }
    }

    if let Some(c8y_url) = c8y_url {
        Ok(c8y_url)
    } else {
        Err(err.map_or(ConnectError::TimeoutElapsedError, Into::into))
    }
}

pub(crate) fn decode_jwt_token(token: &str) -> Result<String, ConnectError> {
    // JWT token format: <header>.<payload>.<signature>. Thus, we want only <payload>.
    let payload = token
        .split_terminator('.')
        .nth(1)
        .ok_or(ConnectError::InvalidJWTToken {
            token: token.to_string(),
            reason: "JWT token format must be <header>.<payload>.<signature>.".to_string(),
        })?;

    let decoded =
        BASE64_STANDARD_NO_PAD
            .decode(payload)
            .map_err(|_| ConnectError::InvalidJWTToken {
                token: token.to_string(),
                reason: "Cannot decode the payload of JWT token by Base64 without padding."
                    .to_string(),
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

    #[test]
    fn check_decode_jwt_token_missing_base64_padding() {
        // JWTs don't pad base64-encoded strings to make them more compact. This
        // JWT has a 215 byte payload, so if our parsing disallows non-padded
        // input (base64 that isn't a multiple of 4 bytes long), we will fail on this valid JWT
        let token = "eyJ0eXAiOiJKV1QiLCJhbGciOiJIUzI1NiJ9.eyJpc3MiOiJ0ZXN0LmN1bXVsb2NpdHkuY29tIiwiaWF0IjoxNzQyMjI3NjY4LCJleHAiOjE3NDIyMzEyNjgsImF1ZCI6InRlc3QuY3VtdWxvY2l0eS5jb20iLCJzdWIiOiJkZXZpY2VfdGVzdDAwNSIsIm5iZiI6IjE3NDIyMjc2NjgiLCJ0Y2kiOiJkZXZpY2VfdG9rZW5fY29uZmlnIn0.JgoTORxZk8LN51e9-gHzfpr59JlaIT5oHFXuGQxP2zY";
        let expected_url = "test.cumulocity.com";
        assert_eq!(decode_jwt_token(token).unwrap(), expected_url.to_string());
    }

    #[test_case(
    "dGVzdC5jdW11bG9jaXR5LmNvbQ",
    "The JWT token received from Cumulocity is invalid.\n\
    Token: dGVzdC5jdW11bG9jaXR5LmNvbQ\n\
    Reason: JWT token format must be <header>.<payload>.<signature>."
    ; "not jwt token"
    )]
    #[test_case(
    "aaa.bbb.ccc",
    "The JWT token received from Cumulocity is invalid.\n\
    Token: aaa.bbb.ccc\n\
    Reason: Cannot decode the payload of JWT token by Base64 without padding."
    ; "payload is not base64 encoded"
    )]
    #[test_case(
    "aaa.eyJpc3MiOiJ0ZXN0LmN1bXVsb2NpdHkuY29tIn0=.ccc",
    "The JWT token received from Cumulocity is invalid.\n\
    Token: aaa.eyJpc3MiOiJ0ZXN0LmN1bXVsb2NpdHkuY29tIn0=.ccc\n\
    Reason: Cannot decode the payload of JWT token by Base64 without padding."
    ; "payload has base64 padding"
    )]
    #[test_case(
    "aaa.dGVzdC5jdW11bG9jaXR5LmNvbQ.ccc",
    "The JWT token received from Cumulocity is invalid.\n\
    Token: aaa.dGVzdC5jdW11bG9jaXR5LmNvbQ.ccc\n\
    Reason: The payload of JWT token is not JSON."
    ; "payload is not json"
    )]
    #[test_case(
    "aaa.eyJqdGkiOm51bGwsImF1ZCI6InRlc3QuY3VtdWxvY2l0eS5jb20iLCJzdWIiOiJkZXZpY2VfdGVzdDAwMDUiLCJ0Y2kiOiJkZXZpY2VfdG9rZW5fY29uZmlnIiwiaWF0IjoxNjM4NDQyOTk3LCJuYmYiOjE2Mzg0NDI5OTcsImV4cCI6MTYzODQ0NjU5NywidGZhIjpmYWxzZSwidGVuIjoidDMxNzA0OCIsInhzcmZUb2tlbiI6IktzZUFVRkFMYXVpSmVkUU1HZnNGIn0.ccc",
    "The JWT token received from Cumulocity is invalid.\n\
    Token: aaa.eyJqdGkiOm51bGwsImF1ZCI6InRlc3QuY3VtdWxvY2l0eS5jb20iLCJzdWIiOiJkZXZpY2VfdGVzdDAwMDUiLCJ0Y2kiOiJkZXZpY2VfdG9rZW5fY29uZmlnIiwiaWF0IjoxNjM4NDQyOTk3LCJuYmYiOjE2Mzg0NDI5OTcsImV4cCI6MTYzODQ0NjU5NywidGZhIjpmYWxzZSwidGVuIjoidDMxNzA0OCIsInhzcmZUb2tlbiI6IktzZUFVRkFMYXVpSmVkUU1HZnNGIn0.ccc\n\
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
