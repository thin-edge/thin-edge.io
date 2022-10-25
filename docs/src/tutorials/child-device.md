Declare supported configuration list to thin-edge

The supported configuration list should be sent to thin-edge during the startup/bootstrap phase of the child device agent. This bootstrapping is a 3 step process:

    Prepare a c8y-configuration-plugin.toml file with the supported configuration list
    Upload this file to thin-edge via HTTP
    Notify thin-edge about the upload via MQTT

The child device agent needs to capture the list of configuration files that needs be managed from the cloud in a c8y-configuration-plugin.toml file in the same format as specified in the configuration management documentation as follows:

files = [
    { path = '/path/to/some/config', type = 'config1'},
    { path = '/path/to/another/config', type = 'config2'},
]

    path is the full path to the configuration file on the child device file system.
    type is a unique alias for each file entry which will be used to represent that file in Cumulocity UI

The child device agent needs to upload this file to thin-edge with an HTTP PUT request to the URL: http://{tedge-ip}:8000/tedge/file-transfer/{child-id}/c8y-configuration-plugin

    {tedge-ip} is the IP of the thin-edge device which is configured as mqtt.external.bind_address or mqtt.bind_address or 127.0.0.1 if neither is configured.
    {child-id} is the child-device-id

Once the upload is complete, the agent should notify thin-edge about the upload by sending the following MQTT message:

Topic:

tedge/{child-d}/commands/res/config_snapshot

Payload:

{ "type": "c8y-configuration-plugin”, "path": ”/child/local/fs/path” }
