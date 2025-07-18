export function onMessage (message, config) {
  return [{
    topic: config.topic || "te/error",
    payload: message.payload
  }]
}
