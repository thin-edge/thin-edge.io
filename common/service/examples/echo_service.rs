use async_trait::async_trait;
use service::*;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpListener,
};

struct EchoService {
    listener: TcpListener,
}

#[async_trait]
impl Service for EchoService {
    const NAME: &'static str = "echo-service";

    type Error = std::io::Error;

    type Configuration = std::net::SocketAddr;

    async fn setup(sockaddr: Self::Configuration) -> Result<Self, Self::Error> {
        let listener = TcpListener::bind(sockaddr).await?;
        Ok(Self { listener })
    }

    async fn run(&mut self) -> Result<(), Self::Error> {
        loop {
            let (mut socket, _) = self.listener.accept().await?;
            let mut buffer = String::with_capacity(1024);
            socket.read_to_string(&mut buffer).await?;
            socket.write_all(buffer.as_bytes()).await?;
        }
    }

    async fn reload(&mut self) -> Result<(), Self::Error> {
        Ok(())
    }

    async fn shutdown(self) -> Result<(), Self::Error> {
        Ok(())
    }
}

///
/// Test with netcat: `echo abc | nc -N localhost 8080`
///
#[tokio::main]
async fn main() {
    ServiceRunner::<EchoService>::new()
        .run_with_config("127.0.0.1:8080".parse().unwrap())
        .await
        .unwrap();
}
