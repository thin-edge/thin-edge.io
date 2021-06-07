use futures::future::FutureExt;
use futures::select;
use futures_timer::Delay;
use log::debug;
use log::error;
use log::info;
use mqtt_client::{Client, Config, Message, MqttClient, MqttErrorStream, MqttMessageStream, Topic};
use std::convert::TryFrom;
use std::env;
use std::io::Write;
use std::process;
use std::time::{Duration, Instant};
//use rumqttc::QoS;

/*

This is a small and flexible publisher for deterministic test data.
Its based on the temperature publisher.

- TODO: Improve code quality
- TODO: Add different data types for JSON publishing
- TODO: Command line switch to swith betwen REST and JSON
- TODO: Currently REST sending is disabled and JSON publishing is enabled
- TODO: Add QoS selection
*/

const C8Y_TEMPLATE_RESTART: &str = "510";

// Templates:
// https://cumulocity.com/guides/10.4.6/device-sdk/mqtt/
//
// Create custom measurement (200)
// Create signal strength measurement (210)
// Create temperature measurement (211)
// Create battery measurement (212)

// sawtooth_publisher <wait_time_ms> <height> <iterations> <template>
//
// cargo run --example sawtooth_publisher 100 100 100 flux
// cargo run --example sawtooth_publisher 1000 10 10 sawmill

#[tokio::main]
pub async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = env::args().collect();
    // wait time, template, tooth-height,
    if args.len() != 5 {
        println!("Usage: sawtooth_publisher <wait_time_ms> <height> <iterations> <template: sawmill|flux>");
        panic!("Errof: Not enough Command line Arguments");
    }
    let wait: i32 = args[1].parse().expect("Cannot parse wait time");
    let height: i32 = args[2].parse().expect("Cannot parse height");
    let iterations: i32 = args[3].parse().expect("Cannot parse iterations");
    let template: String = String::from(&args[4]);

    println!(
        "Publishing sawtooth with delay {}ms height {} iterations {} template {} will cause {} publishs.",
        wait,
        height,
        iterations,
        template,
        height * iterations
    );
    let c8y_msg = Topic::new("tedge/measurements")?;
    let c8y_cmd = Topic::new("c8y/s/ds")?;
    let c8y_err = Topic::new("c8y/s/e")?;

    init_logger();

    let name = "sawtooth_".to_string() + &process::id().to_string();
    let mqtt = Client::connect(&name, &Config::default()).await?;

    let commands = mqtt.subscribe(c8y_cmd.filter()).await?;
    let c8y_errors = mqtt.subscribe(c8y_err.filter()).await?;
    let errors = mqtt.subscribe_errors();

    let start = Instant::now();

    if template == "flux" {
        tokio::spawn(publish_topic(mqtt, c8y_msg, wait, height, iterations));
    } else if template == "sawmill" {
        tokio::spawn(publish_multi_topic(mqtt, c8y_msg, wait, height, iterations));
    } else {
        println!("Wrong template");
        panic!("Exiting");
    };

    select! {
        _ = listen_command(commands).fuse() => (),
        _ = listen_c8y_error(c8y_errors).fuse() => (),
        _ = listen_error(errors).fuse() => (),
    }

    let elapsed = start.elapsed();
    println!(
        "Execution took {} s {} ms",
        elapsed.as_secs(),
        elapsed.as_millis()
    );

    let elapsedm: u32 = u32::try_from(elapsed.as_millis()).unwrap();
    let elapsedmsf: f64 = f64::try_from(elapsedm).unwrap();
    let rate: f64 =
        elapsedmsf / (f64::try_from(height).unwrap() * f64::try_from(iterations).unwrap());

    let pubpersec = 1.0 / rate * 1000.0;
    println!("Publish rate: {:.3} ms/pub", rate);
    println!("Publish per second: {:.3} pub/s", pubpersec);

    Ok(())
}

async fn publish_topic(
    mqtt: Client,
    c8y_msg: Topic,
    wait: i32,
    height: i32,
    iterations: i32,
) -> Result<(), mqtt_client::Error> {
    info!("Publishing temperature measurements");
    println!();
    for iteration in 0..iterations {
        for value in 0..height {
            let payload = format!("{{ {}: {} }}", "\"Flux [F]\"", value);
            debug!("{} ", value);
            debug!("{}", payload);

            mqtt.publish(Message::new(&c8y_msg, payload)).await?;
            Delay::new(Duration::from_millis(u64::try_from(wait).unwrap())).await;
            std::io::stdout().flush().expect("Flush failed");
        }
        println!("Iteraton: {}", iteration);
    }
    println!();

    mqtt.disconnect().await?;
    Ok(())
}

async fn publish_multi_topic(
    mqtt: Client,
    c8y_msg: Topic,
    wait: i32,
    height: i32,
    iterations: i32,
) -> Result<(), mqtt_client::Error> {
    info!("Publishing temperature measurements");
    println!();
    let series_name = "\"Sawmill [S]\"";
    let series_count = 10;
    for iteration in 0..iterations {
        for value in 0..height {
            let mut series: String = String::new();
            for s in 0..series_count {
                series += &format!(
                    "\"saw_{}\": {} ,",
                    s,
                    (value + s * height / series_count) % height
                );
            }
            let seriesx = &series.trim_end_matches(',');

            let payload = format!("{{ {}: {{ {} }} }}", series_name, seriesx);
            debug!("{} ", value);
            debug!("{}", payload);

            mqtt.publish(Message::new(&c8y_msg, payload)).await?;

            Delay::new(Duration::from_millis(u64::try_from(wait).unwrap())).await;
            std::io::stdout().flush().expect("Flush failed");
        }
        println!("Iteraton: {}", iteration);
    }
    println!();

    mqtt.disconnect().await?;
    Ok(())
}

async fn listen_command(mut messages: Box<dyn MqttMessageStream>) {
    while let Some(message) = messages.next().await {
        debug!("C8Y command: {:?}", message.payload_trimmed());
        if let Some(cmd) = std::str::from_utf8(&message.payload_trimmed()).ok() {
            if cmd.contains(C8Y_TEMPLATE_RESTART) {
                info!("Stopping on remote request ... should be restarted by the daemon monitor.");
                break;
            }
        }
    }
}

async fn listen_c8y_error(mut messages: Box<dyn MqttMessageStream>) {
    let mut count: u32 = 0;
    while let Some(message) = messages.next().await {
        error!("C8Y error: {:?}", message.payload_trimmed());
        if count >= 3 {
            panic!("Panic!");
        }
        count += 1;
    }
}

async fn listen_error(mut errors: Box<dyn MqttErrorStream>) {
    let mut count: u32 = 0;
    while let Some(error) = errors.next().await {
        error!("System error: {}", error);
        if count >= 3 {
            panic!("Panic!");
        }
        count += 1;
    }
}

fn init_logger() {
    let logger = env_logger::Logger::from_default_env();
    let task_id = 1;

    async_log::Logger::wrap(logger, move || task_id)
        .start(log::LevelFilter::Trace)
        .unwrap();
}
