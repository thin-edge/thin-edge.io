# plugin_httpstop

A Plugin that spins up a _very_ minimal web server, that when accessed stops
thin-edge.

This plugin is for showcasing how such a thing would be implemented.


## Configuration

The configuration of this plugin only supports one (self-explanatory) setting:

```toml
bind = "127.0.0.1:8080"
```
