// Reject any message that is too old, too new or with no timestamp
export function onMessage (message, context) {
    const { max_advance = 1, max_delay = 10} = context.config
    let payload = JSON.parse(message.payload)
    if (!payload.time) {
        return []
    }

    let msg_time = payload.time
    if (typeof(msg_time) === "string") {
        msg_time = Date.parse(msg_time) / 1e3
    }

    let time = message.time
    let max = time + max_advance;
    let min = time - max_delay;

    if (min <= msg_time && msg_time <= max) {
        return [message]
    } else {
        return [{"topic":" te/error", "payload":`straggler rejected on ${message.topic} with time=${msg_time} at ${time}`}]
    }
}
