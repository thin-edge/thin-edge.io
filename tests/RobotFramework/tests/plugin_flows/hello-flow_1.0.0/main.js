export function onMessage(message, context) {
    return [{
        topic: "hello/out",
        payload: JSON.stringify({
            text: `hello to ${context.config.call}!`,
        }),
    }];
}
