// Checking that fsync is actually called on the updated file and its directory
// $ cargo build --release --example atomic_write
// $ strace -f target/release/examples/atomic_write 2>&1 | grep sync

use tedge_utils::fs::atomically_write_file_async;

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    Ok(atomically_write_file_async("/tmp/foo.txt", b"Hello from an example!\n").await?)
}

/*
use tedge_utils::fs::atomically_write_file_sync;

fn main() -> Result<(),anyhow::Error> {
    Ok(atomically_write_file_sync("/tmp/foo.txt", b"Hello from an example!\n")?)
}
 */
