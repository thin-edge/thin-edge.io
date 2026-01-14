export function onMessage(message, context) {
    let topic = message.topic.split('/')
    let mea_type = topic[6];

    let mea = JSON.parse(message.payload)
    if (mea_type) {
        mea.type = mea_type
    }

    let entity_id = `${topic[1]}/${topic[2]}/${topic[3]}/${topic[4]}`
    let entity = context.mapper.get(entity_id)
    if (entity) {
        if (entity.name) {
            mea.source = entity.name
        } else {
            mea.source = entity_id
        }
    }

    return [{
        topic: message.topic,
        payload: JSON.stringify(mea)
    }]
}