// Reject any message that is too old, too new or with no timestamp
export function process (timestamp, message, config) {
    let payload = JSON.parse(message.payload)
    let msg_time = payload.time
    if (!msg_time) {
        return []
    }
    if (!config) {
        config = {}
    }

    let msg_timestamp = msg_time
    if (typeof(msg_time) === "string") {
        msg_timestamp = Date.parse(msg_time) / 1e3
    }

    let time = timestamp.seconds + (timestamp.nanoseconds / 1e9)
    let max = time + (config.max || 1);
    let min = time - (config.min || 10);

    if (min <= msg_timestamp && msg_timestamp <= max) {
        return [message]
    } else {
        return [{"topic":" te/error", "payload":`straggler rejected on ${message.topic} with time=${msg_timestamp}`}]
    }
}
