use std::process::{self, Command, ExitStatus, Stdio};
use std::time::Duration;
use std::{thread, time};

use rumqttc::QoS::AtLeastOnce;
use rumqttc::{Event, Incoming, MqttOptions, Outgoing, Packet};
use sysinfo::{PidExt, ProcessExt, System, SystemExt};

fn main() {
    //let tedge_services = vec!["tedge-mapper-c8y","tedge-mapper-az", "tedge-mapper-collectd", "tedge-agent"];
    let _ = notify_systemd(process::id(), "--ready");
    let tedge_services = vec!["tedge-mapper-c8y"];
    for service in tedge_services {
        let req_topic = format!("tedge/health-check/{}", service);
        let res_topic = format!("tedge/health/{}", service);
        if service.contains("c8y") {
            let _ = monitor_tedge_service(service, Some("c8y"), &req_topic, &res_topic);
        }
    }
    println!(
        "tedge_mapper_c8y_pid: {:?}",
        get_process_id("tedge_mapper", Some("c8y"))
    );
}

pub fn monitor_tedge_service(
    name: &str,
    m_type: Option<&str>,
    req_topic: &str,
    res_topic: &str,
) -> Result<ExitStatus, WatchdogError> {
    const RESPONSE_TIMEOUT: Duration = Duration::from_secs(60);
    let client_id: &str = &format!("{}-health", name);

    let mut options = MqttOptions::new(client_id, "localhost", 1883);
    options.set_keep_alive(RESPONSE_TIMEOUT);

    let (mut client, mut connection) = rumqttc::Client::new(options, 10);

    client.subscribe(res_topic, AtLeastOnce).unwrap();

    loop {
        for event in connection.iter() {
            match event {
                Ok(Event::Incoming(Packet::SubAck(_))) => {
                    client.publish(req_topic, AtLeastOnce, false, "").unwrap();
                }
                Ok(Event::Incoming(Packet::PubAck(_))) => {
                    println!("published successfully");
                    // The request has been sent
                }
                Ok(Event::Incoming(Packet::Publish(_response))) => {
                    // We got a response forward it to systemd
                    // println!("received response {:?}", response);
                    let pid = get_process_id("tedge_mapper", m_type);
                    notify_systemd(pid, "WATCHDOG=1")?;
                    break;
                }
                Ok(Event::Outgoing(Outgoing::PingReq)) => {
                    // No messages have been received for a while
                    println!("Local MQTT publish has timed out.");
                    break;
                }
                Ok(Event::Incoming(Incoming::Disconnect)) => {
                    eprintln!("ERROR: Disconnected");
                    break;
                }
                Err(err) => {
                    eprintln!("ERROR: {:?}", err);
                    client.subscribe(res_topic, AtLeastOnce).unwrap();
                    break;
                }
                _ => {}
            }
        }
        thread::sleep(time::Duration::from_secs(2));
        client.publish(req_topic, AtLeastOnce, false, "").unwrap();
    }
}

fn get_process_id(daemon_name: &str, mapper_type: Option<&str>) -> u32 {
    let s = System::new_all();

    for process in s.processes_by_exact_name(daemon_name) {
        println!("{} {} {:?}", process.pid(), process.name(), process.cmd());
        match mapper_type {
            Some(m_type) => {
                for subcmd in process.cmd().iter() {
                    if m_type.eq(subcmd) {
                        return process.pid().as_u32();
                    }
                }
            }
            None => {
                return process.pid().as_u32();
            }
        }
    }
    return 0;
}

fn notify_systemd(pid: u32, status: &str) -> Result<ExitStatus, WatchdogError> {
    let pid_opt = format!("--pid={}", pid.to_string());
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
