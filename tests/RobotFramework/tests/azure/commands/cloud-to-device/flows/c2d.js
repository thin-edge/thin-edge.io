const utf8 = new TextDecoder()

// Parse the trailing Azure property bag of a C2D message topic, e.g.
// "method=shell&$.mid=abc123" (no leading "?", unlike a direct method's).
// Both keys and values are URL-decoded, falling back to the raw string if
// decoding throws.
function parseProperties(propertyBag) {
    const stripped = propertyBag.startsWith("?") ? propertyBag.slice(1) : propertyBag
    const properties = {}
    if (stripped === "") {
        return properties
    }
    for (const pair of stripped.split("&")) {
        const eq = pair.indexOf("=")
        const rawKey = eq === -1 ? pair : pair.slice(0, eq)
        const rawValue = eq === -1 ? "" : pair.slice(eq + 1)
        properties[decodeSafely(rawKey)] = decodeSafely(rawValue)
    }
    return properties
}

function decodeSafely(value) {
    try {
        return decodeURIComponent(value)
    } catch (e) {
        return value
    }
}

// $.mid and method are sender-controlled and, when present, become part of the
// command topic: reject anything that would corrupt it.
function assertValidTopicSegment(value, name) {
    if (value === "" || /[+#/\x00]/.test(value)) {
        throw new Error(`Invalid ${name}: cannot be used to build a command topic`)
    }
}

export function onMessage(message, context) {
    const levels = message.topic.split("/")
    if (levels.length < 3 || levels[1] !== "messages" || levels[2] !== "devicebound") {
        throw new Error(`Not an Azure IoT Hub cloud-to-device message: ${message.topic}`)
    }
    const properties = parseProperties(levels.slice(3).join("/"))

    const method = properties["method"]
    if (method === undefined) {
        throw new Error(`No 'method' property found in cloud-to-device message: ${message.topic}`)
    }
    // An unmapped method runs the operation of the same name: [methods.*] is only
    // needed to rename an operation or to declare its required payload fields.
    const methodConfig = (context.config.methods ?? {})[method] ?? {}
    const operation = methodConfig.operation ?? method
    assertValidTopicSegment(operation, "method")

    let body
    try {
        body = JSON.parse(utf8.decode(message.payload))
    } catch (e) {
        throw new Error(`Malformed payload: not a valid JSON document`)
    }
    if (typeof body !== "object" || body === null || Array.isArray(body)) {
        throw new Error(`Malformed payload: expected a JSON object`)
    }
    for (const field of methodConfig.required_fragment ?? []) {
        if (!(field in body)) {
            throw new Error(`Malformed payload: missing required field '${field}'`)
        }
    }

    const mid = properties["$.mid"]
    let suffix
    if (mid !== undefined) {
        assertValidTopicSegment(mid, "$.mid")
        suffix = mid
    } else {
        suffix = message.time.getTime()
    }

    const cmd_id = `az-c2d-${suffix}`

    return [{
        topic: `te/device/main///cmd/${operation}/${cmd_id}`,
        payload: JSON.stringify({ ...body, status: "init", az: properties }),
        mqtt: { qos: 1, retain: true },
    }]
}
