// Translate a collectd measurement into a thin-edge measurement
//
// The output topic is not a regular thin-edge topic, though, but a measurement key.
// The intent is to use a batcher to group measurements over time windows keeping only one measurement per type.

const utf8 = new TextDecoder();

export function onMessage(message) {
    let groups = message.topic.split('/')
    let data = utf8.decode(message.payload).split(':')

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
        topic: `${group}/${measurement}`,
        payload: `{"time": ${time}, "${group}": {"${measurement}": ${value}}}`
    }]
}