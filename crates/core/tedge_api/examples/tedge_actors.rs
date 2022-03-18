use actix::prelude::*;

#[derive(Message, Debug)]
#[rtype(result = "()")]
struct Measurement {
    source: String,
    timestamp: usize,
    value: f32,
}

struct MeasurementSource {
    name: String,
}

impl Actor for MeasurementSource {
    type Context = Context<Self>;
}

struct MeasurementConsumer {
    name: String,
}

impl Actor for MeasurementConsumer {
    type Context = Context<Self>;
}

impl Handler<Measurement> for MeasurementConsumer {
    type Result = ();

    fn handle(&mut self, msg: Measurement, ctx: &mut Self::Context) -> Self::Result {
        println!("{} handles {:?}", self.name, msg);
    }
}

#[actix_rt::main]
async fn main() {
    // start new actor
    let consumer = MeasurementConsumer {
        name: "c8y".to_string(),
    }
    .start();

    // send message and get future for result
    let () = consumer
        .send(Measurement {
            source: "collectd/cpu".to_string(),
            timestamp: 123456,
            value: 10.0,
        })
        .await
        .unwrap();

    // stop system and exit
    System::current().stop();
}
