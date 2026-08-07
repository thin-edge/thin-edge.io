# Refactor: Push from_root resolution into ConfigOps::read_with_root

## Problem

`FederatedConfig::read` has special-case post-hoc logic to resolve `FromRoot` defaults:
it calls `mount.source.read()`, and if that returns `None`, iterates `root_defaults()`
doing a linear scan to find a match, then reads from the root mount.

This means `FederatedConfig` knows about from_root semantics, which should be internal
to the mount.

## Proposed change

Add `read_with_root` to `ConfigOps`. The federated layer passes a closure that reads
from the root mount. The mount's `config_get_with_defaults` already handles
`DefaultSpec::FromRoot` — it just needs a non-`None` resolver.

```rust
// ops.rs — trait
pub trait ConfigOps {
    fn get(&self, key: &str) -> Result<Option<String>, ConfigError>;
    fn read(&self, key: &str) -> Result<Option<String>, ConfigError>;
    fn read_with_root(
        &self,
        key: &str,
        root: &dyn Fn(&str) -> Option<String>,
    ) -> Result<Option<String>, ConfigError> {
        self.read(key)
    }
    fn mutate(&mut self, key: &str, action: Action) -> Result<(), ConfigError>;
    fn entries(&self) -> Vec<KeyEntry>;
    // root_defaults() removed
}

// ops.rs — impl for TypedConfigOps
fn read_with_root(
    &self,
    key: &str,
    root: &dyn Fn(&str) -> Option<String>,
) -> Result<Option<String>, ConfigError> {
    config_get_with_defaults(&self.dto, key, self.manager.defaults(), Some(root))
}

// federated.rs — simplified read
pub fn read(&self, full_key: &str) -> Result<Option<String>, ConfigError> {
    let (mount, local_key) = self.route(full_key)?;
    let root_resolver = |key: &str| -> Option<String> {
        self.root_mount()
            .and_then(|root| root.source.read(key).ok().flatten())
    };
    mount.source.read_with_root(&local_key, &root_resolver)
}
```

## What this eliminates

- `root_defaults()` method from `ConfigOps` trait
- Linear scan of root_defaults on every read
- Special-case from_root handling in `FederatedConfig::read`

## Open question

Does `build_reader_with_root` also need to go through this path? If mappers call
`build_reader` at startup, they'd want the root resolver wired in too — but that's
a separate concern from the per-key read path.
