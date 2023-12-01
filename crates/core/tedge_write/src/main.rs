use clap::Parser;

fn main() -> anyhow::Result<()> {
    let args = tedge_write::bin::Args::parse();
    tedge_write::bin::run(args)
}
