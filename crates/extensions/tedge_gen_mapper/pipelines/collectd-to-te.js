export function process(_timestamp, message, config) {
    let groups = message.topic.split('/')
    let data = message.payload.split(':')

    let group = groups[2]
    let measurement = groups[3]
    let time = data[0]
    let value = data[1]

    return [{
        topic: config.topic || "te/device/main///m/collectd",
        payload: `{"time": ${time}, "${group}": {"${measurement}": ${value}}}`
    }]
}