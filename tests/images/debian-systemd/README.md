# debian-systemd container

This folder contains the container definition which is used within the integration tests (when using the `docker` device test adapter). The image provides a `systemd` enabled multi-process container so that it can simulate a real device as much as possible.

To enable `systemd` inside the container, the container needs to be launched with elevated privileges, something that is not so "normal" in the container world. For this reason, it is recommended that this image only be used for development and testing purposes and to run on a machine that is not publicly accessible (or running in a sufficiently sand-boxed environment).
