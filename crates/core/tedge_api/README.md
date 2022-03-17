# Thin Edge API

thin-edge is made up out of "Plugins"[^1] which pass messages to eachother.
These plugins run on a "Core", which handles the message passing.
This crate defines the interfaces a plugin author needs to implement so that a
plugin can be built into thin-edge.

[^1]: Name is subject to change.

