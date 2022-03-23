use std::time::Duration;
use std::{thread, time};

use rumqttc::QoS::AtLeastOnce;
use rumqttc::{Event, Incoming, MqttOptions, Outgoing, Packet};
use sd_notify::NotifyState;
use sysinfo::{ProcessExt, System, SystemExt, PidExt};

fn main() {
    println!("tedge_mapper_c8y_pid: {:?}",get_process_id("tedge_mapper"));
    monitor_tedge_mapper_c8y();
}

pub fn monitor_tedge_mapper_c8y() {
    const C8Y_MAPPER_HEALTH_CHECK_REQ: &str = "tedge/health-check/req/tedge-mapper-c8y";
    const C8Y_MAPPER_HEALTH_CHECK_RES: &str = "tedge/health-check/res/tedge-mapper-c8y";
    const RESPONSE_TIMEOUT: Duration = Duration::from_secs(10);
    const CLIENT_ID: &str = "tedge_mapper_c8y_health";

    let mut options = MqttOptions::new(CLIENT_ID, "localhost", 1883);
    options.set_keep_alive(RESPONSE_TIMEOUT);

    let (mut client, mut connection) = rumqttc::Client::new(options, 10);

    client
        .subscribe(C8Y_MAPPER_HEALTH_CHECK_RES, AtLeastOnce)
        .unwrap();

    loop {
        for event in connection.iter() {
            match event {
                Ok(Event::Incoming(Packet::SubAck(_))) => {
                    //println!("received sub ack and publishing");
                    // We are ready to get the response, hence send the request
                    client
                        .publish(
                            C8Y_MAPPER_HEALTH_CHECK_REQ,
                            AtLeastOnce,
                            false,
                            "{\"id\":\"3453466\"}",
                        )
                        .unwrap();
                }
                Ok(Event::Incoming(Packet::PubAck(_))) => {
                    println!("published successfully");
                    // The request has been sent
                }
                Ok(Event::Incoming(Packet::Publish(response))) => {
                    // We got a response forward it to systemd
                    println!("received response {:?}", response);
                    notify_systemd();
                    break;
                    // return Ok(connected_url);
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
                    break;
                }
                _ => {}
            }
        }
        thread::sleep(time::Duration::from_secs(3));
        client
            .publish(
                C8Y_MAPPER_HEALTH_CHECK_REQ,
                AtLeastOnce,
                false,
                "{\"id\":\"3453466\"}",
            )
            .unwrap();
    }
}

fn get_process_id(name:&str) -> u32 {
    let s = System::new_all();

    for process in s.processes_by_exact_name(name) {
        println!("{} {} ", process.pid(), process.name());
        return process.pid().as_u32();
    }
    return 0;
}

fn notify_systemd(){
    let _ = sd_notify::notify(true, &[NotifyState::Watchdog]);
}