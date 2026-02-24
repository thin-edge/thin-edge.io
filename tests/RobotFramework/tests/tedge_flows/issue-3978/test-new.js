export function onMessage() {
    return [{
        topic: "test/out",
        payload: `{"new":"I am from test-new.js"}`
    }]
}