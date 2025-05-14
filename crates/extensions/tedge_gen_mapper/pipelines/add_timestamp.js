export function process (timestamp, message) {
  let payload = JSON.parse(message.payload)
  payload.time = Number(timestamp.seconds) + (timestamp.nanoseconds / 1e9)

  return [{
    topic: message.topic,
    payload: JSON.stringify(payload)
  }]
}
