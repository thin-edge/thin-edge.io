
# Configuration of the dummy plugin

Build the plugin:

    $ cargo build --bin tedge_dummy_plugin

On every invocation, the dummy plugin will respond with the
contents of file list-valid.<return_code> . Where the return code
defines the intended return code of the invocation.

Upon manual execution, this path here will be used:

    .tedge_dummy_plugin/list-valid.0

When executed via the software management agent, this path will be used:

    /tmp/.tedge_dummy_plugin/list-valid.0

## Emulation of fruits

The current trick is just to use a static configuration to emulate a functional
plugin that lets us install and de-install fruits.
Right now, the apt plugin only cares about the returned text on the list command.
So we just insert a static list and base tests on that.

Note that even when a package is de-installed it will still appear in the list
as the response is static.

**Usage:**

Copy the dummy plugin to this path:

    /etc/tedge/sm-plugins/fruits

    sudo cp target/debug/tedge_dummy_plugin /etc/tedge/sm-plugins/fruits

Copy file list-valid.0 to the following place to use it as a plugin:

    /tmp/.tedge_dummy_plugin/list-valid.0

    $ sudo cp tests/PySys/software-management-end-to-end/dummy-plugin-configuration/list-valid.0 /tmp/.tedge_dummy_plugin/list-valid.0

Otherwise, the file needs to be in the current directory under ´.tedge_dummy_plugin/list-valid.0´.

Manual invocation e.g.:

    $ /etc/tedge/sm-plugins/fruits list
