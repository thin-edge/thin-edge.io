export function onMessage (message) {
  let payload = JSON.parse(message.payload)
  if (!payload.time) {
    let time = message.time
    payload.time = time.getTime() / 1000;
  }

  return [{
    topic: message.topic,
    payload: JSON.stringify(payload)
  }]
}
