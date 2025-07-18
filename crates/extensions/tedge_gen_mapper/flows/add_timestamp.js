export function onMessage (message) {
  let payload = JSON.parse(message.payload)
  if (!payload.time) {
    payload.time = message.timestamp.seconds + (message.timestamp.nanoseconds / 1e9)
  }

  return {
    topic: message.topic,
    payload: JSON.stringify(payload)
  }
}
