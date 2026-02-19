class State {
    static cache = []
}

const utf8 = new TextDecoder()

export function onMessage(message, context) {
    let topic_parts = message.topic.split( '/')
    if (topic_parts.length < 7 || topic_parts[5] != "e") {
        throw new Error("Not a thin-edge event")
    }
    let event = JSON.parse(utf8.decode(message.payload))
    let c8y_event = {
        type: topic_parts[6] || "ThinEdgeEvent"
    }
    Object.assign(c8y_event, event)

    let entity_id = `${topic_parts[1]}/${topic_parts[2]}/${topic_parts[3]}/${topic_parts[4]}`
    if (entity_id != "device/main//") {
        let entity = context.mapper.get(entity_id)
        if (entity && entity["@type"] == "child-device") {
            let external_id = entity["@id"]
            if (external_id) {
                let source = {
                    "externalSource": {
                        "externalId": external_id,
                        "type": "c8y_Serial"
                    }
                }
                Object.assign(c8y_event, source)
            } else {
                throw new Error(`Unknown @id for ${entity_id}`)
            }
        }
        else {
            // Not registered yet
            State.cache.push(message)
            return null
        }
    }

    return {
        topic: "c8y/event/events/create",
        payload: JSON.stringify(c8y_event)
    }
}

export function onInterval(time, context) {
    let pending_events = State.cache
    State.cache = []

    let c8y_events = []
    for (const event of pending_events) {
        let c8y_event = onMessage(event, context)
        if (c8y_event) {
            c8y_events.push(c8y_event)
        }
    }
    return c8y_events
}
