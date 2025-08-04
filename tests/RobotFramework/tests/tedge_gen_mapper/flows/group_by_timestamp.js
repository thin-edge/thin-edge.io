class State {
    static batch = []
}

export function onMessage (message) {
    State.batch.push(message)
    return []
}

export function onInterval() {
    let batch = State.batch
    State.batch = []
    return batch
}