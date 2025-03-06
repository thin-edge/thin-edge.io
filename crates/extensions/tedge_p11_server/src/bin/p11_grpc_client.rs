use hello_world::greeter_client::GreeterClient;
use hello_world::HelloRequest;
use hyper_util::rt::TokioIo;
use tokio::net::UnixStream;
use tonic::transport::Endpoint;
use tower::service_fn;

pub mod hello_world {
    tonic::include_proto!("p11_grpc");
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // https://github.com/hyperium/tonic/blob/master/examples/src/uds/client.rs
    let channel = Endpoint::try_from("http://[::]:50051")?
        .connect_with_connector(service_fn(|_| async {
            let path = "/tmp/grpc_socket";
            Ok::<_, std::io::Error>(TokioIo::new(UnixStream::connect(path).await?))
        }))
        .await?;

    let mut client = GreeterClient::new(channel);
    //connect("/tmp/grpc_socket").await?;

    let request = tonic::Request::new(HelloRequest {
        name: "Tonic".into(),
    });

    let response = client.say_hello(request).await?;

    println!("RESPONSE={:?}", response);

    Ok(())
}
