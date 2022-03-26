use std::process::{self, Command, ExitStatus, Stdio};
use std::{thread, time};

use nanoid::nanoid;
use rumqttc::QoS::AtLeastOnce;
use rumqttc::{Event, Incoming, MqttOptions, Outgoing, Packet};
use serde::{Deserialize, Serialize};
use tedge_config::{
    ConfigRepository, ConfigSettingAccessor, MqttBindAddressSetting, MqttPortSetting,
};
use time::Duration;

pub const DEFAULT_TEDGE_CONFIG_PATH: &str = "/etc/tedge";

#[derive(Serialize, Deserialize)]
pub struct Response {
    status: String,
    pid: u32,
}

fn main() {
    let _ = start_watchdog();
}

fn start_watchdog() {
    // Send ready notification to systemd.
    let _ = notify_systemd(process::id(), "--ready");

    // Start helth check request publisher
    thread::spawn(move || publish());
    loop {
        let tedge_services = vec![
            "tedge-mapper-c8y",
            "tedge-mapper-az",
            "tedge-mapper-collectd",
            "tedge-agent",
        ];
        let mut watchdog_threads = vec![];

        for service in tedge_services {
            let res_topic = format!("tedge/health/{}", service);

            watchdog_threads.push(thread::spawn(move || {
                monitor_tedge_service(service, &res_topic)
            }));
        }

        for child in watchdog_threads {
            // Wait for the thread to finish. Returns a result.
            let _ = child
                .join()
                .expect("Couldn't join on the associated thread");
        }

        thread::sleep(Duration::from_secs(1));
    }
}

fn publish() {
    let client_id: &str = "watchdog_publisher";
    let options = get_mqtt_options(client_id);

    let mut mqtt = rumqttc::Client::new(options, 10);
    let mut timedout: bool = false;
    loop {
        if timedout {
            let options = get_mqtt_options(client_id);
            mqtt = rumqttc::Client::new(options, 10);
        }
        let _ = mqtt.0.publish("tedge/health-check", AtLeastOnce, false, "");
        for event in mqtt.1.iter() {
            match event {
                Ok(Event::Outgoing(Outgoing::Publish(_))) => {
                    break;
                }
                Ok(Event::Incoming(Packet::PubAck(_))) => {
                    break;
                }
                Ok(Event::Incoming(Packet::PubComp(_))) => {
                    break;
                }
                Ok(Event::Outgoing(Outgoing::PingReq)) => {
                    // No messages have been received for a while
                    eprintln!("Local MQTT publish has timed out.");
                    timedout = true;
                    break;
                }
                Ok(Event::Incoming(Incoming::Disconnect)) => {
                    eprintln!("Disconnected");
                    break;
                }
                Err(err) => {
                    eprintln!("Error: {}", err.to_string());
                    break;
                }
                _ => {}
            }
        }

        thread::sleep(Duration::from_secs(2));
    }
}

fn monitor_tedge_service(name: &str, res_topic: &str) -> Result<(), WatchdogError> {
    let client_id: &str = &format!("{}_{}", name, nanoid!());
    let options = get_mqtt_options(client_id);
    let (mut client, mut connection) = rumqttc::Client::new(options, 10);

    println!("started watchdog for service: {}", name);
    loop {
        for event in connection.iter() {
            match event {
                Ok(Event::Incoming(Packet::Publish(response))) => {
                    // Received response from thin-edge service, update the status to systemd on behalf of service
                    let p: Response = serde_json::from_str(
                        &String::from_utf8(response.payload.to_vec()).unwrap(),
                    )
                    .unwrap();
                    notify_systemd(p.pid, "WATCHDOG=1")?;
                    break;
                }
                Ok(Event::Incoming(Packet::ConnAck(_))) => {
                    eprintln!("INFO: Connected");
                    client.subscribe(res_topic, AtLeastOnce).unwrap();
                }
                Ok(Event::Incoming(Incoming::Disconnect)) => {
                    eprintln!("ERROR: Disconnected");
                    break;
                }
                Err(err) => {
                    eprintln!("ERROR: {:?}", err);
                    break;
                }
                _ => {}
            }
        }
    }
}

fn get_mqtt_options(client_id: &str) -> MqttOptions {
    let tedge_config_location =
        tedge_config::TEdgeConfigLocation::from_custom_root(DEFAULT_TEDGE_CONFIG_PATH);
    let config_repository = tedge_config::TEdgeConfigRepository::new(tedge_config_location.clone());
    let tedge_config = config_repository.load().unwrap();
    let host = tedge_config
        .query(MqttBindAddressSetting)
        .unwrap()
        .to_string();
    let port = tedge_config.query(MqttPortSetting).unwrap().into();
    MqttOptions::new(client_id, host, port)
}

fn notify_systemd(pid: u32, status: &str) -> Result<ExitStatus, WatchdogError> {
    let pid_opt = format!("--pid={}", pid);
    let status = Command::new("systemd-notify")
        .args([status, &pid_opt])
        .stdin(Stdio::null())
        .status()
        .map_err(|err| WatchdogError::CommandExecError {
            cmd: String::from("systemd-notify"),
            from: err,
        })?;
    Ok(status)
}

#[derive(Debug, thiserror::Error)]
pub enum WatchdogError {
    #[error("Fail to run `{cmd}`: {from}")]
    CommandExecError { cmd: String, from: std::io::Error },
}
