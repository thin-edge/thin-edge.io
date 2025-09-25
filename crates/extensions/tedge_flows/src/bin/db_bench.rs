use anyhow::Context;
use cfg_if::cfg_if;
use tedge_flows::database;
use tedge_flows::database::MeaDb;
use tedge_flows::flow::DateTime;
use tedge_flows::flow::Message;
use tokio::time::Instant;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let mut args = std::env::args();
    let _program_name = args.next();

    // Parse arguments: [series_name] [db_path] [--backend=fjall|sqlite]
    let backend = args.next().unwrap_or_else(|| {
        cfg_if! {
            if #[cfg(feature = "fjall-db")] {
                "fjall".to_string()
            } else if #[cfg(feature = "sqlite-db")] {
                "sqlite".to_string()
            } else {
                compile_error!(
                    "No database backend enabled. Enable either 'fjall-db' or 'sqlite-db' feature."
                );
            }
        }
    });
    let count: usize = args
        .next()
        .unwrap_or_else(|| "1000".to_string())
        .parse()
        .unwrap_or(1000);
    let db_path = format!("bench.{backend}");

    println!(
        "Benchmarking {backend} backend with {} inserts and {} drains from {db_path}",
        count * 5 * 3,
        count * 5
    );

    let mut db: Box<dyn MeaDb> = match backend.as_str() {
        #[cfg(feature = "fjall-db")]
        "fjall" => Box::new(
            database::FjallMeaDb::open(&db_path)
                .await
                .with_context(|| format!("Failed to open Fjall DB at path={db_path}"))?,
        ),

        #[cfg(feature = "sqlite-db")]
        "sqlite" => Box::new(
            database::SqliteMeaDb::open(&db_path)
                .await
                .with_context(|| format!("Failed to open SQLite DB at path={db_path}"))?,
        ),

        _ => {
            eprintln!("Invalid backend: {backend}. Available backends:");
            #[cfg(feature = "fjall-db")]
            eprintln!("  - fjall");
            #[cfg(feature = "sqlite-db")]
            eprintln!("  - sqlite");
            std::process::exit(1);
        }
    };

    let time = Instant::now();
    for _ in 0..5 {
        for series in ["bench1", "bench2", "bench3"] {
            db.store_many(
                series,
                std::iter::repeat_with(|| {
                    (
                        DateTime::now(),
                        Message {
                            timestamp: None,
                            topic: "bench".into(),
                            payload: "a payload for benchmarking read write performance".into(),
                        },
                    )
                })
                .take(count)
                .collect(),
            )
            .await
            .unwrap();
        }
    }
    println!(
        "Inserted {} items (across 15 batches of {count}) in {:?}",
        count * 5 * 3,
        time.elapsed()
    );

    let time = Instant::now();

    let items = db
        .drain_older_than(DateTime::now(), "bench1")
        .await
        .context("Failed to query for all items")?;

    println!("Drained {} items in {:?}", items.len(), time.elapsed());

    Ok(())
}
