export function onMessage(message) {
    let topic = message.topic.split('/')
    let mea_type = topic[6];

    let mea = JSON.parse(message.payload)
    if (mea_type) {
        mea.type = mea_type
    }

    return [{
        topic: message.topic,
        payload: JSON.stringify(mea)
    }]
}