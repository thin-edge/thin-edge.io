export function onMessage() {
    return [{
        topic: "test/out",
        payload: `{"old":"I am from test-old.js"}`
    }]
}