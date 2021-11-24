use c8y_translator::json;
use criterion::{criterion_group, criterion_main, Criterion};

pub fn criterion_benchmark(c: &mut Criterion) {
    translate_ref_measurement(c);
    translate_2_measurements(c);
    translate_50_measurements(c);
    translate_17x3_multi_measurements(c);
}

const REFERENCE_THIN_EDGE_JSON: &str = r#"{
            "time": "2021-06-22T17:03:14.123456789+05:00",
            "temperature": 25.01,
            "location": {
                  "latitude": 32.54,
                  "longitude": -117.67,
                  "altitude": 98.6
              },
            "pressure": 98.01
        }"#;

fn translate_ref_measurement(c: &mut Criterion) {
    let id = "Translate reference measurement";
    sanity_check_translate_reference_thin_edge_json()
        .expect("Expect a valid thin-edge-json message");

    c.bench_function(id, |b| {
        b.iter(|| json::from_thin_edge_json(REFERENCE_THIN_EDGE_JSON))
    });
}

fn translate_2_measurements(c: &mut Criterion) {
    let id = "Translate 2 measurements";
    let message = r#"{
            "temperature": 12.34,
            "pressure": 56.78
        }"#;
    sanity_check(message);

    c.bench_function(id, |b| b.iter(|| json::from_thin_edge_json(message)));
}

fn translate_50_measurements(c: &mut Criterion) {
    let id = "Translate 50 measurements";
    let message = flat_message(50);
    sanity_check(&message);

    c.bench_function(id, |b| b.iter(|| json::from_thin_edge_json(&message)));
}

fn translate_17x3_multi_measurements(c: &mut Criterion) {
    let id = "Translate 17x3 multi-measurements";
    let message = group_message(17, 3);
    sanity_check(&message);

    c.bench_function(id, |b| b.iter(|| json::from_thin_edge_json(&message)));
}

fn flat_message(n: u64) -> String {
    let mut message = String::with_capacity(5000);
    let mut sep = "{";
    for i in 0..n {
        message.push_str(&format!("{}\n\t\"measurement_{}\" : {}", sep, i, i * 10));
        sep = ","
    }
    message.push_str("\n}");
    message
}

fn group_message(n_grp: u64, n_per_grp: u64) -> String {
    let mut message = String::with_capacity(5000);
    let mut sep = "{";
    for i in 0..n_grp {
        message.push_str(&format!("{}\n\t\"group_{}\" : {{", sep, i));
        sep = "";
        for j in 0..n_per_grp {
            message.push_str(&format!(
                "{}\n\t\"measurement_{}_{}\" : {}",
                sep,
                i,
                j,
                i * j
            ));
            sep = ","
        }
        message.push_str("\n\t}");
        sep = ","
    }
    message.push_str("\n}");
    message
}

fn sanity_check(message: &str) {
    json::from_thin_edge_json(message).expect("Expect a valid thin-edge-json message");
}

fn sanity_check_translate_reference_thin_edge_json() -> Result<(), anyhow::Error> {
    let output = json::from_thin_edge_json(REFERENCE_THIN_EDGE_JSON)?;

    let simple_c8y_json = serde_json::json!({
        "type": "ThinEdgeMeasurement",
        "time": "2021-06-22T17:03:14.123456789+05:00",
        "temperature": {
            "temperature": {
                "value": 25.01
             }
        },
       "location": {
            "latitude": {
               "value": 32.54
             },
            "longitude": {
              "value": -117.67
            },
            "altitude": {
              "value": 98.6
           }
      },
     "pressure": {
        "pressure": {
             "value": 98.01
        }
      }
    });

    assert_json_diff::assert_json_eq!(
        serde_json::from_slice::<serde_json::Value>(output.as_bytes())?,
        simple_c8y_json
    );

    Ok(())
}

criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);
