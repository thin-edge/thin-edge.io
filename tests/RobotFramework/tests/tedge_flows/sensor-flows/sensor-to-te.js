const utf8 = new TextDecoder()

export function onMessage(message) {
    let topic = message.topic.split('/')
    let data = utf8.decode(message.payload)

    if (topic.length < 2) {
        throw new Error("Not a sensor topic");
    }

    let measurement = topic[1]

    return [{
        topic: "te/sensor",
        payload: `{"${measurement}": ${data}}`
    }]
}
