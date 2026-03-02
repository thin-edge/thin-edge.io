export function onMessage(message, context) {
    const greeting = context.config.greeting ?? "Hello World!"
    message.payload = greeting

    return [message]
}