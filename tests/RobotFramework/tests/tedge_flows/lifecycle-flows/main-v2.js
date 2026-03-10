// Version 2 of the lifecycle-test flow script.
// Marks output with "version":"v2" so tests can confirm the script was updated.
export function onMessage(message) {
    message.payload = JSON.stringify({ version: "v2" })
    return [message]
}
