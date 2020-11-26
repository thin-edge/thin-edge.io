mod mapper;

fn main() {
    let configuration = mapper::Configuration::default();
    let mut mapper = mapper::EventLoop::new(configuration).unwrap();
    mapper.run().unwrap();
}
