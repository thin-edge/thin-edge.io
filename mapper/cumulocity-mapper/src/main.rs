mod mapper;

#[tokio::main]
pub async fn main() -> std::result::Result<(),mapper::Error> {
    let configuration
        = mapper::Configuration::default();
    let mut mapper = mapper::EventLoop::new(configuration);

    mapper.run().await
}
