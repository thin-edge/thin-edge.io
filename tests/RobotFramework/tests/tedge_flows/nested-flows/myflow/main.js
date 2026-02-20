export function onMessage(message, context) {
    return [];
}

export function onInterval(time, context) {
    return [
        { topic: "myflow", payload: "myflow" }
    ]
}
