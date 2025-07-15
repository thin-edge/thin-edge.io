// Compute the average value of a series of measurements received during a time windows
// - Take care of the topic: messages received over different topics are not mixed
// - Ignore messages which are not formated as thin-edge JSON
// - Ignore values which are not numbers
// - Use the first timestamp as the timestamp for the aggregate
class State {
    static agg_for_topic = {}
}

export function process (timestamp, message) {
    let topic = message.topic
    let payload = JSON.parse(message.payload)
    let agg_payload = State.agg_for_topic[topic]
    if (agg_payload) {
        for (let [k, v] of Object.entries(payload)) {
            let agg = agg_payload[k]
            if (k === "time") {
                if (!agg) {
                    let fragment = {time: v}
                    Object.assign(agg_payload, fragment)
                }
            } else if (typeof (v) === "number") {
                if (!agg) {
                    let fragment = {[k]: {sum: v, count: 1}}
                    Object.assign(agg_payload, fragment)
                } else {
                    agg.sum += v
                    agg.count += 1
                }
            } else {
                if (!agg) {
                    let fragment = {}
                    for (let [sub_k, sub_v] of Object.entries(v)) {
                        let sub_fragment = { [sub_k]: { sum: sub_v, count: 1 } }
                        Object.assign(fragment, sub_fragment)
                    }
                    Object.assign(agg_payload, { [k]: fragment })
                } else {
                    for (let [sub_k, sub_v] of Object.entries(v)) {
                        let sub_agg = agg[sub_k]
                        if (!sub_agg) {
                            agg[sub_k] = { sum: sub_v, count: 1 }
                        } else {
                            sub_agg.sum += sub_v
                            sub_agg.count += 1
                        }
                    }
                }
            }
        }
    } else {
        let agg_payload = {}
        for (let [k, v] of Object.entries(payload)) {
            if (k === "time") {
              let fragment = { time: v }
              Object.assign(agg_payload, fragment)
            }
            else if (typeof(v) === "number") {
              let fragment = { [k]: { sum: v, count: 1 } }
              Object.assign(agg_payload, fragment)
            } else {
                let fragment = {}
                for (let [sub_k, sub_v] of Object.entries(v)) {
                    let sub_fragment = { [sub_k]: { sum: sub_v, count: 1 } }
                    Object.assign(fragment, sub_fragment)
                }
                Object.assign(agg_payload, { [k]: fragment })
            }
        }
        State.agg_for_topic[topic] = agg_payload
    }

    console.log("average.state", State.agg_for_topic)
    return []
}

export function tick() {
    let messages = []

    for (let [topic, agg] of Object.entries(State.agg_for_topic)) {
        let payload = {}
        for (let [k, v] of Object.entries(agg)) {
            if (k === "time") {
                let fragment = { time: v }
                Object.assign(payload, fragment)
            }
            else if (v.sum && v.count) {
                let fragment = { [k]: v.sum / v.count }
                Object.assign(payload, fragment)
            } else {
                let fragment = {}
                for (let [sub_k, sub_v] of Object.entries(v)) {
                    let sub_fragment = { [sub_k]: sub_v.sum / sub_v.count }
                    Object.assign(fragment, sub_fragment)
                }
                Object.assign(payload, { [k]: fragment })
            }
        }

        messages.push ({
            topic: topic,
            payload: JSON.stringify(payload)
        })
    }

    State.agg_for_topic = {}
    return messages
}