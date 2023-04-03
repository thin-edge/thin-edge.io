use futures::future::FutureExt;
use futures::select;
use futures_timer::Delay;
use log::debug;
use log::error;
use log::info;
use mqtt_channel::Connection;
use mqtt_channel::ErrChannel;
use mqtt_channel::Message;
use mqtt_channel::MqttError;
use mqtt_channel::PubChannel;
use mqtt_channel::SubChannel;
use mqtt_channel::Topic;
use mqtt_channel::TopicFilter;
use std::convert::TryFrom;
use std::env;
use std::fmt::Write as _;
use std::io::Write;
use std::process;
use std::time::Duration;
use std::time::Instant;
use tedge_config::ConfigRepository;

/*

This is a small and flexible publisher for deterministic test data.

- TODO: Improve code quality
- TODO: Add different data types for JSON publishing
- TODO: Command line switch to switch between REST and JSON
- TODO: Currently REST sending is disabled and JSON publishing is enabled
- TODO: Add QoS selection
*/

// Templates:
// https://cumulocity.com/guides/10.4.6/device-sdk/mqtt/
//
// Create custom measurement (200)
// Create signal strength measurement (210)
// Create temperature measurement (211)
// Create battery measurement (212)

// sawtooth-publisher <wait_time_ms> <height> <iterations> <template>
//
// cargo run sawtooth-publisher 100 100 100 flux
// cargo run sawtooth-publisher 1000 10 10 sawmill

#[tokio::main(flavor = "current_thread")]
pub async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = env::args().collect();
    // wait time, template, tooth-height,
    if args.len() != 5 {
        println!("Usage: sawtooth-publisher <wait_time_ms> <height> <iterations> <template: sawmill|flux>");
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
    let c8y_err = TopicFilter::new("c8y/s/e")?;

    init_logger();

    let config_location = tedge_config::TEdgeConfigLocation::default();
    let config_repository = tedge_config::TEdgeConfigRepository::new(config_location);
    let tedge_config = config_repository.load()?;

    let name = "sawtooth_".to_string() + &process::id().to_string();
    let mqtt_config = tedge_config
        .mqtt_config()?
        .with_clean_session(true)
        .with_session_name(name)
        .with_subscriptions(c8y_err);

    let mqtt = Connection::new(&mqtt_config).await?;

    let mqtt_pub_channel = mqtt.published;
    let c8y_errors = mqtt.received;
    let errors = mqtt.errors;

    let start = Instant::now();

    if template == "flux" {
        tokio::spawn(publish_topic(
            mqtt_pub_channel,
            c8y_msg,
            wait,
            height,
            iterations,
        ));
    } else if template == "sawmill" {
        tokio::spawn(publish_multi_topic(
            mqtt_pub_channel,
            c8y_msg,
            wait,
            height,
            iterations,
        ));
    } else {
        println!("Wrong template");
        panic!("Exiting");
    };

    select! {
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
    mut mqtt: impl PubChannel,
    c8y_msg: Topic,
    wait: i32,
    height: i32,
    iterations: i32,
) -> Result<(), MqttError> {
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

    let _ = mqtt.close().await;
    Ok(())
}

async fn publish_multi_topic(
    mut mqtt: impl PubChannel,
    c8y_msg: Topic,
    wait: i32,
    height: i32,
    iterations: i32,
) -> Result<(), MqttError> {
    info!("Publishing temperature measurements");
    println!();
    let series_name = "\"Sawmill [S]\"";
    let series_count = 10;
    for iteration in 0..iterations {
        for value in 0..height {
            let mut series: String = String::new();
            for s in 0..series_count {
                let _ = write!(
                    series,
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

    let _ = mqtt.close().await;
    Ok(())
}

async fn listen_c8y_error(mut messages: impl SubChannel) {
    let mut count: u32 = 0;
    while let Some(message) = messages.next().await {
        error!("C8Y error: {:?}", message.payload_str());
        if count >= 3 {
            panic!("Panic!");
        }
        count += 1;
    }
}

async fn listen_error(mut errors: impl ErrChannel) {
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
