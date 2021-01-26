use criterion::{criterion_group, criterion_main, Criterion};
use c8y_json_translator::CumulocityJson;

pub fn criterion_benchmark(c: &mut Criterion) {
    let single_value = r#"{
        "temperature": 23,
        "pressure": 220
     }"#;
    c.bench_function("translate 2 single", |b| b.iter(|| CumulocityJson::from_thin_edge_json(single_value.as_bytes())));
    let mut tag50 = String::new();
    tag50.push_str("{");
    for n in 1..50 {
        tag50.push_str(&format!("\"tagvalue{}\": {},", n, n*10));
    }
    tag50.push_str(&format!("\"tagvalue{}\": {} }}", 50, 50*10));

    c.bench_function("translate 50 single", |b| b.iter(|| CumulocityJson::from_thin_edge_json(tag50.as_bytes())));
}

criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);
