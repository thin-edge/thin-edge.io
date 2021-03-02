use async_trait::async_trait;
use service::*;
use std::net::{AddrParseError, SocketAddr};
use std::sync::atomic::{AtomicUsize, Ordering};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    net::TcpListener,
};

#[derive(thiserror::Error, Debug)]
enum EchoServiceError {
    #[error("IoError {0}")]
    IoError(#[from] std::io::Error),

    #[error("Configuration parse error: {0:?}")]
    ConfigurationParseError(#[from] AddrParseError),
}

struct EchoServiceConfig {
    next_port: AtomicUsize,
}

impl Default for EchoServiceConfig {
    fn default() -> Self {
        Self {
            next_port: AtomicUsize::new(8080),
        }
    }
}

impl EchoServiceConfig {
    fn listen_on(&self) -> Result<SocketAddr, EchoServiceError> {
        // The `port` and `host` would normally read from a config file.

        // To simulate a changed config file, each time `listen_on` is called,
        // we increment the `next_port` by one.
        let port = self.next_port.fetch_add(1, Ordering::Relaxed);

        Ok(format!("127.0.0.1:{}", port).parse()?)
    }
}

struct EchoService {
    listener: TcpListener,
    listen_on: SocketAddr,
    config: EchoServiceConfig,
}

#[async_trait]
impl Service for EchoService {
    const NAME: &'static str = "echo-service";

    type Error = EchoServiceError;

    type Configuration = EchoServiceConfig;

    async fn setup(config: Self::Configuration) -> Result<Self, Self::Error> {
        let listen_on = config.listen_on()?;
        log::info!("Starting service on {:?}", listen_on);
        let listener = TcpListener::bind(listen_on).await?;
        Ok(Self {
            listener,
            listen_on,
            config,
        })
    }

    async fn run(&mut self) -> Result<(), Self::Error> {
        loop {
            let (socket, _remote_addr) = self.listener.accept().await?;
            let mut io = BufReader::new(socket);
            let mut line = String::with_capacity(1024);
            io.read_line(&mut line).await?;
            io.write_all(line.as_bytes()).await?;
            io.flush().await?;
        }
    }

    async fn reload(self) -> Result<Self, Self::Error> {
        match self.config.listen_on()? {
            updated_listen_on if updated_listen_on != self.listen_on => {
                log::info!(
                    "Changing listen_on from {:?} to {:?}",
                    self.listen_on,
                    updated_listen_on
                );
                let new_listener = TcpListener::bind(updated_listen_on).await?;
                let updated_service = Self {
                    listener: new_listener,
                    listen_on: updated_listen_on,
                    config: self.config,
                };
                Ok(updated_service)
            }
            _ => {
                log::info!("Configuration has not changed");
                Ok(self)
            }
        }
    }

    async fn shutdown(self) -> Result<(), Self::Error> {
        // `drop` will clean up for us.
        Ok(())
    }
}

///
/// Test with netcat: `echo abc | nc localhost 8080`
///
#[tokio::main]
async fn main() {
    env_logger::init();

    ServiceRunner::<EchoService>::new()
        .run_with_default_config()
        .await
        .unwrap();
}
