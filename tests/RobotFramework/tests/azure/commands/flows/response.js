const utf8 = new TextDecoder()

export function onMessage(message) {
    // The empty retained clear this very flow publishes below, looping back in.
    if (message.payload.length === 0) {
        return null
    }

    const levels = message.topic.split("/")
    const cmd_id = levels[levels.length - 1]
    if (!cmd_id.startsWith("az-")) {
        // Not a command created by the direct-methods or cloud-to-device request flow.
        return null
    }

    let state
    try {
        state = JSON.parse(utf8.decode(message.payload))
    } catch (e) {
        throw new Error(`Malformed command state on ${message.topic}: not a valid JSON document`)
    }

    if (state.status !== "successful" && state.status !== "failed") {
        return null
    }

    // Clears the command by publishing an empty retained message on its own topic,
    // which matches this flow's input filter: expect_loop (see response.toml) is
    // needed so the replayed empty message isn't dropped as a self-loop. The loop is
    // finite, since it is ignored by the empty-payload guard above on the way back in.
    const clear = { topic: message.topic, payload: "", mqtt: { qos: 1, retain: true } }

    const rid = state.az && state.az["$rid"]
    if (rid === undefined) {
        // A cloud-to-device-triggered command: there is no direct method request to
        // answer, just clear it.
        return [clear]
    }

    const status_code = state.status === "successful" ? 200 : 500

    // Internal fields that must not leak to Azure.
    const { status, logPath, "@version": version, az, ...body } = state

    return [
        {
            topic: `az/methods/res/${status_code}/?$rid=${rid}`,
            payload: JSON.stringify(body),
        },
        clear,
    ]
}
