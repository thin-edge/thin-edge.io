export function onMessage (message) {
  let payload = JSON.parse(message.payload)
  let timestamp = message.timestamp
  let tzOffset = timestamp.getTimezoneOffset() * 60000;

  payload.time = timestamp.toString();
  payload.utc = timestamp.toISOString();
  payload.local = (new Date(timestamp.getTime() - tzOffset)).toISOString().slice(0, -1);

  return {
    topic: message.topic,
    payload: JSON.stringify(payload)
  }
}