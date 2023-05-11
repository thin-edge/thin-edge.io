Defines the necessary structures to create a tedge config struct

# Output
This macro outputs a few different types:
- `TEdgeConfigDto` ([example](example::TEdgeConfigDto)) --- A data-transfer
  object, used for reading and writing to toml
- `TEdgeConfigReader` ([example](example::TEdgeConfigReader)) --- A struct to
  read configured values from, populating values with defaults if they exist
- `ReadableKey` ([example](example::ReadableKey)) --- An enum of all the
  possible keys that can be read from the configuration, with
  [FromStr](std::str::FromStr) and [Display](std::fmt::Display) implementations.
- `WritableKey` ([example](example::WritableKey)) --- An enum of all the
  possible keys that can be written to the configuration, with
  [FromStr](std::str::FromStr) and [Display](std::fmt::Display) implementations.
- `ReadOnlyKey` ([example](example::ReadOnlyKey)) --- An enum of all the
  possible keys that can be read from, but not written to, the configuration,
  with [FromStr](std::str::FromStr) and [Display](std::fmt::Display)
  implementations.

# Example

# Attributes

## Naming

## Documentation
- Doc comments for fields are automatically converted to documentation in `tedge
  config list --doc`. All line breaks are removed before this. They are also
  preserved on the generated `Reader` and `Dto` structs.
- Doc comments for groups are preserved on the `Reader` and `Dto` structs, and
  ignored by `tedge config list --doc`.
- Examples can be added using `#[tedge_config(example = "value")]`. The value
  provided must be a string literal. A test will automatically be generated for
  each example field to check that the provided value can be deserialised to the
  field's type (i.e. the example would work if used with `tedge config set`)
- Additional notes about fields can be added using `#[tedge_config(note =
  "content")]`. This will be added to `tedge config list --doc` on a seperate
  line, with a coloured heading to make it more distinctive.

## Defaults

## Read only