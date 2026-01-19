export function onMessage(message, context) {
    const { topic = "te/device/main///m/collectd" } = context.config;
    let groups = message.topic.split('/')
    let data = message.payload.split(':')

    if (groups.length < 4) {
        throw new Error("Not a collectd topic");
    }

    if (data.length < 2) {
        throw new Error("Not a collectd payload");
    }

    let group = groups[2]
    let measurement = groups[3]
    let time = data[0]
    let value = data[1]

    return [{
        topic: topic,
        payload: `{"time": ${time}, "${group}": {"${measurement}": ${value}}}`
    }]
}