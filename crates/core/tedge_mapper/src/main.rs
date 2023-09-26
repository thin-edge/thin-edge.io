use clap::Parser;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let mapper_opt = tedge_mapper::MapperOpt::parse();
    tedge_mapper::run(mapper_opt).await
}
