mod cli;
mod list;
mod test;

use base64::prelude::*;
pub use cli::TEdgeFlowsCli;
use tedge_flows::flow::Message;

fn decode_message(
    topic: String,
    payload: String,
    base64_payload: bool,
) -> Result<Message, anyhow::Error> {
    let payload = if base64_payload {
        BASE64_STANDARD.decode(payload.as_bytes())?
    } else {
        payload.into_bytes()
    };
    Ok(Message::new(topic, payload))
}

fn encode_message(mut message: Message, base64_payload: bool) -> Message {
    if base64_payload {
        message.payload = BASE64_STANDARD.encode(message.payload).into_bytes();
    };

    message
}
