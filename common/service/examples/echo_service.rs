use async_trait::async_trait;
use futures::prelude::stream::*;
use service::*;
use std::net::{AddrParseError, SocketAddr};
use std::sync::atomic::{AtomicUsize, Ordering};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    net::{TcpListener, TcpStream},
    select,
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

    async fn run(&mut self, signal_stream: SignalStream) -> Result<(), Self::Error> {
        self.run_loop(signal_stream).await
    }

    async fn shutdown(self) -> Result<(), Self::Error> {
        // `drop` will clean up for us.
        Ok(())
    }
}

impl EchoService {
    async fn reload(&mut self) -> Result<(), EchoServiceError> {
        match self.config.listen_on()? {
            updated_listen_on if updated_listen_on != self.listen_on => {
                log::info!(
                    "Changing listen_on from {:?} to {:?}",
                    self.listen_on,
                    updated_listen_on
                );
                let new_listener = TcpListener::bind(updated_listen_on).await?;
                self.listener = new_listener;
                self.listen_on = updated_listen_on;
            }
            _ => {
                log::info!("Configuration has not changed");
            }
        }
        Ok(())
    }

    async fn run_loop(&mut self, mut signal_stream: SignalStream) -> Result<(), EchoServiceError> {
        loop {
            select! {
                signal = signal_stream.next() => {
                    match signal {
                        Some(SignalKind::Hangup) => {
                            log::info!("Got SIGHUP");
                            self.reload().await?;
                        }
                        Some(SignalKind::Interrupt) => {
                            log::info!("Got SIGINT");
                            return Ok(());
                        }
                        Some(SignalKind::Terminate) => {
                            log::info!("Got SIGTERM");
                            return Ok(());
                        }
                        _ => {
                            // ignore
                        }
                    }
                }
                accept = self.listener.accept() => {
                    match accept {
                        Ok((socket, _remote_addr)) => {
                            let _handler = tokio::spawn(async move {
                                let _ = handle_request(socket).await;
                            });
                        }
                        Err(err) => {
                            log::info!("Accept failed with: {:?}", err);
                            return Err(err.into());
                        }
                    }
                }
            }
        }
    }
}

async fn handle_request(socket: TcpStream) -> std::io::Result<()> {
    let mut io = BufReader::new(socket);
    let mut line = String::with_capacity(1024);
    io.read_line(&mut line).await?;
    io.write_all(line.as_bytes()).await?;
    io.flush().await?;
    Ok(())
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
