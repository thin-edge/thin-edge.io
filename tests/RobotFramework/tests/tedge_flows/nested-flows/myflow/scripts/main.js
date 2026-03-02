export function onMessage(message) {
    const utf8 = new TextDecoder();
    console.log(utf8.decode(message.payload))
}

