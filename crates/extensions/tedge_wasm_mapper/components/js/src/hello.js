class Message {
  constructor(topic, payload) {
    this.topic = topic;
    this.payload = payload;
  }
}


export function process (timestamp, message) {
  payload.time = timestamp.seconds + timestamp.nanoseconds / 10^9
  payload = JSON.parse(message.payload);
  
  return [Message.new(message.topic, payload.stringify())]; 
}
