# Extension Crate Conventions

## TEdgeComponent Trait

All mapper extensions implement `TEdgeComponent`:
```rust
#[async_trait]
pub trait TEdgeComponent: Sync + Send {
    fn session_name(&self) -> &str;
    async fn start(
        &self,
        runtime: &mut Runtime,
        mqtt: &mut MqttActorBuilder,
    ) -> Result<(), anyhow::Error>;
}
```

## Feature Flags

Cloud-specific extensions are feature-gated:
- `#[cfg(feature = "c8y")]` — Cumulocity IoT
- `#[cfg(feature = "aws")]` — AWS IoT Core
- `#[cfg(feature = "azure")]` — Azure IoT Hub

## Actor Composition

Extensions wire actors together using builders and `DynSender`:
- Create actor builders in `start()`
- Connect actors via `connect_sink()`, `connect_source()`, `get_sender()`
- Spawn all actors on the `Runtime`

## Reference Implementation

`c8y_mapper_ext` is the most feature-complete mapper — use it as the primary reference for complex extensions. For simpler patterns, see `tedge_timer_ext` or `tedge_signal_ext`.
