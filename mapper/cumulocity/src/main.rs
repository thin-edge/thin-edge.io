mod mapper;

fn main() {
    let name = "c8y-mapper";
    let in_topic = "tedge/measurements";
    let out_topic = "c8y/s/us";

    mapper::run(name, in_topic, out_topic, log_and_continue).unwrap();
}

fn log_and_continue(err: mapper::Error) -> Result<(),mapper::Error> {
    eprintln!("{}", err);
    Ok(())
}