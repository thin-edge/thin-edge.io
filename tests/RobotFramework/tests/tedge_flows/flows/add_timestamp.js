export function onMessage (message) {
  let payload = JSON.parse(message.payload)
  if (!payload.time) {
    let timestamp = message.timestamp
    payload.time = timestamp.seconds + (timestamp.nanoseconds / 1e9)
  }

  return [{
    topic: message.topic,
    payload: JSON.stringify(payload)
  }]
}
