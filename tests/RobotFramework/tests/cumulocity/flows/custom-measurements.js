export function onMessage(message, context) {
    let topic = message.topic.split('/')
    let name = topic[6];
    let value = JSON.parse(message.payload);
    if (!Number.isFinite(value)) {
        throw new Error("Invalid payload. Only numbers are accepted");
    }

    let c8y_msg = {
        "type": "custom",
        [name]: {
            [name]: {
                "value": value
             }
        },
    }

    let entity_id = `${topic[1]}/${topic[2]}/${topic[3]}/${topic[4]}`
    let entity = context.mapper.get(entity_id)
    if (entity) {
        console.log(entity)
        if (entity["@id"]) {
            c8y_msg.externalSource =  {
                "externalId": entity["@id"],
                "type":"c8y_Serial"
            }
        }
    }

    return [{
        topic: "c8y/measurement/measurements/create",
        payload: JSON.stringify(c8y_msg)
    }]
}