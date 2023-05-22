Defines the necessary structures to create a tedge config struct

For a complete example of its usage, see the `macros.rs` file inside the
examples folder of this crate.

# Output
This macro outputs a few different types:
- `TEdgeConfigDto` ([example](example::TEdgeConfigDto)) --- A data-transfer
  object, used for reading and writing to toml
- `TEdgeConfigReader` ([example](example::TEdgeConfigReader)) --- A struct to
  read configured values from, populating values with defaults if they exist
- `ReadableKey` ([example](example::ReadableKey)) --- An enum of all the
  possible keys that can be read from the configuration, with
  [`FromStr`](std::str::FromStr) and [`Display`](std::fmt::Display)
  implementations.
- `WritableKey` ([example](example::WritableKey)) --- An enum of all the
  possible keys that can be written to the configuration, with
  [`FromStr`](std::str::FromStr) and [`Display`](std::fmt::Display)
  implementations.
- `ReadOnlyKey` ([example](example::ReadOnlyKey)) --- An enum of all the
  possible keys that can be read from, but not written to, the configuration,
  with [`FromStr`](std::str::FromStr) and [`Display`](std::fmt::Display)
  implementations.

# Attributes
## `#[tedge_config(...)]` attributes
| Attribute                                     | Supported for                                 | Summary                                                                   |
| --------------------------------------------- | --------------------------------------------- | ------------------------------------------------------------------------- |
| [`rename`](#rename)                           | fields/groups                                 | Renames a field or group in serde and the `tedge config` key              |
| [`deprecated_name`](#dep-name)                | fields/groups                                 | Adds an alias for a field or group in serde/`tedge config`                |
| [`deprecated_key`](#dep-key)                  | fields                                        | Adds an alias for the field or group in serde/`tedge config`              |
| **Doc comments**                              | [fields](#docs-fields)/[groups](#docs-groups) | Adds a description of a key in `tedge config` docs                        |
| [`example`](#examples)                        | fields                                        | Adds an example value to `tedge config` docs                              |
| [`note`](#notes)                              | fields                                        | Adds a highlighted note to `tedge config` docs                            |
| [`reader(skip)`](#reader-skip)                | groups                                        | Omits a group from the reader struct entirely                             |
| [`reader(private)`](#reader-priv)             | fields/groups                                 | Stops the field from the reader struct being marked with `pub`            |
| [`default(value)`](#default-lit)              | fields                                        | Sets the default value for a field from a literal                         |
| [`default(variable)`](#default-var)           | fields                                        | Sets the default value for a field from a variable                        |
| [`default(from_key)`](#from-key)              | fields                                        | Sets the default value for a field to the value of another field          |
| [`default(from_optional_key)`](#from-opt-key) | fields                                        | Sets the default value for a field to the value of another field          |
| [`default(function)`](#default-fn)            | fields                                        | Specifies a function that will be used to compute a field's default value |
| [`readonly(...)`](#readonly)                  | fields                                        | Marks a field as read-only                                                |

## Other attributes
### `#[doku(as = "...")]`
Some types are not known to [`doku`], and therefore do not implement
[`doku::Document`] (which is required for all fields). In order to resolve this
error, you can use a `#[doku(as = "...")]` attribute like so:

```rust
# use tedge_config_macros::*;
# #[derive(::thiserror::Error, Debug)]
# pub enum ReadError { #[error(transparent)] NotSet(#[from] ConfigNotSet)}
use camino::Utf8PathBuf;
use std::path::PathBuf;

define_tedge_config! {
  device: {
    #[doku(as = "PathBuf")]
    cert_path: Utf8PathBuf,
  }
}
```

The actual type information isn't currently used for anything, but it's usually
possible to find a close match. Custom types can also implement the trait very
easily.

```rust
# use tedge_config_macros::*;
# #[derive(::thiserror::Error, Debug)]
# pub enum ReadError { #[error(transparent)] NotSet(#[from] ConfigNotSet)}
#
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, Eq, PartialEq)]
#[serde(transparent)]
pub struct ConnectUrl(String); 

impl doku::Document for ConnectUrl {
    fn ty() -> doku::Type {
        // Just call `ty()` on a similar enough type
        String::ty()
    }
}

define_tedge_config! {
  c8y: {
    // We now don't need `#[doku(as = "String")]`
    url: ConnectUrl,
  }
}
#
# impl std::str::FromStr for ConnectUrl {
#   type Err = std::convert::Infallible;
#   fn from_str(s: &str) -> Result<Self, Self::Err> { Ok(Self(s.to_owned())) }
# }
# impl std::fmt::Display for ConnectUrl {
#   fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
#       self.0.fmt(f)
#   }
# }
```

## Denied attributes
Some `tedge_config` attributes are translated under the hood to `serde` or
`doku` attributes, as well as adding some additional behaviour (e.g. [example
tests](#example-tests)). To avoid these attributes being used directly,
`define_tedge_config!` will emit a compiler error if you attempt to use these
attributes.

```rust compile_fail
# use tedge_config_macros::*;
# #[derive(::thiserror::Error, Debug)]
# pub enum ReadError { #[error(transparent)] NotSet(#[from] ConfigNotSet)}
#
define_tedge_config! {
  device: {
    #[serde(rename = "type")]
    ty: String,
  }
}
```

## Naming
### <a name="rename"></a>Customising config keys: `#[tedge_config(rename = "new_name")]`
```rust
# use tedge_config_macros::*;
# #[derive(::thiserror::Error, Debug)]
# pub enum ReadError { #[error(transparent)] NotSet(#[from] ConfigNotSet)}
#
define_tedge_config! {
  device: {
    #[tedge_config(rename = "type")]
    ty: String,
  }
}

assert!("device.ty".parse::<WritableKey>().is_err());
assert!("device.type".parse::<WritableKey>().is_ok());
```

Fields and groups can be renamed (i.e. such that the field or group name as
observed by serde and `tedge config` does not match the Rust identifier) using
the `rename` attribute. This is generally useful for keys like `device.type`,
where the `type` field conflicts with the Rust `type` keyword. Instead we can
call the field `ty` and rename it to `type` as shown above (which is more
ergonomic and idiomatic than using a [raw
identifier](https://doc.rust-lang.org/rust-by-example/compatibility/raw_identifiers.html)
to solve the problem instead).

### <a name="dep-name"></a>Deprecated names: `#[tedge_config(deprecated_name = "old_name")]`
```rust
# use tedge_config_macros::*;
# #[derive(::thiserror::Error, Debug)]
# pub enum ReadError { #[error(transparent)] NotSet(#[from] ConfigNotSet)}
#
define_tedge_config! {
  group: {
    #[tedge_config(deprecated_name = "old_name")]
    new_name: String,
  }
}

assert_eq!(
  "group.old_name".parse::<WritableKey>().unwrap(),
  "group.new_name".parse::<WritableKey>().unwrap(),
);
```

Fields and groups can be updated in a backwards compatible manner (i.e. the name
can be changed between different thin-edge.io versions)  with the
`deprecated_name` attribute. This is useful for e.g. renaming the `azure` group
to `az`, or renaming a field to include an `_` when it previously didn't.

The alias that is created will apply to keys supplied to `tedge config`, values
read from `tedge.toml` and `TEDGE_` environment variables, and the field will be
automatically renamed in `tedge.toml` next time `tedge` writes to `tedge.toml`.

### <a name="dep-key"></a>Deprecated keys: `#[tedge_config(deprecated_key = "some.old.key")]`
```rust
# use tedge_config_macros::*;
# #[derive(::thiserror::Error, Debug)]
# pub enum ReadError { #[error(transparent)] NotSet(#[from] ConfigNotSet)}
#
define_tedge_config! {
  mqtt: {
    bind: {  
      #[tedge_config(deprecated_key = "mqtt.port")]
      port: u16,
    }
  }
}

assert_eq!(
  "mqtt.port".parse::<WritableKey>().unwrap(),
  "mqtt.bind.port".parse::<WritableKey>().unwrap(),
);
```
More complex field name updates can be carried out with the `deprecated_key`
attribute. This is required when a field is moved to a different group, e.g.
renaming `mqtt.port` to `mqtt.bind.port`.

#### Note
The `deprecated_key` attribute only adds aliases that will be handled by `tedge
config` and `TEDGE_` environment variables, you will also need to add a TOML
migration step if making a change like this to ensure the field is moved in
`tedge.toml` too.

## Documentation
### <a></a>Doc comments: `/// Comment` or `#[doc = "Comment"]`
```rust
# use tedge_config_macros::*;
# #[derive(::thiserror::Error, Debug)]
# pub enum ReadError { #[error(transparent)] NotSet(#[from] ConfigNotSet)}
#
define_tedge_config! {
  /// You can document groups too, but this won't be shown to users
  az: {
    /// Endpoint URL of Azure IoT tenant
    url: ConnectUrl,
  }
}
```

<a name="docs-fields"></a>Doc comments for **fields** are automatically
converted to documentation in `tedge config list --doc`. All line breaks are
removed before this. They are also preserved on the generated `Reader` and `Dto`
structs.

<a name="docs-groups"></a>Doc comments for **groups** are preserved on the
`Reader` and `Dto` structs, and ignored by `tedge config list --doc`.

### <a name="examples"></a>Examples: `#[tedge_config(example = "value")]`
```rust
# use tedge_config_macros::*;
# #[derive(::thiserror::Error, Debug)]
# pub enum ReadError { #[error(transparent)] NotSet(#[from] ConfigNotSet)}
#
define_tedge_config! {
  az: {
    #[tedge_config(example = "myazure.azure-devices.net")]
    url: ConnectUrl,

    mapper: {
      // Examples are always specified using string literals
      #[tedge_config(example = "true")]
      timestamp: bool,
    }
  }
}
```

Examples can be added using `#[tedge_config(example = "value")]`. The value
provided must be a string literal.

#### Example tests
A test will automatically be generated for each example field to check that the
provided value can be deserialised to the field's type (i.e. the example would
work if used with `tedge config set`)

For the code above, `define_tedge_config!` will create a test to verify
`"myazure.azure-devices.net".parse::<ConnectUrl>().is_ok()` and another test
will verify `"true".parse::<bool>().is_ok()`.

### <a name="notes"></a>Notes 
```rust
# use tedge_config_macros::*;
# #[derive(::thiserror::Error, Debug)]
# pub enum ReadError { #[error(transparent)] NotSet(#[from] ConfigNotSet)}
use std::path::PathBuf;
use camino::Utf8PathBuf;

define_tedge_config! {
  c8y: {
    #[tedge_config(note = "The value can be a directory path as well as the path of the direct certificate file.")]
    #[doku(as = "PathBuf")]
    root_cert_path: Utf8PathBuf,
  }
}
```

Additional notes about fields can be added using `#[tedge_config(note =
"content")]`. This will be added to `tedge config list --doc` on a separate
line, with a coloured heading to make it more distinctive.

## Reader: `#[tedge_config(reader(...))]`
There are some options to customise the fields in the generated `Reader` struct.

### <a name="reader-skip"></a> Skipping fields: `#[tedge_config(reader(skip))]`
```rust
# use tedge_config_macros::*;
# #[derive(::thiserror::Error, Debug)]
# pub enum ReadError { #[error(transparent)] NotSet(#[from] ConfigNotSet)}
#
define_tedge_config! {
  #[tedge_config(reader(skip))]
  config: {
    version: u32,
  }
}

assert!("config.version".parse::<WritableKey>().is_err());
```

Groups can be omitted from the `Reader` struct entirely. This was added to
support the `config.version` field, which is used to manage `tedge.toml`
migrations. Since this is just a detail about the `tedge.toml` version, it must
not be exposed in `tedge config` or to other tedge crates. Omitting the group
from the reader using this attribute

### <a name="reader-priv"></a> Private fields: `#[tedge_config(reader(private))]`
```rust compile_fail
# use tedge_config_macros::*;
# #[derive(::thiserror::Error, Debug)]
# pub enum ReadError { #[error(transparent)] NotSet(#[from] ConfigNotSet)}
#
mod tedge_config {
  define_tedge_config! {
    #[tedge_config(reader(skip))]
    config: {
      version: u32,
    }
  }
}

use tedge_config::*;

let reader = TEdgeConfigReader::from_dto(&TEdgeConfigDto::default(), &TEdgeConfigLocation::default());
println!("{}", tedge_config::reader.config.version); // compile error! The field is not public
```

Occasionally, you may want to add custom logic, such as
[`all_or_nothing`](`crate::all_or_nothing::all_or_nothing`) to field accesses.
This attribute prevents the relevant `Reader` struct field from being marked
with `pub`, which prevents other crates from accessing the value directly. The
value is still exposed via `tedge config` to read from and write to.

## Defaults
There are a variety of ways to specify default values for fields. Supplying a
default value for a field will result in the reader field being non-optional
(with the exception of [`from_optional_key`](#from-opt-key)). 

### <a name="default-lit"></a> Literals: `#[tedge_config(default(value = ...))]`
```rust
# use tedge_config_macros::*;
# #[derive(::thiserror::Error, Debug)]
# pub enum ReadError { #[error(transparent)] NotSet(#[from] ConfigNotSet)}
#
define_tedge_config! {
  mqtt: {
    bind: {
      #[tedge_config(default(value = 1883_u16))]
      port: u16,
    }
  }
}

let reader = TEdgeConfigReader::from_dto(&TEdgeConfigDto::default(), &TEdgeConfigLocation::default());
assert_eq!(reader.mqtt.bind.port, 1883);
```
If the value can be specified as a literal (e.g. for `bool`, primitive numeric
types (`u16`, `i32`, etc.), and strings), you can use the `value` specifier for
a default value.

The implematation calls `.into()` on the provided value, so any field with a
type that implements `From<T>`, where `T` is the type of the literal, can be
filled using this method. As shown above, numeric literals may have to specify
their type [using a
suffix](https://doc.rust-lang.org/rust-by-example/primitives/literals.html). As
the value is a simply a literal, it does not have to be quoted.

### <a name="default-var"></a> Variables/constants: `#[tedge_config(default(variable = "..."))]`
```rust
# use tedge_config_macros::*;
# #[derive(::thiserror::Error, Debug)]
# pub enum ReadError { #[error(transparent)] NotSet(#[from] ConfigNotSet)}
use std::net::IpAddr;
use std::net::Ipv4Addr;
use camino::Utf8PathBuf;
use std::path::PathBuf;

const DEFAULT_CERT_PATH: &str = "/etc/ssl/certs";

define_tedge_config! {
  mqtt: {
    bind: {
      #[tedge_config(default(variable = "Ipv4Addr::LOCALHOST"))]
      address: IpAddr,
    }
  },
  c8y: {
    // This will default to `DEFAULT_CERT_PATH.into()`, so we
    // can use a `&str` const as the default for this field
    #[tedge_config(default(variable = "DEFAULT_CERT_PATH"))]
    #[doku(as = "PathBuf")]
    root_cert_path: Utf8PathBuf,
  }
}

let reader = TEdgeConfigReader::from_dto(&TEdgeConfigDto::default(), &TEdgeConfigLocation::default());
assert_eq!(reader.mqtt.bind.address, Ipv4Addr::LOCALHOST);
```

Instead of providing a value as a literal, you can a reference a `const` value
using the `variable` specifier. Like `value`, the generated implematation calls
`.into()` on the constant.

### <a name="from-key"></a> Fallback keys/derived keys `#[tedge_config(default(from_key = "..."))]`
```rust
# use tedge_config_macros::*;
# #[derive(::thiserror::Error, Debug)]
# pub enum ReadError { #[error(transparent)] NotSet(#[from] ConfigNotSet)}
#
define_tedge_config! {
  mqtt: {
    bind: {
      #[tedge_config(default(value = 1883_u16))]
      port: u16
    },
    client: {
      #[tedge_config(default(from_key = "mqtt.bind.port"))]
      port: u16,
    }
  }
}

let mut dto = TEdgeConfigDto::default();
let reader = TEdgeConfigReader::from_dto(&dto, &TEdgeConfigLocation::default());
assert_eq!(reader.mqtt.client.port, reader.mqtt.bind.port);

dto.mqtt.bind.port = Some(2387);
let reader = TEdgeConfigReader::from_dto(&dto, &TEdgeConfigLocation::default());
assert_eq!(reader.mqtt.client.port, reader.mqtt.bind.port);
```

Using `from_key`, the default value can be derived from the value of another
field. This allows the provided key to act as the default/fallback value for the
field.

| `dto.mqtt.bind.port` | `dto.mqtt.client.port` | `reader.mqtt.bind.port` | `reader.mqtt.client.port` |
| -------------------- | ---------------------- | ----------------------- | ------------------------- |
| `None`               | `None`                 | `1883`                  | `1883`                    |
| `Some(5678)`         | `None`                 | `5678`                  | `5678`                    |
| `None`               | `Some(1234)`           | `1883`                  | `1234`                    |
| `Some(5678)`         | `Some(1234)`           | `1234`                  | `1234`                    |

### <a name="from-opt-key"></a> Optional fallback keys `#[tedge_config(default(from_optional_key = "..."))]`
```rust
# use tedge_config_macros::*;
# #[derive(::thiserror::Error, Debug)]
# pub enum ReadError { #[error(transparent)] NotSet(#[from] ConfigNotSet)}
#
define_tedge_config! {
  c8y: {
    url: ConnectUrl,

    /// The endpoint used for HTTP communication with Cumulocity
    #[tedge_config(note = "This will fall back to the value of 'c8y.url' if it is not set")]
    #[tedge_config(default(from_optional_key = "c8y.url"))]
    http: ConnectUrl,
  }
}

let mut dto = TEdgeConfigDto::default();
let reader = TEdgeConfigReader::from_dto(&dto, &TEdgeConfigLocation::default());
assert_eq!(reader.c8y.url, reader.c8y.http);

let not_set_err = reader.c8y.http.or_config_not_set().unwrap_err();
assert!(not_set_err.to_string().contains("'c8y.url'"));

dto.c8y.url = Some("test.cumulocity.com".parse().unwrap());
let reader = TEdgeConfigReader::from_dto(&dto, &TEdgeConfigLocation::default());
assert_eq!(reader.c8y.url, reader.c8y.http);

```

Using `from_optional_key`, the default value can be derived from the value of an
optional field (i.e. a field without a default value of its own).

The type of the resulting reader field will be
[`OptionalConfig`](crate::OptionalConfig), with `Empty("fallback.key.here")` if
the value is not set.

| `dto.c8y.url`         | `dto.c8y.http`             | `reader.c8y.url`         | `reader.c8y.http`             |
| --------------------- | -------------------------- | ------------------------ | ----------------------------- |
| `None`                | `None`                     | `Empty("c8y.url")`       | `Empty("c8y.url")`            |
| `Some("example.com")` | `None`                     | `Present("example.com")` | `Present("example.com")`      |
| `None`                | `Some("http.example.com")` | `Empty("c8y.url")`       | `Present("http.example.com")` |
| `Some("example.com")` | `Some("http.example.com")` | `Present("example.com")` | `Present("http.example.com")` |

### <a name="default-fn"></a> Default functions: `#[tedge_config(default(function = "..."))]`
```rust
# use tedge_config_macros::*;
# #[derive(::thiserror::Error, Debug)]
# pub enum ReadError { #[error(transparent)] NotSet(#[from] ConfigNotSet)}
use std::num::NonZeroU16;
use camino::Utf8PathBuf;

define_tedge_config! {
  device: {
    #[tedge_config(default(function = "default_device_cert_path"))]
    #[doku(as = "std::path::PathBuf")]
    cert_path: Utf8PathBuf,
  },
  mqtt: {
    bind: {
      #[tedge_config(default(function = "default_mqtt_port"))]
      #[doku(as = "u16")]
      port: NonZeroU16,

      #[tedge_config(default(function = "|| NonZeroU16::try_from(1883).unwrap()"))]
      #[doku(as = "u16")]
      another_port: NonZeroU16,
    },
  },
}

fn default_device_cert_path(location: &TEdgeConfigLocation) -> Utf8PathBuf {
    location
        .tedge_config_root_path()
        .join("device-certs")
        .join("tedge-certificate.pem")
}

fn default_mqtt_port() -> NonZeroU16 {
    NonZeroU16::try_from(1883).unwrap()
}

```

The `function` sub-attribute allows a function to be supplied as the source for
a default value. The function can be supplied as either the name of a function,
or as a closure (both are demonstrated above). Through the magic of
[`TEdgeConfigDefault`], these functions can depend on `&TEdgeConfigDto`,
`&TEdgeConfigLocation`, neither, or both. The relevant values will be
automatically passed in.


## <a name="readonly"></a> Read only settings: `#[tedge_config(readonly(...))]`
The `readonly` attribute marks a field as read only, and has two required sub
attributes:

- `#[tedge_config(readonly(function = "..."))]` specifies the function that is
  called to populate the field in the reader, this is called lazily using
  [`once_cell::sync::Lazy`]
- `#[tedge_config(readonly(write_error = "..."))]` specifies the error that will
  be displayed if someone attempts to `tedge config set` the field

```rust
use tedge_config_macros::*;

#[derive(::thiserror::Error, Debug)]
pub enum ReadError {
    #[error(transparent)]
    NotSet(#[from] ConfigNotSet),

    #[error("Couldn't read certificate")]
    CertificateParseFailure(#[from] Box<dyn std::error::Error>),
}

define_tedge_config! {
  device: {
    #[tedge_config(readonly(
      function = "try_read_device_id",
      write_error = "This setting is derived from the device certificate and is therefore read only.",
    ))]
    #[doku(as = "String")]
    id: Result<String, ReadError>,
  }
}

fn try_read_device_id(_reader: &TEdgeConfigReader) -> Result<String, ReadError> {
    unimplemented!()
}
```
The `function` sub attribute is more restrictive than
[`default(function)`](#default-fn). The function must be passed in by name, and
must have a single argument of type `&TEdgeConfigReader`.
