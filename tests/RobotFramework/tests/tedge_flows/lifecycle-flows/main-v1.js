// Version 1 of the lifecycle-test flow script.
// Just marks the output with "version":"v1" so tests can confirm which version is active.
export function onMessage(message) {
    message.payload = JSON.stringify({ version: "v1" })
    return [message]
}
