// Reject any message that is too old, too new or with no timestamp
export function onMessage (message, config) {
    let payload = JSON.parse(message.payload)
    if (!payload.time) {
        return []
    }

    let msg_time = payload.time
    if (typeof(msg_time) === "string") {
        msg_time = Date.parse(msg_time) / 1e3
    }

    let time = message.time
    let max = time + (config.max_advance || 1);
    let min = time - (config.max_delay || 10);

    if (min <= msg_time && msg_time <= max) {
        return [message]
    } else {
        return [{"topic":" te/error", "payload":`straggler rejected on ${message.topic} with time=${msg_time} at ${time}`}]
    }
}
