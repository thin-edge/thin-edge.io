export function onMessage(message, context) {
    return [{
        topic: "default/out",
        payload: JSON.stringify({
            text: `default flow!`,
        }),
    }];
}
