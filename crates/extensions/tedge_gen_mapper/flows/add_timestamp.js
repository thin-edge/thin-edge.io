export function onMessage (timestamp, message) {
  let payload = JSON.parse(message.payload)
  if (!payload.time) {
    payload.time = timestamp.seconds + (timestamp.nanoseconds / 1e9)
  }

  return [{
    topic: message.topic,
    payload: JSON.stringify(payload)
  }]
}
