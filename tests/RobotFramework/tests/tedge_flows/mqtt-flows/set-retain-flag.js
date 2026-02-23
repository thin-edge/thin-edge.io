export function onMessage(message) {
    message.mqtt = {
        retain: true
    }

    return [message]
}
