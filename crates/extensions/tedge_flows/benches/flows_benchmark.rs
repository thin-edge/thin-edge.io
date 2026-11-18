use criterion::criterion_group;
use criterion::criterion_main;
use criterion::Criterion;
use std::time::SystemTime;
use tedge_flows::FlowStep;
use tedge_flows::JsRuntime;
use tedge_flows::JsScript;
use tedge_flows::Message;
use tokio::sync::Mutex;

fn collectd_script(c: &mut Criterion) {
    let js = r#"
export function onMessage(message, context) {
const { topic = "te/device/main///m/collectd" } = context.config
const td = new globalThis.TextDecoder()
let groups = message.topic.split( '/')
let data = td.decode(message.payload).split(':')

let group = groups[2]
	let measurement = groups[3]
	let time = data[0]
	let value = data[1]

return [ {
    topic: topic,
    payload: `{"time": ${time}, "${group}": {"${measurement}": ${value}}}`
}]
}
    "#;

    let input = Message::new(
        "collectd/h/memory/percent-used",
        "1748440192.104:19.9289468288182",
    );

    let output = vec![Message::new(
        "te/device/main///m/collectd",
        r#"{"time": 1748440192.104, "memory": {"percent-used": 19.9289468288182}}"#,
    )];

    let async_runtime = tokio::runtime::Runtime::new().unwrap();
    let (js_runtime, script) = async_runtime.block_on(runtime_with(js));
    let script = Mutex::new(script);
    let timestamp = SystemTime::now();

    c.bench_function("collectd-js", |b| {
        b.to_async(&async_runtime)
            .iter(|| run_script(&js_runtime, timestamp, &script, &input, &output))
    });
}

fn collectd_native(c: &mut Criterion) {
    let input = Message::new(
        "collectd/h/memory/percent-used",
        "1748440192.104:19.9289468288182",
    );

    let output = vec![Message::new(
        "te/device/main///m/collectd",
        r#"{"time": 1748440192.104, "memory": {"percent-used": 19.9289468288182}}"#,
    )];
    c.bench_function("collectd-native", |b| {
        b.iter(|| run_collectd_native(&input, &output))
    });
}

fn run_collectd_native(input: &Message, expected: &Vec<Message>) {
    let groups: Vec<_> = input.topic.split('/').collect();
    let data: Vec<_> = input.payload_str().unwrap().split(':').collect();

    let group = groups[2];
    let measurement = groups[3];
    let time = data[0];
    let value = data[1];

    let payload = format!(r#"{{"time": {time}, "{group}": {{"{measurement}": {value}}}}}"#,);
    let output = vec![Message::new("te/device/main///m/collectd", payload)];
    assert_eq!(&output, expected);
}

async fn run_script(
    runtime: &JsRuntime,
    timestamp: SystemTime,
    js: &Mutex<FlowStep>,
    input: &Message,
    expected: &Vec<Message>,
) {
    let mut js = js.lock().await;
    let output = js.on_message(runtime, timestamp, &input).await.unwrap();
    assert_eq!(&output, expected);
}

async fn runtime_with(js: &str) -> (JsRuntime, FlowStep) {
    let mut runtime = JsRuntime::with_default().await.unwrap();
    let mut script = JsScript::new("toml|1|js".to_owned(), "toml".into(), "js".into());
    if let Err(err) = runtime.load_script_literal(&mut script, js).await {
        panic!("{:?}", err);
    }
    let step = FlowStep::new_script(script);
    (runtime, step)
}

criterion_group!(benches, collectd_script, collectd_native);
criterion_main!(benches);
