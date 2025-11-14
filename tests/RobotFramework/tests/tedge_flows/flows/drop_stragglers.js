// Reject any message that is too old, too new or with no timestamp
export function onMessage (message, config) {
    let payload = JSON.parse(message.payload)
    let msg_time = payload.time
    if (!msg_time) {
        return []
    }

    let msg_timestamp = msg_time
    if (typeof(msg_time) === "string") {
        msg_timestamp = Date.parse(msg_time) / 1e3
    }

    let timestamp = message.timestamp
    let max = time + (config.max_advance || 1);
    let min = time - (config.max_delay || 10);

    if (min <= msg_timestamp && msg_timestamp <= max) {
        return [message]
    } else {
        return [{"topic":" te/error", "payload":`straggler rejected on ${message.topic} with time=${msg_timestamp} at ${time}`}]
    }
}
