class State {
    static count_per_topic = {}
}

export function onMessage (message) {
    let topic = message.topic
    let count = State.count_per_topic[topic] || 0
    State.count_per_topic[topic] = count + 1

    console.log("current count", State.count_per_topic)
    return []
}

export function onInterval(timestamp, config) {
    let message = {
        topic: config.topic || "te/error",
        payload: JSON.stringify(State.count_per_topic)
    }

    State.count_per_topic = {}
    return [message]
}