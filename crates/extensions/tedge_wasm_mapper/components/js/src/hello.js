class Message {
  constructor(topic, payload) {
    this.topic = topic;
    this.payload = payload;
  }
}


export function process (timestamp, message) {
  let payload = JSON.parse(message.payload)
  payload.time = Number(timestamp.seconds) + (timestamp.nanoseconds / 1e9)
  
  return [new Message(`${message.topic}/out`, JSON.stringify(payload))]
}
