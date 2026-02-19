export function onMessage(message) {
    let topic_parts = message.topic.split( '/')
    if (topic_parts.length < 5) {
        throw new Error("Not a thin-edge entity registration");
    }
    let entity_id = `${topic_parts[1]}/${topic_parts[2]}/${topic_parts[3]}/${topic_parts[4]}`

    return {
        topic: entity_id,
        payload: message.payload
    }
}
