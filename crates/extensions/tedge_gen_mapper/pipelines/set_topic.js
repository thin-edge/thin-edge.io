export function process (timestamp, message, config) {
  return [{
    topic: config?.topic || "te/error",
    payload: message.payload
  }]
}
