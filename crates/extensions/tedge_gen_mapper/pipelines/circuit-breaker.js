// A filter that let messages go through, unless too many messages are received within a given period
//
// This filter is configured by the following settings:
// - tick_every_seconds: the frequency at which the sliding window is moved
// - tick_count: size of the time windows
// - too_many: how many messages is too many (received during the last tick_count*tick_every_seconds seconds)
// - back_to_normal: how many messages is okay to reactivate the filter if bellow
// - message_on_too_many: message sent when the upper threshold is crossed
// - message_on_back_to_normal: message sent when the lower threshold is crossed
// - stats_topic: topic for statistic messages
class State {
    static open = false
    static total = 0
    static batch = [0]
}


export function process (timestamp, message, config) {
    State.total += 1
    State.batch[0] += 1
    if (State.open) {
        let back_to_normal = config?.back_to_normal || 100
        if (State.total < back_to_normal) {
            State.open = false
            if (config?.message_on_back_to_normal) {
                return [config?.message_on_back_to_normal, message]
            } else {
                return [message]
            }
        } else {
            return []
        }
    } else {
        let too_many = config?.too_many || 1000
        if (State.total < too_many) {
            return [message]
        } else {
            State.open = true
            if (config?.message_on_too_many) {
                return [config?.message_on_too_many]
            } else {
                return []
            }
        }
    }
}


export function tick(timestamp, config) {
    let max_batch_count = config?.tick_count || 10
    let new_batch_count = State.batch.unshift(0)
    if (new_batch_count > max_batch_count) {
        State.total -= State.batch.pop()
    }

    if (config?.stats_topic) {
        return [{
            topic: config?.stats_topic,
            payload: `{"circuit-breaker-open": ${State.open}, "total": ${State.total}, "batch": ${State.batch}}`
        }]
    } else {
        return []
    }

}