use chrono::prelude::*;
use criterion::{black_box, criterion_group, criterion_main, Criterion};
use thin_edge_json::measurement::MeasurementVisitor;

const INPUT: &str = r#"{
    "time" : "2021-04-30T17:03:14.123+02:00",
    "pressure": 123.4,
    "temperature": 24,
    "location": {
          "latitude": 32.54,
          "longitude": -117.67,
          "altitude": 98.6
    },
    "coordinate": {
        "x": 1,
        "y": 2.0,
        "z": -42.0
    }
}"#;

#[derive(thiserror::Error, Debug)]
enum DummyError {}

struct DummyVisitor;

impl MeasurementVisitor for DummyVisitor {
    type Error = DummyError;

    fn visit_timestamp(&mut self, _value: DateTime<FixedOffset>) -> Result<(), Self::Error> {
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

fn parse_stream(input: &str) {
    thin_edge_json::parser::parse_str(input, &mut DummyVisitor).unwrap();
}

fn criterion_benchmark(c: &mut Criterion) {
    c.bench_function("parse_stream", |b| {
        b.iter(|| parse_stream(black_box(INPUT)))
    });
}

criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);
