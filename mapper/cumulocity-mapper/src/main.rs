mod mapper;

fn main() {
    let name = "c8y-mapper";
    let in_topic = "tedge/measurements";
    let out_topic = "c8y/s/us";
    let err_topic = "tedge/errors";

    mapper::run(name, in_topic, out_topic, err_topic).unwrap();
}