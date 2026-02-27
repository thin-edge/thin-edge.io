export function onStartup(time, context) {
    context.mapper.set("on-startup.js", "hello from on-startup.js")
    console.log(context.mapper.get("on-startup.js"))
}
