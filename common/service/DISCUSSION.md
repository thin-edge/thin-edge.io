# Signal Handling and Service architecture

## What is this document about?

What are the options we have when it comes to handling signals in services. How
can (or should) we structure a service? This document comes up with four
different approaches and describes their pros and cons.

Note that we want to use signals for graceful shutdown and to support graceful
reload.

We start from the simplest approaches towards the more high-level ones.

## Approach 1 - Providing a SignalStream

[Branch feature/CIT-83/service-signal-stream][1].

All we provide is a `SignalStream` and minimal structure for a `Service`.
Within the `run` method of your service, you have to take care of handling
signals yourself. This gives you full control over when to handle signals and
when better not.

This leads to code like this:

```rust
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
```

Pros:

* Simple to understand.

* Full control of when signals can interrupt you and when not. 

Cons:

* Every service has to do that over and over again.

## Approach 2 - Service Framework plus Interrupt Flag 

[Branch feature/CIT-83/interruption-flag][2].

We provide more structure for your service. Signal handling is done by the
framework. The framework gives you callbacks how to handle `reload`, how to
`shutdown` and how to run your service loop (`run`) and possibly more.

The `run` method gets passed an `Interruption` struct that you can use to tell
the framework where it is okay to interrupt and where not. Interruption here
really means, where is it okay to terminate the service loop, run a signal
handler (e.g. `reload`) and then enter `run` again (loosing all the previously
created context). Terminating `run` is required because it has to modify the
state (it takes `&mut self`) and so does `reload`. We could get away with `&mut
self` and as such with terminating `run` to run the signal handler but using
interior mutability and just pass in `&self`. But that just shifts complexity
and doesn't really help with shutting down the service.

With this approach `run` may look like this:

```rust
async fn run(&mut self, interruption: &mut Interruption) -> Result<(), Self::Error> {
    loop {
            let (socket, _remote_addr) = self.listener.accept().await?;
            let mut io = BufReader::new(socket);
            let mut line = String::with_capacity(1024);
            io.read_line(&mut line).await?;

            // writes should not be aborted in the middle.
            interruption.disable();
            io.write_all(line.as_bytes()).await?;
            io.flush().await?;
            interruption.enable();
    }
}
```

Note the calls to `disable` and `enable`. Anything between these two lines is
guaranteed to not be interrupted by a signal.

Pros:

* Provides more structure than Approach 1.

* No need to handle signals yourself.

Cons:

* You might easily shoot yourself into your foot by forgetting to reenable the 
  interruption flag.

* Framework implementation is more complex.

* You could instead just spawn a new task and move operations that should not be interrupted there.

## Approach 3 - Using Actors

[Branch feature/CIT-83/using-actors][3].

Actors use (asynchronous) message passing to communicate with other actors.
Actors process messages sequentially. A signal is just another message being
sent to an actor. This is a rather different model than using `async/await`
mainly because actors never block, whereas `async/await` is all about blocking.

This is how the message handler for the "incoming MQTT message" looks like:

```rust
#[actor]
impl Mapper {
    fn on_message(&mut self, message: MqttMessage) {
        log::debug!("Mapping {:?}", message);

        let mapped_message = CumulocityJson::from_thin_edge_json(&message.payload)
            .map(|mapped| MqttMessage::new(&self.config.out_topic, mapped))
            .unwrap_or_else(|error| {
                log::debug!("Mapping error: {}", error);
                MqttMessage::new(&self.config.err_topic, error.to_string())
            });

        if let Some(mqtt_client) = &self.mqtt_client {
            mqtt_client.publish(mapped_message);
        } else {
            log::warn!("Disconnected");
        }
    }
}
```

Pros:

* Much simpler to reason about. There is no interruption. Much less to shoot yourself into the foot. 

* Everything is just message passing. "reload" is a message. "shutdown" is a message.

* You define the behavior of an incoming message. "When this message comes in, do this."

* Some parts of the rumqttc library already operate in this model (or use channels).

* Deadlock-free when done right.

* All actors can run concurrently.

Cons:

* Actors require state to be encoded by a state-machine rather than using
  sequential flow and the stack. You end up using callbacks here and there.

* Rather bigger changes requires.

* Different model. Needs acceptance. 

## Approach 4 - Service as a Stream

[Branch feature/CIT-83/service-as-a-stream-coherence][4].

This uses streams to represent a service. I/O is shifted towards the boundaries
(source and sink). This is a functional approach where mapping and reducing the
stream is purely functional (side-effect free).

For this approach, you do not really need a framework at all. You need to
define your stream sources and stream sinks and then the map&reduce functions.

This is how the main loop of mapper looks like with this approach:

```rust
loop {
    let mapper_config = MapperConfig::default();
    let mqtt_config = MqttConfig::new("test.mosquitto.org", 1883);
    let mqtt_client = MqttClient::connect("tedge-mapper", &mqtt_config).await?;

    let result = select_all(vec![
        signals()?.boxed(),
        errors(&mqtt_client)?.boxed(),
        messages(&mqtt_client, &mapper_config).await?.boxed(),
    ])
    .try_for_each(|output| process_output(output, &mqtt_client))
    .await;

    // Gracefully shutdown
    let _ = mqtt_client.disconnect().await;

    // NOTE: A reload will throw away the remainder of the stream. Any partial
    // resolved future will be aborted.

    match result {
        Ok(()) => return Ok(()),
        Err(MapperError::TerminatedBySignal) => return Ok(()),
        Err(MapperError::ReloadRequested) => {
            println!("Reloading");
        }
        err => return err,
    }
}
```


This defines the stream source:

```rust
let result = select_all(vec![
    signals()?.boxed(),
    errors(&mqtt_client)?.boxed(),
    messages(&mqtt_client, &mapper_config).await?.boxed(),
])
```

So we have signals, errors and messages as inputs.

We don't have an intermediary map'n reduce. The sink is defined by
`process_output`:

```rust
async fn process_output(output: Output, mqtt_client: &MqttClient) -> MapperResult<()> {
    match output {
        Output::OutMessage(out_message) => {
            mqtt_client.publish(out_message).await?;
            Ok(())
        }
        Output::Reload => Err(MapperError::ReloadRequested),
        Output::Terminate => Err(MapperError::TerminatedBySignal),
        Output::Nop => {
            // ignore
            Ok(())
        }
    }
}
```

We terminate the stream as soon as an error occurs.

Pros:

* Stateless, functional approach. Mapping over streams is purely functional.

* Testable. Pass in a stream and compare against the stream that is produced. 

* Reusability.

Cons:

* Still need to handle signals yourself.

* In order to support graceful reload or shutdown, the stream sink
  might need to communicate with the stream source.

* As we are merging three streams into one in our mapper (errors, signals and messages),
  we have to take great care that each of the items produced by the individual streams
  might be aborted at any time when we terminate the stream.

* Exploited concurrency?

[1]: https://github.com/mneumann/thin-edge.io/tree/feature/CIT-83%2Fservice-signal-stream
[2]: https://github.com/mneumann/thin-edge.io/tree/feature/CIT-83%2Finterruption-flag
[3]: https://github.com/mneumann/thin-edge.io/tree/feature/CIT-83%2Fusing-actors
[4]: https://github.com/mneumann/thin-edge.io/blob/feature/CIT-83%2Fservice-as-a-stream-coherence
