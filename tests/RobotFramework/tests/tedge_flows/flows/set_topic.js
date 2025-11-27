export function onMessage (message, context) {
  return [{
    topic: context.config.topic || "te/error",
    payload: message.payload
  }]
}
