use daemonize::Daemonize;
use rumqttc::{Client, Event, Incoming, MqttOptions, Packet, QoS};
use std::fs::File;
use structopt::StructOpt;

/// Record all the messages of a topic
///
/// Wait that the subscription has been acknowledged, before returning.
/// On success, the recording in done by a background process.
#[derive(StructOpt)]
struct RecordCmd {
    /// The topic which messages are to be recorded
    pub topic: String,

    /// The file where to store the recorded messages
    pub output: String,
}

fn main() {
    let command = RecordCmd::from_args();

    record(&command.topic, &command.output);
}

fn record(topic: &str, output: &str) {
    let mut garded_output = match File::create(output) {
        Ok(file) => Some(file),
        Err(err) => {
            eprintln!("ERROR: {}", err);
            return;
        }
    };

    let mut options = MqttOptions::new("mqtt-record", "localhost", 1883);
    options.set_clean_session(true);

    let (mut client, mut connection) = Client::new(options, 10);
    if let Err(err) = client.subscribe(topic, QoS::AtLeastOnce) {
        eprintln!("ERROR: {}", err);
        return;
    }

    for event in connection.iter() {
        match event {
            Ok(Event::Incoming(Packet::SubAck(_))) => {
                if let Some(output) = garded_output.take() {
                    eprintln!("INFO: Connected");
                    run_background(output);
                }
            }
            Ok(Event::Incoming(Packet::Publish(message))) => {
                match std::str::from_utf8(&message.payload) {
                    Ok(payload) => {
                        println!("[{}] {}", &message.topic, payload);
                    }
                    Err(err) => {
                        eprintln!("ERROR: {}", err);
                    }
                }
            }
            Ok(Event::Incoming(Incoming::Disconnect)) => {
                eprintln!("INFO: Disconnected");
                return;
            }
            Err(err) => {
                eprintln!("ERROR: {}", err);
                return;
            }
            _ => {}
        }
    }
}

fn run_background(output: File) {
    let daemonize = Daemonize::new().stdout(output);

    match daemonize.start() {
        Ok(_) => eprintln!("Success, daemonized"),
        Err(e) => eprintln!("Error, {}", e),
    }
}
