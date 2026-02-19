export function onMessage(message) {
    let topic = message.topic.split('/')
    let data = message.payload

    if (topic.length < 2) {
        throw new Error("Not a sensor topic");
    }

    let measurement = topic[1]

    return [{
        topic: "te/sensor",
        payload: `{"${measurement}": ${data}}`
    }]
}