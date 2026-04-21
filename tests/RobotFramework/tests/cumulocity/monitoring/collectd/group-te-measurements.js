// Translate an array of thin-edge measurements into a single message with grouped measurements

const utf8 = new TextDecoder();

export function onMessage(message, context) {
    const { topic = "te/device/main///m/" } = context.config;
    let measurements = JSON.parse(utf8.decode(message.payload))
    let grouped_measurements = {}

    for (let measurement of measurements) {
        Object.assign(grouped_measurements, measurement.payload)
    }

    return [{
        topic,
        payload: JSON.stringify(grouped_measurements)
    }]
}