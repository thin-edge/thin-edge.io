const utf8 = new TextDecoder()

export function onMessage (message) {
  let payload = JSON.parse(utf8.decode(message.payload))
  let time = message.time
  let tzOffset = time.getTimezoneOffset() * 60000;

  payload.time = time.toString();
  payload.utc = time.toISOString();
  payload.local = (new Date(time.getTime() - tzOffset)).toISOString().slice(0, -1);

  return {
    topic: message.topic,
    payload: JSON.stringify(payload)
  }
}
