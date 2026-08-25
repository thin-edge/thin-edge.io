# Facet-powered tedge config support

* Date: __2026-07-23__
* Status: __Approved__

## Background

The existing `tedge config` command is powered by a very complicated macro that generates highly-specialised code with a rigid structure. While the macro makes it easy to add new configuration options, extending the functionality of `tedge config` (e.g. to add a new subcommand or support for custom mapper configuration) is very difficult. Across the thin-edge team, there is virtually no knowledge of how the macro works or how to extend it.

The macro currently allows us define the configuration schema in a nested struct-like manner with minimal boilerplate. This has worked nicely, with new configurations being essentially trivial for developers to add. As a result, we'd like to preserve this UI aspect of the current `tedge_config` design. Here is a more concrete example of the macro in use:

```rust
define_tedge_config! {
  device: {
    id: String,

    #[tedge_config(rename = "type")]
    #[tedge_config(default = "thin-edge.io", example = "thin-edge.io")]
    ty: String,
  }
}
```

We have recently added support for custom cloud connections. Users can now configure a mapper using flows and a bridge to connect to a cloud platform of their choosing. The configuration for these currently needs to be managed manually by the user editing the toml file directly. This is error-prone (there is no validation as people configure the file) and means users have to read the documentation to discover configuration keys, there is no solution analogous to `tedge config list`.

Adding support for these custom mapper configs would involve some major rearchitecting of the existing macro, including some notably challenging features. These are discussed in more detail below, and are sufficiently complex that the macro would need to be significantly overhauled to support them, hence the proposal to simplify the macro's architecture in the process.

### Generalised mapper names

The existing cloud config keys are hardcoded into the configuration schema (e.g. `aws.url`, `c8y.smartrest_templates`). Since custom mappers can have arbitrary names, we need a mechanism to allow for arbitrary mapper names without risking conflicts with existing config keys.

For instance, the syntax could look something like this:

```bash
tedge config set mappers.thingsboard.url thingsboard.example.com
TEDGE_CONFIG_MAPPERS_THINGSBOARD_URL=thingsboard.example.com tedge config get mappers.thingsboard.url
```

### Config schema federation

The existing macro requires a monolithic schema definition, and it is already becoming unwieldy. The mapper-specific configuration should be able to be defined separately from the core agent configuration.

For instance, the syntax could look something like this:

```rust
define_config! {
  Tedge {
    device: {
      #[tedge_config(default(value = "/etc/tedge/device-certs/tedge-certificate.pem"))]
      cert_path: Utf8PathBuf,

      #[tedge_config(default(value = "/etc/tedge/device-certs/tedge-private-key.pem"))]
      key_path: Utf8PathBuf,
    }
  }
}

define_config! {
  Mapper {
    url: String,

    device: {
      #[tedge_config(default(from_root = "device.cert_path"))]
      cert_path: Utf8PathBuf,

      #[tedge_config(default(from_root = "device.key_path"))]
      key_path: Utf8PathBuf,
    }
  }
}
```

## Goals

* A `tedge config` command that supports custom mappers and is easy to extend with more functionality in the future
* Mapper configurations defined separately from the core tedge configuration schema
* Preservation of all the existing `tedge config` functionality, notably the ability for the mapper `device` configuration to fall back to the core `device` configuration values if not explicitly defined.

## Design

The proposed solution is based around the [facet](https://facet.rs) crate, which provides a mechanism for reflection in rust. This will allow us to define the configuration schema as a struct that implements the `Facet` trait, then implement the code to power `tedge config` in "normal" rust code, rather than through the macro.

There will still be a macro to generate the configuration schema, but it will be much simpler and will not generate any code for the `tedge config` command itself. This allows us to retain the benefits of nested struct syntax and the attributes we have for default values and examples.

### Why facet?

The root of the complexity in the existing macro is that it needs to generate a very large amount of infrastructure for supporting the `tedge config` command. There is something of a direct conflict here between the ability to interact with the configuration by setting a string key to a value to be parsed from a string, and the ability for the code to consume the config as a normal Rust struct.

The macro currently needs to do all the heavy lifting as we have no way to introspect the configuration schema at runtime. `facet` is a crate designed to provide a reflection mechanism in Rust. This means we can define a normal Rust struct for the schema, then introspect on it at runtime with normal Rust code. Moving the logic out of the macro will make it much simpler to understand what's going on - "normal" Rust functions are much easier to poke around with, and a simpler macro is easier to understand, e.g. by reading `cargo expand` output.

### Compile-time vs runtime validation

The main trade-off of this approach is a reduction in compile-time safety. With the existing macro, we can validate the types of default values at compile time, as well as validating other aspects of the configuration schema (e.g. is key.x defaults to key.y, we validate statically that key.y exists).

With `facet`, the default values are stringified then parsed at runtime. This means a default value of the wrong type is only caught when the config reader struct is constructed as the program runs, rather than at compile time. We can mitigate this issue by adding auto-generated tests that validate the default values in the macro, so any configurations defined by the macro are automatically verified.

The validation for keys in `from_key` attributes also needs to be done at runtime. We can do this early in the process of constructing a DTO, so we catch any errors before it is used, and therefore lightweight tests that touch any of the configuration are sufficient to catch such issues.

With the `from_root` attribute, we can validate this when constructing the federated configuration (i.e. the object that contains both the mapper configuration and the core tedge configuration it depends upon). In this example, the fact we are able to do runtime validation is the very reason we _can_ support this federated configuration.

We need to ensure that the macro is adequately tested with negative tests in particular to ensure that invalid configurations are caught without depending on the consumer of the macro manually writing tests.

## Migration structure

The migration to the facet-powered configuration will be a large change, so the implementation will be done in a series of PRs to make it easier to review. These will be focused on providing meaningful externally observable behaviour changes in each PR.

PR 1: Engine crates: The underlying logic for the macro and the `tedge config` command is implemented. This serves as the groundwork for the rest of the change. No changes to the existing `tedge config` behaviour is made in this PR, nor is the new engine wired into any production code paths.

PR 2: `mappers.*` in `tedge config`: The new engine is wired into the `tedge config` command, behind a feature flag. This PR adds support for the `mappers.*` keys to be read and written via the CLI, but does not change any existing behaviour.

PR 3: Macro parity: The remaining `define_tedge_config` attributes are ported to the new macro. These are required to support the tedge.toml schema, but not the mapper.toml schema.

PR 4: Migrate the full tedge.toml schema to the new macro, leaving the existing production paths untouched. This defines the full schema in the new macro and allows us to write tests verifying the two implementations behave identically.

PR 5: Generalise the key routing to support both backends. This serves as the groundwork for swapping which backend is used for the `tedge config` command as well as the other consumers of the configuration.

PR 6: Swap the backend to the new engine. This PR switches the backend used for interacting with both tedge.toml and mapper.toml to the new engine.

PR 7: Clean up the now unused code, including the old macro and the config definition.
