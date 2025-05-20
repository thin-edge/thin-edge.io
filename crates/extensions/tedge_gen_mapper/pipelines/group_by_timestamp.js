// Demonstrate that messages can be delayed
export function process (timestamp, message, config) {
    if ( typeof process.batch == 'undefined' ) {
        process.batch = [];
    }

    let len = process.batch.push(message)
    let batch_len = config.batch_len || 4
    if (len < batch_len) {
        return []
    }

    let batch = process.batch
    process.batch = []
    return batch
}