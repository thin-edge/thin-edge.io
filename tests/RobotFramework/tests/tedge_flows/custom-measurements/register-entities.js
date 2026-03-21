const utf8 = new TextDecoder()

export function onMessage(message, context) {
    let topic = message.topic.split( '/')
    let entity_id = `${topic[1]}/${topic[2]}/${topic[3]}/${topic[4]}`
    let metadata = JSON.parse(utf8.decode(message.payload))

    let name = metadata["@name"] ?? metadata["@id"] ?? entity_id
    Object.assign(metadata, {
        "@topic-id": entity_id,
        "@name": name,
        "@type-name": "thin-edge"
    })

    context.mapper.set(entity_id, metadata)

    return {
        "topic": "fake/c8y/entities",
        "payload": JSON.stringify(metadata)
    }
}