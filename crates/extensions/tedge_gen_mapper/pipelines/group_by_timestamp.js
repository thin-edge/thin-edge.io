class State {
    static batch = []
}

export function process (timestamp, message) {
    State.batch.push(message)
    return []
}

export function tick() {
    let batch = State.batch
    State.batch = []
    return batch
}