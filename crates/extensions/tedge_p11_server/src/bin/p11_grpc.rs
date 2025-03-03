use std::path::Path;
use tokio::net::UnixListener;
use tokio_stream::wrappers::UnixListenerStream;
use tonic::{transport::Server, Request, Response, Status};

// #[derive(Debug, Default)]
// struct MyGreeter;

// #[derive(Debug)]
// pub struct HelloRequest {
//     pub name: String,
// }

// #[derive(Debug)]
// pub struct HelloReply {
//     pub message: String,
// }

// // Define the gRPC service manually
// struct SayHelloService {
//     inner: Arc<MyGreeter>,
// }

// impl UnaryService<HelloRequest> for SayHelloService {
//     type Response = HelloReply;
//     type Future = Pin<Box<dyn Future<Output = Result<Response<Self::Response>, Status>>>>;

//     fn call(&mut self, request: Request<HelloRequest>) -> Self::Future {
//         let inner = Arc::clone(&self.inner);
//         let name = request.into_inner().name;
//         let reply = HelloReply {
//             message: format!("Hello, {}!", name),
//         };

//         Box::pin(async move { Ok(Response::new(reply)) })
//     }
// }

use p11_grpc::greeter_server::{Greeter, GreeterServer};
use p11_grpc::{HelloReply, HelloRequest};
pub mod p11_grpc {
    tonic::include_proto!("p11_grpc");
}

#[derive(Debug, Default)]
pub struct MyGreeter {}

#[tonic::async_trait]
impl Greeter for MyGreeter {
    async fn say_hello(
        &self,
        request: Request<HelloRequest>,
    ) -> Result<Response<HelloReply>, Status> {
        println!("Got a request from {:?}", request.remote_addr());

        let reply = p11_grpc::HelloReply {
            message: format!("Hello {}!", request.into_inner().name),
        };
        Ok(Response::new(reply))
    }
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let socket_path = "/tmp/grpc_socket";

    // Remove old socket if it exists
    if Path::new(socket_path).exists() {
        std::fs::remove_file(socket_path)?;
    }

    let listener = UnixListener::bind(socket_path)?;
    let incoming = UnixListenerStream::new(listener);

    let greeter = MyGreeter::default();

    println!("Server listening on UNIX socket: {}", socket_path);

    Server::builder()
        .add_service(GreeterServer::new(greeter))
        .serve_with_incoming(incoming)
        .await?;

    Ok(())
}
