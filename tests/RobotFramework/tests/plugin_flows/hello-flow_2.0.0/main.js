export function onMessage(message, context) {
    return [{
        topic: "hello/out",
        payload: JSON.stringify({
            text: `hi to ${context.config.call}!`,
        }),
    }];
}
