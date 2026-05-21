const utf8 = new TextDecoder();

export function onMessage(message) {
  return {
    topic: message.topic,
    payload: `${message.topic}:${utf8.decode(message.payload)}`,
  };
}
