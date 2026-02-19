const utf8 = new TextDecoder()

export function onMessage(message, context) {
    const topic = message.topic.split('/');
    const name = topic[6];
    const value = JSON.parse(utf8.decode(message.payload));
    if (!Number.isFinite(value)) {
        throw new Error("Invalid payload. Only numbers are accepted");
    }

    const c8y_msg = {
        "type": "custom",
        [name]: {
            [name]: {
                "value": value,
            },
        },
    };

    const entity_id = `${topic[1]}/${topic[2]}/${topic[3]}/${topic[4]}`;
    const entity = context.mapper.get(entity_id);
    if (entity) {
        console.log(entity);
        if (entity["@id"]) {
            c8y_msg.externalSource = {
                "externalId": entity["@id"],
                "type": "c8y_Serial",
            };
        }
    }

    return [{
        topic: "c8y/measurement/measurements/create",
        payload: JSON.stringify(c8y_msg),
    }]
}
