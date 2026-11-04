const utf8 = new TextDecoder()

// Parse the trailing Azure property bag of a direct method topic, e.g.
// "?$rid=42" or "?$rid=42&foo=bar", into a map. Values are returned verbatim
// (not URL-decoded): "$rid" is an opaque token echoed back on the response
// topic, and direct methods carry no other properties in practice.
function parseProperties(propertyBag) {
    const stripped = propertyBag.startsWith("?") ? propertyBag.slice(1) : propertyBag
    const properties = {}
    for (const pair of stripped.split("&")) {
        const eq = pair.indexOf("=")
        if (eq === -1) {
            continue
        }
        properties[pair.slice(0, eq)] = pair.slice(eq + 1)
    }
    return properties
}

export function onMessage(message, context) {
    const levels = message.topic.split("/")
    if (levels.length < 5 || levels[1] !== "methods" || levels[2] !== "POST") {
        throw new Error(`Not an Azure IoT Hub direct method request: ${message.topic}`)
    }
    const method = levels[3]
    const properties = parseProperties(levels.slice(4).join("/"))
    const rid = properties["$rid"]
    if (rid === undefined) {
        throw new Error(`No $rid found in direct method topic: ${message.topic}`)
    }

    function reject(status, reason) {
        return [{
            topic: `az/methods/res/${status}/?$rid=${rid}`,
            payload: JSON.stringify({ reason }),
        }]
    }

    // An unmapped method runs the operation of the same name: [methods.*] is only
    // needed to rename an operation or to declare its required payload fields.
    const methodConfig = (context.config.methods ?? {})[method] ?? {}
    const operation = methodConfig.operation ?? method
    if (operation === "" || /[+#/\x00]/.test(operation)) {
        return reject(400, `Invalid method name: cannot be used to build a command topic`)
    }

    let body
    try {
        body = JSON.parse(utf8.decode(message.payload))
    } catch (e) {
        return reject(400, `Malformed payload: not a valid JSON document`)
    }
    if (typeof body !== "object" || body === null || Array.isArray(body)) {
        return reject(400, `Malformed payload: expected a JSON object`)
    }
    for (const field of methodConfig.required_fragment ?? []) {
        if (!(field in body)) {
            return reject(400, `Malformed payload: missing required field '${field}'`)
        }
    }

    const cmd_id = `az-dm-${rid}`
    if (/[+#/\x00]/.test(cmd_id)) {
        return reject(400, `Invalid $rid: cannot be used to build a command id`)
    }

    return [{
        topic: `te/device/main///cmd/${operation}/${cmd_id}`,
        payload: JSON.stringify({ ...body, status: "init", az: properties }),
        mqtt: { qos: 1, retain: true },
    }]
}
