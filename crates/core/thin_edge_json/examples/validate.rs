use std::env;
use thin_edge_json::measurement::MeasurementVisitor;
use time::OffsetDateTime;

#[global_allocator]
static GLOBAL: &stats_alloc::StatsAlloc<std::alloc::System> = &stats_alloc::INSTRUMENTED_SYSTEM;

fn main() -> anyhow::Result<()> {
    let region = stats_alloc::Region::new(GLOBAL);

    let mut args = env::args();
    let _ = args.next();
    let input_file = args.next().expect("input: file name");

    let input = std::fs::read_to_string(input_file)?;

    let mut builder = DummyVisitor;

    let res: anyhow::Result<()> =
        thin_edge_json::parser::parse_str(&input, &mut builder).map_err(Into::into);

    if res.is_ok() {
        println!("OK");
    }

    println!("{:?}", region.change());

    res
}

#[derive(thiserror::Error, Debug)]
enum DummyError {}

struct DummyVisitor;

impl MeasurementVisitor for DummyVisitor {
    type Error = DummyError;

    fn visit_timestamp(&mut self, _value: OffsetDateTime) -> Result<(), Self::Error> {
        Ok(())
    }
    fn visit_measurement(&mut self, _name: &str, _value: f64) -> Result<(), Self::Error> {
        Ok(())
    }
    fn visit_start_group(&mut self, _group: &str) -> Result<(), Self::Error> {
        Ok(())
    }
    fn visit_end_group(&mut self) -> Result<(), Self::Error> {
        Ok(())
    }
}
