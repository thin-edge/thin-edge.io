const utf8 = new TextDecoder()

export function onMessage(message, context) {
    let topic = message.topic.split( '/')
    if (topic.length < 5) {
        throw new Error("Not a thin-edge entity registration");
    }
    let entity_id = `${topic[1]}/${topic[2]}/${topic[3]}/${topic[4]}`

    let entity = JSON.parse(utf8.decode(message.payload))
    context.mapper.set(entity_id, entity)

    console.log("Entity metadata", entity_id, entity)
    return [{
        topic: "te/infos",
        payload: `New entity: ${entity.name}`
    }]
}
