mod mapper;

fn main() {
    mapper::run(mapper::Configuration::default()).unwrap();
}